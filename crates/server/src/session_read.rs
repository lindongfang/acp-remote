//! `session.read` 的共享投影（Sync 与 Node Link 两个入站适配器共用同一套 core → wire 映射）。
//!
//! 本模块**不是**第三个入站适配器：它没有自己的连接、授权或 feature 词表，只接收调用方已经完成的
//! 门控结果（`SessionReadRequest` 的布尔字段），把 core 的 [`HistoryPage`] 投影成 `sync-protocol` 的
//! `sessionReadResult`。两个协议面因此不会出现第二套「用户输入 + Agent 正文」合并规则
//! （`SYNC_PROTOCOL.md` §11.5、`NODE_LINK_PROTOCOL.md` §12.7/§12.10）。
//!
//! 依赖方向：`server::sync` 与 `server::node_link` 都只依赖本模块；本模块只依赖 `core::use_cases` /
//! `core::ports` 与 `sync-protocol`，不调用任何一个适配器。
//!
//! **正文只在内存中**：返回的 `SessionReadResult` 由调用方直接编码进响应帧；本模块不写数据库、临时文件
//! 或日志，也不缓存结果。

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use acp_core::model::{
    AcpRaw as CoreAcpRaw, Actor, ConfigOption as CoreConfigOption, ConfigOptionKind, ConfigValue,
    EffectiveInteraction as CoreEffectiveInteraction, InteractionKind as CoreInteractionKind,
    MessageChannel as CoreMessageChannel, MessageId, MessageRecord,
    MessageState as CoreMessageState, PartialReason as CorePartialReason,
    PartialResult as CorePartialResult, PortError, RawUnavailableReason, Sequence as CoreSequence,
    SessionId, Timestamp as CoreTimestamp,
};
use acp_core::ports::{
    HistoryInclude, HistoryPage, HistoryQuery, ImportedRead, ImportedReadQuery, MessageCursor,
    OwnedEventRecord, ReplayLimit,
};
use acp_core::use_cases::UseCases;
use async_trait::async_trait;
use sync_protocol::common::{
    AgentContentBlock, Base64Url, ConfigOption as WireConfigOptionEntry, ConfigOptionType,
    ConfigOptionValue, ConfigOptionView as WireConfigOptionView, DecimalString,
    InteractionOption as WireInteractionOption, NonEmptyText, Nullable, RawAcp,
    RawAcpUnavailableReason, RawObject, Text, Timestamp, Uuid,
};
use sync_protocol::event::EventType;
use sync_protocol::media::{
    EffectiveInteraction as WireEffectiveInteraction, InteractionKind as WireInteractionKind,
    MessageChannel as WireMessageChannel, MessageState as WireMessageState,
    PartialReason as WirePartialReason, PartialResult as WirePartialResult,
};
use sync_protocol::sync::{MessageRole, SnapshotItemHistory, SnapshotItemMessage};
use sync_protocol::views::{View, project};
use tracing::debug;

/// `session.read` 单页消息数的服务端上限（客户端给的 `limit` 收敛到它；§11.5「超过服务端为该会话配置的
/// 明细条数上限时，服务端收敛到自己的上限并照常返回」）。
pub(crate) const MAX_PAGE_MESSAGES: u32 = 200;

/// `session.read` 省略 `limit` 时的默认页大小（§11.5：默认页是最近 20 条用户输入）。
pub(crate) const DEFAULT_PAGE_MESSAGES: u32 = 20;

/// 单次 `session.read` 扫描事件的上限（超出即显式失败）。
const MAX_SCAN_EVENTS: u64 = 20_000;

/// 单次 `session.read` 读取 Agent 正文行的上限（合并页覆盖到页边界所需；超出即显式失败）。
const MAX_SCAN_MESSAGES: usize = 20_000;

/// 扫描事件时的单批取行数（与 §14 的 `maxReplayEventsPerBatch` 同量级）。
const SCAN_BATCH_EVENTS: u32 = 2_000;

/// 内嵌 ACP 原文的单条上限（§10.1/§11.5：超过即 `rawAcp.unavailable{size_limit}`）。
const MAX_INLINE_ACP_BYTES: usize = 256 * 1024;

/// 单个文本块的字符上限（`common.schema.json#/$defs/agentContentBlock` 的 `text.maxLength`）。
const MAX_TEXT_CHARS: usize = 262_144;

/// `file.changed` 的事件类型（`core.file-diff.v1` 门控只作用于它）。
const FILE_CHANGED_EVENT: &str = "file.changed";

/// `file.changed` 视图里受 `core.file-diff.v1` 门控的两个顶层键。
const FILE_DIFF_VIEW_KEYS: [&str; 2] = ["diffLines", "diffUnavailable"];

/// 调用方已经完成的 feature 门控与分页参数。
#[derive(Debug, Clone)]
pub(crate) struct SessionReadRequest {
    pub session: SessionId,
    pub include: HistoryInclude,
    pub before_history: Option<CoreSequence>,
    pub before_messages: Option<MessageCursor>,
    /// 消息页大小（已由调用方按自己的协议默认值与上限收敛）。
    pub page_limit: u32,
    /// `core.typed-media.v1` / `node-link.typed-media.v1` 的协商结果。
    pub typed_media: bool,
    /// `core.raw-payload.v1` / `node-link.raw-acp.v1` 的协商结果。
    pub negotiate_acp: bool,
    /// `core.file-diff.v1` 的协商结果（Node Link 没有对应 feature，恒为 `true`）。
    pub negotiate_file_diff: bool,
}

/// 共享投影的失败分类：两个适配器各自映射成自己的 wire 错误码。
#[derive(Debug)]
pub(crate) enum SessionReadFault {
    Schema(&'static str),
    CursorExpired,
    TooLarge(&'static str),
    Internal(&'static str),
    Port(PortError),
}

/// `session.read` 的完成结果：owned 会话由本机事件日志投影；imported 会话是 Owner 结果的原文，
/// 由调用方按本地 feature 协商做最后一道门控（Access 侧不解释正文）。
#[derive(Debug)]
pub(crate) enum SessionReadOutcome {
    Owned(sync_protocol::command::SessionReadResult),
    Imported(ImportedRead),
}

/// `session.read` 依赖的用例读面。
///
/// 生产实现就是 [`UseCases`]；单独抽出来的理由是 **Agent 正文的多页补齐循环**只能在一个返回多页
/// `messages` 的读面上被完整驱动——真实存储无法在同一测试里构造出三页以上的确定分页。
#[async_trait]
pub(crate) trait SessionReadSource: Send + Sync {
    async fn read_session(
        &self,
        actor: &Actor,
        query: HistoryQuery,
    ) -> Result<HistoryPage, PortError>;

    async fn effective_interactions(
        &self,
        actor: &Actor,
        reference: &acp_core::model::SessionReference,
    ) -> Result<Vec<CoreEffectiveInteraction>, PortError>;

    /// imported 会话的在线回源；默认没有 imported 索引（测试替身与 Node Link 的 Owner 面）。
    async fn read_imported(
        &self,
        _actor: &Actor,
        _session: &SessionId,
        _query: ImportedReadQuery,
    ) -> Result<Option<ImportedRead>, PortError> {
        Ok(None)
    }
}

#[async_trait]
impl SessionReadSource for UseCases {
    async fn read_session(
        &self,
        actor: &Actor,
        query: HistoryQuery,
    ) -> Result<HistoryPage, PortError> {
        UseCases::read_session(self, actor, query).await
    }

    async fn effective_interactions(
        &self,
        actor: &Actor,
        reference: &acp_core::model::SessionReference,
    ) -> Result<Vec<CoreEffectiveInteraction>, PortError> {
        UseCases::effective_interactions(self, actor, reference).await
    }

    async fn read_imported(
        &self,
        actor: &Actor,
        session: &SessionId,
        query: ImportedReadQuery,
    ) -> Result<Option<ImportedRead>, PortError> {
        UseCases::read_imported_session(self, actor, session, query).await
    }
}

#[async_trait]
impl<T: SessionReadSource> SessionReadSource for Arc<T> {
    async fn read_session(
        &self,
        actor: &Actor,
        query: HistoryQuery,
    ) -> Result<HistoryPage, PortError> {
        T::read_session(self.as_ref(), actor, query).await
    }

    async fn effective_interactions(
        &self,
        actor: &Actor,
        reference: &acp_core::model::SessionReference,
    ) -> Result<Vec<CoreEffectiveInteraction>, PortError> {
        T::effective_interactions(self.as_ref(), actor, reference).await
    }

    async fn read_imported(
        &self,
        actor: &Actor,
        session: &SessionId,
        query: ImportedReadQuery,
    ) -> Result<Option<ImportedRead>, PortError> {
        T::read_imported(self.as_ref(), actor, session, query).await
    }
}

/// 共享投影入口：调用方完成授权、feature 门控与参数形状校验后调用。
///
/// owned 会话走本机事件日志；`NotFound` 时再问 imported 索引（与 `session_files_root` 的判定同序）——
/// 命中则把 Owner 的完成结果原文原样返回，未命中才把 `NotFound` 交回调用方。
pub(crate) async fn read_session<S: SessionReadSource>(
    core: &S,
    actor: &Actor,
    request: SessionReadRequest,
) -> Result<SessionReadOutcome, SessionReadFault> {
    let session_id = request.session.clone();
    let include = request.include;
    let page_limit = request.page_limit.clamp(1, MAX_PAGE_MESSAGES);
    let before_history = request.before_history;
    let before_messages = request.before_messages.clone();
    let typed_media = request.typed_media;
    let owned = core
        .read_session(
            actor,
            HistoryQuery {
                session: session_id.clone(),
                include,
                after: None,
                before_history,
                before_messages: before_messages.clone(),
                messages_limit: ReplayLimit::new(if include.messages {
                    MAX_PAGE_MESSAGES
                } else {
                    0
                }),
                limit: ReplayLimit::new(page_limit),
            },
        )
        .await;
    let mut batch = match owned {
        Ok(batch) => batch,
        Err(PortError::NotFound(_)) => {
            let imported = core
                .read_imported(
                    actor,
                    &session_id,
                    ImportedReadQuery {
                        include,
                        before_history,
                        before_messages,
                        messages_limit: ReplayLimit::new(if include.messages {
                            MAX_PAGE_MESSAGES
                        } else {
                            0
                        }),
                        limit: ReplayLimit::new(page_limit),
                    },
                )
                .await
                .map_err(SessionReadFault::Port)?;
            return match imported {
                Some(page) => Ok(SessionReadOutcome::Imported(page)),
                None => Err(SessionReadFault::Port(PortError::NotFound(
                    acp_core::model::EntityRef::Session(session_id),
                ))),
            };
        }
        Err(error) => return Err(SessionReadFault::Port(error)),
    };
    let config_options = std::mem::take(&mut batch.config);
    let history = std::mem::take(&mut batch.history);
    let history_has_earlier = batch.history_has_earlier;
    let mut owned_messages = std::mem::take(&mut batch.messages);
    let mut owned_has_earlier = batch.messages_has_earlier;
    let cursor = before_messages.as_ref().and_then(|cursor| {
        Uuid::parse(cursor.message_id.as_str())
            .ok()
            .map(|message_id| (cursor.created_at.as_str().to_owned(), message_id))
    });
    let mut merger = MessageMerger::new(cursor.clone());
    let mut scanned = 0_u64;
    let mut complete = false;
    loop {
        if include.messages {
            for record in &batch.events {
                merger.absorb(record);
            }
            scanned += batch.events.len() as u64;
        }
        let Some(next) = batch.next.clone() else {
            complete = true;
            break;
        };
        if !include.messages {
            complete = true;
            break;
        }
        if scanned >= MAX_SCAN_EVENTS {
            break;
        }
        batch = core
            .read_session(
                actor,
                HistoryQuery {
                    session: session_id.clone(),
                    include: HistoryInclude {
                        messages: true,
                        ..HistoryInclude::default()
                    },
                    after: Some(next),
                    before_history: None,
                    before_messages: None,
                    messages_limit: ReplayLimit::new(0),
                    limit: ReplayLimit::new(SCAN_BATCH_EVENTS),
                },
            )
            .await
            .map_err(SessionReadFault::Port)?;
    }
    if !complete {
        debug!(
            event = "session_read_window_exceeded",
            session_id = session_id.as_str(),
            scanned,
            "the session history exceeds the readable window of a single read"
        );
        return Err(SessionReadFault::TooLarge(
            "the session history exceeds the readable window of a single read",
        ));
    }
    let mut agent_has_earlier = false;
    if include.messages {
        let floor = merger.floor(page_limit);
        let mut fetched = owned_messages.len();
        let mut oldest = oldest_message(&owned_messages);
        while owned_has_earlier && needs_older_agents(floor.as_ref(), oldest.as_ref()) {
            if fetched >= MAX_SCAN_MESSAGES {
                debug!(
                    event = "session_read_messages_window_exceeded",
                    session_id = session_id.as_str(),
                    fetched,
                    "the agent messages exceed the readable window of a single read"
                );
                return Err(SessionReadFault::TooLarge(
                    "the session messages exceed the readable window of a single read",
                ));
            }
            let Some(cursor) = oldest.clone() else {
                return Err(SessionReadFault::Internal(
                    "the message page cannot advance",
                ));
            };
            let before_messages = Some(MessageCursor {
                created_at: CoreTimestamp::new(cursor.0.as_str())
                    .map_err(|_| SessionReadFault::Internal("a message cursor is invalid"))?,
                message_id: MessageId::new(cursor.1.as_str())
                    .map_err(|_| SessionReadFault::Internal("a message cursor is invalid"))?,
            });
            batch = core
                .read_session(
                    actor,
                    HistoryQuery {
                        session: session_id.clone(),
                        include: HistoryInclude {
                            messages: true,
                            ..HistoryInclude::default()
                        },
                        after: None,
                        before_history: None,
                        before_messages,
                        messages_limit: ReplayLimit::new(MAX_PAGE_MESSAGES),
                        limit: ReplayLimit::new(0),
                    },
                )
                .await
                .map_err(SessionReadFault::Port)?;
            let earlier = std::mem::take(&mut batch.messages);
            let Some(earlier_oldest) = oldest_message(&earlier) else {
                return Err(SessionReadFault::Internal(
                    "the message page cannot advance",
                ));
            };
            fetched += earlier.len();
            oldest = Some(earlier_oldest);
            owned_messages.extend(earlier);
            owned_has_earlier = batch.messages_has_earlier;
        }
        agent_has_earlier =
            owned_has_earlier && needs_older_agents(floor.as_ref(), oldest.as_ref());
    }
    let agents = owned_messages
        .iter()
        .map(|record| Message::from_record(record, typed_media))
        .collect::<Option<Vec<_>>>()
        .ok_or(SessionReadFault::Internal(
            "a persisted message cannot be projected",
        ))?;
    let page = merger.finish(page_limit, agents, agent_has_earlier);
    if include.messages && page.cursor_expired {
        debug!(
            event = "session_read_cursor_expired",
            session_id = session_id.as_str(),
            "the message cursor precedes the readable window"
        );
        return Err(SessionReadFault::CursorExpired);
    }
    let wire_session_id = Uuid::parse(session_id.as_str())
        .map_err(|_| SessionReadFault::Schema("sessionId is not a uuid"))?;
    let messages = if include.messages {
        Some(
            page.messages
                .iter()
                .map(|message| message.to_wire(&wire_session_id))
                .collect::<Option<Vec<_>>>()
                .ok_or(SessionReadFault::Internal("a message cannot be projected"))?,
        )
    } else {
        None
    };
    let pending_interactions = if include.pending_interactions {
        let reference = acp_core::model::SessionReference::Owned(
            acp_core::model::OwnedSessionRef::new(session_id.clone()),
        );
        let effective = core
            .effective_interactions(actor, &reference)
            .await
            .map_err(SessionReadFault::Port)?;
        Some(
            effective
                .iter()
                .map(wire_effective_interaction)
                .collect::<Option<Vec<_>>>()
                .ok_or(SessionReadFault::Internal(
                    "a pending interaction cannot be projected",
                ))?,
        )
    } else {
        None
    };
    let history_items = if include.history {
        Some(
            history
                .iter()
                .map(|record| {
                    wire_history_item(
                        record,
                        request.negotiate_acp,
                        request.negotiate_file_diff,
                        typed_media,
                    )
                })
                .collect::<Option<Vec<_>>>()
                .ok_or(SessionReadFault::Internal(
                    "a history entry cannot be projected",
                ))?,
        )
    } else {
        None
    };
    let config = if include.config_options {
        Some(
            config_options
                .iter()
                .map(wire_config_option)
                .collect::<Option<Vec<_>>>()
                .ok_or(SessionReadFault::Internal(
                    "a config option cannot be projected",
                ))?,
        )
    } else {
        None
    };
    Ok(SessionReadOutcome::Owned(
        sync_protocol::command::SessionReadResult {
            session_id: wire_session_id,
            resources: sync_protocol::command::SessionReadResources {
                messages,
                turns: None,
                pending_interactions,
                config_options: config,
                capabilities: None,
                history: history_items,
            },
            has_earlier: page.has_earlier,
            history_has_earlier: include.history.then_some(history_has_earlier),
        },
    ))
}

/// 把 Owner 的 `session.read` 结果原文投影成本地 wire 结果：只做**本地 feature 门控**，不重新解释正文。
///
/// 未协商 `core.typed-media.v1`/`core.raw-payload.v1`/`core.file-diff.v1` 时，按与 owned 投影完全相同的
/// 规则去键/降级（`gate_typed_media`/`degrade_blocks`/`gate_file_diff`）；请求里没有的 include 一律清空，
/// 保证 Access 的响应不会因为 Owner 协商了更多 feature 而向旧客户端泄露新字段。
pub(crate) fn project_imported_read(
    page: ImportedRead,
    session: &SessionId,
    include: HistoryInclude,
    typed_media: bool,
    negotiate_acp: bool,
    negotiate_file_diff: bool,
) -> Result<sync_protocol::command::SessionReadResult, SessionReadFault> {
    let mut result: sync_protocol::command::SessionReadResult =
        serde_json::from_str(page.document.as_str()).map_err(|_| {
            SessionReadFault::Internal("the owner returned a malformed session.read result")
        })?;
    if result.session_id.as_str() != session.as_str() {
        return Err(SessionReadFault::Internal(
            "the owner returned a result for another session",
        ));
    }
    if !include.messages {
        result.resources.messages = None;
    } else if let Some(messages) = result.resources.messages.as_mut() {
        for message in messages {
            if !typed_media {
                message.channel = None;
                message.partial = None;
                if let Ok(mut value) = serde_json::to_value(&message.content) {
                    degrade_blocks(&mut value);
                    if let Ok(degraded) = serde_json::from_value(value) {
                        message.content = degraded;
                    }
                }
            }
        }
    }
    if !include.pending_interactions {
        result.resources.pending_interactions = None;
    }
    if !include.config_options {
        result.resources.config_options = None;
    }
    result.resources.turns = None;
    result.resources.capabilities = None;
    if !include.history {
        result.resources.history = None;
        result.history_has_earlier = None;
    } else if let Some(history) = result.resources.history.as_mut() {
        for item in history {
            let view = gate_file_diff(item.event_type.as_str(), &item.view, negotiate_file_diff)
                .unwrap_or_else(|| item.view.clone());
            let view = gate_typed_media(&view, typed_media).unwrap_or(view);
            item.view = view;
            if !negotiate_acp {
                item.acp = None;
            }
        }
    }
    Ok(result)
}

/// 十进制串（序号/版本/长度在 wire 上的形状）。
fn decimal(text: &str) -> Option<DecimalString> {
    DecimalString::parse(text).ok()
}

/// core 的 `ConfigOption` → wire `configOptionView`。
pub(crate) fn wire_config_option(option: &CoreConfigOption) -> Option<WireConfigOptionView> {
    let current_value = match option.current() {
        ConfigValue::Text(text) => ConfigOptionValue::Text(text.as_str().to_owned()),
        ConfigValue::Select(select) => ConfigOptionValue::Text(select.as_str().to_owned()),
        ConfigValue::Boolean(flag) => ConfigOptionValue::Boolean(*flag),
    };
    let candidates = option
        .options()
        .iter()
        .map(|entry| {
            Some(WireConfigOptionEntry {
                value: NonEmptyText::<256>::parse(entry.value().as_str()).ok()?,
                name: NonEmptyText::<256>::parse(entry.name()).ok()?,
                description: match entry.description() {
                    Some(text) => Nullable::value(Text::<1024>::parse(text).ok()?),
                    None => Nullable::null(),
                },
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(WireConfigOptionView {
        id: NonEmptyText::<256>::parse(option.id().as_str()).ok()?,
        name: NonEmptyText::<256>::parse(option.name()).ok()?,
        description: match option.description() {
            Some(text) => Nullable::value(Text::<1024>::parse(text).ok()?),
            None => Nullable::null(),
        },
        category: match option.category() {
            Some(text) => Nullable::value(Text::<128>::parse(text).ok()?),
            None => Nullable::null(),
        },
        option_type: match option.kind() {
            ConfigOptionKind::Select => ConfigOptionType::Select,
            ConfigOptionKind::Boolean => ConfigOptionType::Boolean,
        },
        current_value,
        // 空候选在 wire 上是**键缺席**（§11.5 的 `options` 不在 required 里），不是空数组。
        options: if candidates.is_empty() {
            None
        } else {
            Some(candidates)
        },
    })
}

/// 一条结构化历史条目 → wire `snapshotItem.history`（与 `event` 帧的 `payload` 同形：同一个 `view` 原文、
/// 同一条 `acp.raw-payload.v1` 门控，以及同一条 `core.file-diff.v1` 门控）。
fn wire_history_item(
    record: &OwnedEventRecord,
    negotiate_acp: bool,
    negotiate_file_diff: bool,
    negotiate_typed_media: bool,
) -> Option<SnapshotItemHistory> {
    let event = &record.event;
    let sequence = event.session_sequence.as_ref()?;
    let view = RawObject::parse(record.payload.view.as_str()).ok()?;
    // `view` 是库里原文（`payload_json`）：未协商差异行时只在 `file.changed` 上去掉那两个键，其余字节
    // 原样（不解析、不重新序列化）。
    let view =
        gate_file_diff(event.event_type.as_str(), &view, negotiate_file_diff).unwrap_or(view);
    // 与事件帧同一条类型化媒体门控：未协商 `core.typed-media.v1` 时历史条目也不得出现新键/新块。
    let view = gate_typed_media(&view, negotiate_typed_media).unwrap_or(view);
    Some(SnapshotItemHistory {
        event_id: Uuid::parse(event.id.as_str()).ok()?,
        event_type: EventType::parse(event.event_type.as_str()).ok()?,
        session_sequence: decimal(&sequence.get().to_string())?,
        created_at: Timestamp::parse(event.created_at.as_str()).ok()?,
        view,
        acp: if negotiate_acp {
            wire_raw_acp(record.payload.acp.as_ref())?
        } else {
            None
        },
    })
}

/// core 的 `payload.acp` → wire 的 `rawAcp`（`Complete`/`Unavailable` 两个分支）。
///
/// 口径与 [`crate::sync::subscribe::event`] 的同名映射**逐条相同**（media type 固定 `application/json`、
/// 内嵌上限 256 KiB、超限改判 `size_limit`、不可用原因三种一一对应），因为那是 §10.1 的登记规则；本模块
/// **拿不到**那两个函数（它们是订阅模块的私有实现），而历史条目的 `acp` 必须与事件帧的 `acp` 同源同规则，
/// 因此这份规则在本目录里各自持有（代价与 `design.md` D1 登记的「256 KiB 两处常量」同一性质）。
fn wire_raw_acp(acp: Option<&CoreAcpRaw>) -> Option<Option<RawAcp>> {
    Some(match acp {
        Some(CoreAcpRaw::Available {
            media_type,
            raw_json,
            byte_length,
            sha256,
        }) => {
            let digest = Base64Url::<32>::parse(sha256.as_str()).ok()?;
            if media_type != "application/json" || raw_json.len() > MAX_INLINE_ACP_BYTES {
                Some(RawAcp::Unavailable {
                    reason: if media_type != "application/json" {
                        RawAcpUnavailableReason::StorageFailure
                    } else {
                        RawAcpUnavailableReason::SizeLimit
                    },
                    byte_length: decimal(&byte_length.to_string())?,
                    sha256: Nullable::value(digest),
                })
            } else {
                Some(RawAcp::Complete {
                    // `rawJson` 是 JSON **字符串**（内容是 ACP 原文档文本），因此编码一次字符串字面量；
                    // 绝不先解析再重新序列化（那会改写空白、键序与超出 64 位的整数字面量）。
                    raw_json: serde_json::value::RawValue::from_string(
                        serde_json::to_string(raw_json).ok()?,
                    )
                    .ok()?,
                    byte_length: decimal(&byte_length.to_string())?,
                    sha256: digest,
                })
            }
        }
        Some(CoreAcpRaw::Unavailable {
            reason,
            byte_length,
            sha256,
        }) => Some(RawAcp::Unavailable {
            reason: match reason {
                RawUnavailableReason::SizeLimit => RawAcpUnavailableReason::SizeLimit,
                RawUnavailableReason::RetentionExpired => RawAcpUnavailableReason::RetentionExpired,
                RawUnavailableReason::StorageFailure => RawAcpUnavailableReason::StorageFailure,
            },
            byte_length: decimal(&byte_length.to_string())?,
            sha256: match sha256 {
                Some(digest) => Nullable::value(Base64Url::<32>::parse(digest.as_str()).ok()?),
                None => Nullable::null(),
            },
        }),
        None => None,
    })
}

// ---------------------------------------------------------------------------------------------
// 消息页：从持久化事件折叠出一页 `resources.messages`
// ---------------------------------------------------------------------------------------------

/// 一条待投影的消息（取值只来自持久化事实，不重算、不补全）。
///
/// 两个来源：`user.message.delta` 事件（用户输入）与 `owned_message` 行（Agent 的回复与思考，§7.6）。
/// `channel`/`partial` 只在协商 `core.typed-media.v1` 时下发；未协商时保持 `None`，wire 上这两个键
/// **缺席**（不是 `null`）。内容块同样受该 feature 门控：未协商时 `audio_ref`/`file_ref` 整块去掉、
/// `image_ref` 去掉 `attachmentId`（与事件轨道的 `gate_typed_media` 同一套降级）。
#[derive(Debug, Clone)]
pub(crate) struct Message {
    pub(crate) message_id: Uuid,
    pub(crate) role: MessageRole,
    pub(crate) content: Vec<AgentContentBlock>,
    pub(crate) created_at: Timestamp,
    pub(crate) turn_id: Option<Uuid>,
    pub(crate) channel: Option<WireMessageChannel>,
    pub(crate) partial: Option<WirePartialResult>,
}

impl Message {
    /// `owned_message` 行 → 待投影消息。任一处形状不可表达即 `None`（调用方整页失败，不静默丢条目）。
    ///
    /// 未协商 `core.typed-media.v1` 时，`content` 也必须经**与事件轨道同一个**降级判定
    /// （`sync::event::degrade_blocks`）：旧 peer 的 `agentContentBlock` 是封闭词表且 `deny_unknown_fields`，
    /// 一条带 `audio_ref`/`file_ref`（或 `image_ref.attachmentId`）的消息会让整条 `session.read` 结果
    /// 解码失败——不是「少显示一张卡片」，而是整页错误。降级在解析成 wire 块之前做，因此三种键走的是
    /// 事件视图那条完全相同的规则（`§5.3` 的「未协商时新键与不可表达的内容块都不得出现」）。
    pub(crate) fn from_record(record: &MessageRecord, typed_media: bool) -> Option<Self> {
        let mut content_json: serde_json::Value =
            serde_json::from_str(record.content.as_str()).ok()?;
        if !typed_media {
            degrade_blocks(&mut content_json);
        }
        let content: Vec<AgentContentBlock> = serde_json::from_value(content_json).ok()?;
        let channel = match record.channel {
            CoreMessageChannel::Reply => WireMessageChannel::Reply,
            CoreMessageChannel::Thought => WireMessageChannel::Thought,
        };
        let partial = wire_partial(&record.result);
        Some(Self {
            message_id: Uuid::parse(record.message.as_str()).ok()?,
            role: MessageRole::Agent,
            content,
            created_at: Timestamp::parse(record.created_at.as_str()).ok()?,
            turn_id: record
                .turn
                .as_ref()
                .and_then(|turn| Uuid::parse(turn.as_str()).ok()),
            channel: typed_media.then_some(channel),
            partial: typed_media.then_some(partial),
        })
    }

    /// wire 的 `snapshotItem.messages` 元素。
    ///
    /// `status` 是 §11.5 未登记词表的开放串（`NonEmptyText<32>`，客户端不消费它）：取值表达**持久化日志
    /// 里该消息的正文状态**（`completed`/`partial`/`failed`/`cancelled`）。用户消息的来源是它自己的全部
    /// delta（发送即完整），Agent 消息的来源是 `owned_message.state`。
    fn to_wire(&self, session: &Uuid) -> Option<SnapshotItemMessage> {
        let status = match self.partial.as_ref().map(|partial| partial.state) {
            Some(WireMessageState::Completed) | None => "completed",
            Some(WireMessageState::Partial) => "partial",
            Some(WireMessageState::Failed) => "failed",
            Some(WireMessageState::Cancelled) => "cancelled",
        };
        Some(SnapshotItemMessage {
            message_id: self.message_id.clone(),
            session_id: session.clone(),
            role: self.role,
            content: self.content.clone(),
            status: NonEmptyText::<32>::parse(status).ok()?,
            created_at: self.created_at.clone(),
            turn_id: match self.turn_id.as_ref() {
                Some(turn) => Nullable::value(turn.clone()),
                None => Nullable::null(),
            },
            channel: self.channel,
            partial: self.partial.clone(),
        })
    }
}

/// core 的正文完整性 → wire 的 `partialResult`（`core.typed-media.v1`）。
fn wire_partial(result: &CorePartialResult) -> WirePartialResult {
    let state = match result.state {
        CoreMessageState::Completed => WireMessageState::Completed,
        CoreMessageState::Partial => WireMessageState::Partial,
        CoreMessageState::Failed => WireMessageState::Failed,
        CoreMessageState::Cancelled => WireMessageState::Cancelled,
    };
    let reason = match result.reason {
        Some(CorePartialReason::AgentStopped) => Nullable::value(WirePartialReason::AgentStopped),
        Some(CorePartialReason::AgentFailed) => Nullable::value(WirePartialReason::AgentFailed),
        Some(CorePartialReason::TurnCancelled) => Nullable::value(WirePartialReason::TurnCancelled),
        Some(CorePartialReason::ConnectionLost) => {
            Nullable::value(WirePartialReason::ConnectionLost)
        }
        Some(CorePartialReason::RetentionExpired) => {
            Nullable::value(WirePartialReason::RetentionExpired)
        }
        None => Nullable::null(),
    };
    WirePartialResult { state, reason }
}

/// core 的有效待处理交互 → wire 的 `effectiveInteraction`（§11.7）。
fn wire_effective_interaction(
    interaction: &acp_core::model::EffectiveInteraction,
) -> Option<WireEffectiveInteraction> {
    let kind = match interaction.kind {
        CoreInteractionKind::Permission => WireInteractionKind::Permission,
        CoreInteractionKind::Elicitation => WireInteractionKind::Elicitation,
    };
    let options = interaction
        .options
        .iter()
        .map(|option| {
            Some(WireInteractionOption {
                option_id: NonEmptyText::<128>::parse(option.option_id()).ok()?,
                label: NonEmptyText::<512>::parse(option.label()).ok()?,
                kind: NonEmptyText::<64>::parse(option.kind()).ok()?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let schema = match interaction.schema.as_ref() {
        Some(schema) => Nullable::value(RawObject::parse(schema.as_json()).ok()?),
        None => Nullable::null(),
    };
    Some(WireEffectiveInteraction {
        interaction_id: Uuid::parse(interaction.interaction.as_str()).ok()?,
        session_id: Uuid::parse(interaction.session.as_str()).ok()?,
        kind,
        request_version: DecimalString::parse(&interaction.request_version.get().to_string())
            .ok()?,
        expires_at: match interaction.expires_at.as_ref() {
            Some(expires_at) => Nullable::value(Timestamp::parse(expires_at.as_str()).ok()?),
            None => Nullable::null(),
        },
        options,
        schema,
    })
}

/// 把用户输入的事件折叠成消息页的一半（Agent 正文来自 `owned_message`，见模块文档）。
///
/// 折叠规则：
///
/// - `user.message.delta` 是用户输入的持久化载体：同一 `messageId` 的多个 delta 按 `deltaIndex` 升序拼成
///   文本，`createdAt` 取最早一条，`turnId` 取该消息的 turn；
/// - `agent.message.completed`/`agent.message.delta`/`agent.thought.delta` **不参与**：Agent 正文（含
///   思考与部分状态）的权威是 `owned_message`，事件只是流式增量与收尾收据，不能再造第二份正文。
#[derive(Debug)]
pub(crate) struct MessageMerger {
    /// 复合游标（`{createdAt, messageId}`）：只收集**严格早于**它的用户消息；`None` = 不按游标收窄。
    cursor: Option<(String, Uuid)>,
    users: BTreeMap<Uuid, UserMessage>,
    /// 已收下的事件 id：投递至少一次语义下同一条事件可能重复到达，正文只能应用一次。
    seen_events: BTreeSet<String>,
}

#[derive(Debug, Default)]
struct UserMessage {
    created_at: Option<Timestamp>,
    turn_id: Option<Uuid>,
    parts: Vec<(u64, String)>,
}

/// 一页消息 + 页标记（`cursor_expired` 见 [`MessageMerger::finish`]）。
pub(crate) struct MessagePage {
    pub(crate) messages: Vec<Message>,
    pub(crate) has_earlier: bool,
    pub(crate) cursor_expired: bool,
}

impl MessageMerger {
    pub(crate) fn new(cursor: Option<(String, Uuid)>) -> Self {
        Self {
            cursor,
            users: BTreeMap::new(),
            seen_events: BTreeSet::new(),
        }
    }

    /// 收下一条持久化事件（非用户输入的事件直接跳过）。
    ///
    /// 去重有两道：同一 `eventId` 的重复投递整体跳过；同一 `(messageId, deltaIndex)` 的重复增量也
    /// 只应用一次（重连后重放/重试可能换了 eventId，但内容位置相同）。两者都是「只应用一次」，不是
    /// 覆盖：历史与最终呈现因此不会因重复到达而出现两份正文。
    pub(crate) fn absorb(&mut self, record: &OwnedEventRecord) {
        if record.event.event_type.as_str() != "user.message.delta" {
            return;
        }
        if !self.seen_events.insert(record.event.id.as_str().to_owned()) {
            return;
        }
        let Ok(view_object) = RawObject::parse(record.payload.view.as_str()) else {
            return;
        };
        let Ok(view) = project(record.event.event_type.as_str(), &view_object) else {
            // 形状不符只记诊断并跳过该事件：`session.read` 是读取面，不因为一条坏行让整页失败
            // （事件轨道那一侧的显式降级由 `sync::event` 负责）。
            debug!(
                event = "sync.session_read_view_skipped",
                event_type = "user.message.delta",
                "the persisted view does not match the registered shape"
            );
            return;
        };
        if let View::UserMessageDelta(delta) = view {
            let created_at = Timestamp::parse(record.event.created_at.as_str()).ok();
            let entry = self.users.entry(delta.message_id.clone()).or_default();
            if entry.created_at.is_none() {
                entry.created_at = created_at;
            }
            entry.turn_id = Some(delta.turn_id.clone());
            let index = delta
                .delta_index
                .as_str()
                .parse::<u64>()
                .unwrap_or(u64::MAX);
            // 同一位置只应用一次：重复投递/换了 eventId 的重试都不会把同一段正文追加两遍。
            if entry.parts.iter().any(|(existing, _)| *existing == index) {
                return;
            }
            entry.parts.push((index, delta.text.as_str().to_owned()));
        }
    }

    /// 合并页的**排他下界**：按游标收窄后、从最新往前数第 `limit+1` 条用户消息的排序键。
    ///
    /// `finish` 会从尾部往前收，直到已含 `limit` 条用户输入；它停下的位置正好是这条消息之后。Agent 正文
    /// 的取数必须覆盖到该边界（含边界之后的全部 Agent 消息），否则合并页会缺条目。返回 `None` 表示用户
    /// 输入不超过一页——页从最早一条消息开始，Agent 正文也要读到最早。
    pub(crate) fn floor(&self, limit: u32) -> Option<(String, String)> {
        let cursor = self
            .cursor
            .as_ref()
            .map(|(created_at, message_id)| (created_at.clone(), message_id.as_str().to_owned()));
        let mut keys: Vec<(String, String)> = self
            .users
            .iter()
            .filter_map(|(message_id, user)| {
                let created_at = user.created_at.as_ref()?;
                let key = (
                    created_at.as_str().to_owned(),
                    message_id.as_str().to_owned(),
                );
                match &cursor {
                    // 只统计**严格早于**游标的用户消息（与 `finish` 的收窄口径一致）。
                    Some(cursor) if key >= *cursor => None,
                    _ => Some(key),
                }
            })
            .collect();
        keys.sort();
        if keys.len() <= limit as usize {
            return None;
        }
        keys.get(keys.len() - limit as usize - 1).cloned()
    }

    /// 折叠出一页：用户消息（本累加器）+ Agent 正文（`owned_message` 的一页）按 `createdAt` 升序合并
    /// （同一时刻以 `messageId` 稳定定序），只保留游标之前的消息，再取**最新**一页（页里最多 `limit` 条
    /// 用户输入，与 §11.5 的默认页口径一致）。
    ///
    /// `has_earlier` 同时看两半：合并页自身还有被切掉的前缀，或 `owned_message` 那一页本身还有更早行。
    ///
    /// 同时给出 `cursor_expired`：游标在场、这一页为空、两半都说没有更早内容、且游标**严格早于**本次读到
    /// 的**最早一条可读消息**（或本次一条可读消息都没有）时，唯一能解释「游标指向的东西不在这份读口里」
    /// 的物理事实就是它已被保留策略清理。比较必须是严格的：游标恰好等于最早一条可读消息是**合法**的。
    pub(crate) fn finish(
        self,
        limit: u32,
        agents: Vec<Message>,
        agent_has_earlier: bool,
    ) -> MessagePage {
        let MessageMerger { cursor, users, .. } = self;
        let mut messages: Vec<Message> = users
            .into_iter()
            .filter_map(|(message_id, user)| {
                let created_at = user.created_at?;
                let mut parts = user.parts;
                parts.sort_by_key(|(index, _)| *index);
                let text = parts.into_iter().map(|(_, text)| text).collect::<String>();
                Some(Message {
                    message_id,
                    role: MessageRole::User,
                    content: text_blocks(&text)?,
                    created_at,
                    turn_id: user.turn_id,
                    channel: None,
                    partial: None,
                })
            })
            .chain(agents)
            .collect();
        // `createdAt` 定宽（UTC 毫秒），字典序即时间序；同一时刻以 `messageId` 作稳定 tie-breaker。
        messages.sort_by(|left, right| {
            (left.created_at.as_str(), &left.message_id)
                .cmp(&(right.created_at.as_str(), &right.message_id))
        });
        // 可读窗口的下界（本次读口内最早的一条消息），必须在按游标收窄**之前**取。
        let earliest = messages.first().map(|message| {
            (
                message.created_at.as_str().to_owned(),
                message.message_id.clone(),
            )
        });
        messages.retain(|message| match &cursor {
            Some((created_at, message_id)) => {
                (message.created_at.as_str(), &message.message_id)
                    < (created_at.as_str(), message_id)
            }
            None => true,
        });
        // 最新一页：从尾部往前收，直到这一页已含 `limit` 条用户输入；页首之前的 Agent 消息留在页内
        // （它们属于这一页的 turn）。
        let mut start = messages.len();
        let mut users_in_page = 0_u32;
        while start > 0 {
            let candidate = &messages[start - 1];
            let is_user = candidate.role == MessageRole::User;
            if is_user && users_in_page == limit {
                break;
            }
            if is_user {
                users_in_page += 1;
            }
            start -= 1;
        }
        let has_earlier = start > 0 || agent_has_earlier;
        messages.drain(..start);
        let cursor_expired = match (&cursor, &earliest) {
            // 游标早于最早一条可读消息：它指向的行已不在读口内。
            (Some(cursor), Some(earliest)) => {
                messages.is_empty()
                    && !has_earlier
                    && (cursor.0.as_str(), &cursor.1) < (earliest.0.as_str(), &earliest.1)
            }
            // 一条可读消息都没有：任何游标都指向读口之外的区间。
            (Some(_), None) => true,
            (None, _) => false,
        };
        MessagePage {
            messages,
            has_earlier,
            cursor_expired,
        }
    }
}

/// `owned_message` 行的排序键（与合并页的游标同一口径：`(createdAt, messageId)`，升序）。
pub(crate) fn message_key(record: &MessageRecord) -> (String, String) {
    (
        record.created_at.as_str().to_owned(),
        record.message.as_str().to_owned(),
    )
}

/// 一页 `owned_message` 里最早一条的排序键（端口按 `(created_at, message_id)` 升序返回）。
fn oldest_message(records: &[MessageRecord]) -> Option<(String, String)> {
    records.first().map(message_key)
}

/// 是否还需要继续取更早的 Agent 正文页（[`session_read`] 的补齐循环）。
///
/// - 没有下界（用户输入不超过一页）→ 必须读到最早一条；
/// - 有下界 → 最早一条已**严格早于**下界时覆盖完成；否则（包括本页为空）继续往前取。
pub(crate) fn needs_older_agents(
    floor: Option<&(String, String)>,
    oldest: Option<&(String, String)>,
) -> bool {
    match (floor, oldest) {
        (None, _) | (Some(_), None) => true,
        (Some(floor), Some(oldest)) => oldest >= floor,
    }
}

/// 一段文本 → `agentContentBlock` 列表（单个块的文本上限是 262144 **字符**，超出即切分为多个块：既不
/// 丢字也不改写内容；空白串产生一个空文本块，与"消息存在但正文为空"一致）。
fn text_blocks(text: &str) -> Option<Vec<AgentContentBlock>> {
    let mut blocks = Vec::new();
    let mut chunk = String::new();
    for character in text.chars() {
        if chunk.chars().count() >= MAX_TEXT_CHARS {
            blocks.push(AgentContentBlock::Text {
                text: Text::<MAX_TEXT_CHARS>::parse(&chunk).ok()?,
            });
            chunk.clear();
        }
        chunk.push(character);
    }
    blocks.push(AgentContentBlock::Text {
        text: Text::<MAX_TEXT_CHARS>::parse(&chunk).ok()?,
    });
    Some(blocks)
}

/// 类型化媒体的投递侧门控：未协商 `core.typed-media.v1` 时把视图降级成旧 peer 能接受的形状。
///
/// 降级规则（§5.3「未协商时新键与不可表达的内容块都不得出现」）：
///
/// - 去掉 `agent.message.completed` 视图的 `channel`/`partial` 两个顶层键（它们只在协商后出现）；
/// - 去掉 `image_ref` 的 `attachmentId`（旧 peer 不认识附件标识，但图片引用本身是 v1 已有的）；
/// - 去掉 `audio_ref`/`file_ref` 两类内容块：旧 peer 没有对应卡片，保留原文的职责在 `acp.rawJson`
///   （协商了 `acp.raw-payload.v1` 时客户端仍能读到完整 ACP 内容）。
///
/// 只有真的触发了降级才返回新文本（否则返回 `None`，视图的字节与协商时逐字相同）。这一处与
/// `session.read` 的消息页共用同一个判定，旧连接的「history」条目与事件帧因此不会出现两种形状。
pub(crate) fn gate_typed_media(view: &RawObject, negotiated: bool) -> Option<RawObject> {
    if negotiated {
        return None;
    }
    let text = view.get();
    let needs_blocks = text.contains("\"audio_ref\"")
        || text.contains("\"file_ref\"")
        || text.contains("\"attachmentId\"");
    let needs_keys = text.contains("\"channel\"") || text.contains("\"partial\"");
    if !needs_blocks && !needs_keys {
        return None;
    }
    let mut value: serde_json::Value = serde_json::from_str(text).ok()?;
    if let Some(object) = value.as_object_mut() {
        object.remove("channel");
        object.remove("partial");
    }
    degrade_blocks(&mut value);
    let encoded = serde_json::to_string(&value).ok()?;
    RawObject::parse(&encoded).ok()
}

/// 递归降级内容块：`audio_ref`/`file_ref` 整块去掉，`image_ref` 去掉 `attachmentId`。
///
/// 返回 `false` 表示该值所在的数组元素应当被移除（只对内容块对象返回 `false`）。
///
/// `session.read` 的消息页在 `Message::from_record` 里对**内容块数组**调用它（未协商
/// `core.typed-media.v1` 时）：事件轨道与消息页因此共用同一套降级规则，而不是各写一份。
pub(crate) fn degrade_blocks(value: &mut serde_json::Value) -> bool {
    match value {
        serde_json::Value::Array(items) => {
            items.retain_mut(degrade_blocks);
            true
        }
        serde_json::Value::Object(object) => {
            match object.get("type").and_then(serde_json::Value::as_str) {
                Some("audio_ref" | "file_ref") => return false,
                Some("image_ref") => {
                    object.remove("attachmentId");
                    return true;
                }
                _ => {}
            }
            for item in object.values_mut() {
                degrade_blocks(item);
            }
            true
        }
        _ => true,
    }
}

/// `file.changed` 视图里差异行键的投递侧门控：未协商 `core.file-diff.v1` 时去掉
/// [`FILE_DIFF_VIEW_KEYS`] 两个顶层键；协商了、或不是 `file.changed` 时返回 `None`（视图原样）。
///
/// 视图的两条下发路径——事件帧（[`frame`]）与 `session.read` 的历史条目（`sync::command::query` 的
/// `snapshotItem.history`）——共用这一处判定与去键，因此「同一个视图在两条路径上按同一份协商结果去键」
/// 不会分叉。调用方各自给出「本连接是否协商了该 feature」（门控的输入只有连接知道）。
pub(crate) fn gate_file_diff(
    event_type: &str,
    view: &RawObject,
    negotiated: bool,
) -> Option<RawObject> {
    if negotiated || event_type != FILE_CHANGED_EVENT {
        return None;
    }
    // 视图文本由 core 的 `file_changed_view` 逐字符拼出（紧凑 JSON），去键只删掉键与它的值，其余字段的
    // 字节原样保留；文本不合形状时原样下发（判定失败不改变「这一条是否下发」，见模块文档的显式降级）。
    let text = without_keys(view.get(), &FILE_DIFF_VIEW_KEYS)?;
    RawObject::parse(&text).ok()
}

/// 从一个**顶层对象**的 JSON 文本里删掉给定的若干顶层键，其余字节逐字保留（键不存在时返回原文的
/// 拷贝）。
///
/// 为什么是文本级去键而不是「解析成 `serde_json::Value` 再序列化」：`payload.view` 的字节保真是本
/// 模块的硬规则（模块文档），而 `serde_json` 在没有 `preserve_order` 时会按字典序重排键、并按自己的
/// 规则重写数字与转义——那会让未协商的连接收到一份与协商时**其余字段逐字节不同**的视图（规格场景
/// 「其余字段与协商时逐字节一致」）。因此这里只做两件事：定位顶层键、按它的值跨度切掉，其余原样
/// 拼接（成员之间的空白与逗号按紧凑形式规范化，注册的生产者写出的视图本就是紧凑文本）。
///
/// 返回 `None` 表示这段文本不是「一个顶层对象」（调用方保留原文，不猜测）。
///
/// core 的 `file_changed_view` 今天把两个受门控的键写在**末位**、且写出的本就是紧凑文本，因此这个分支
/// 在实际输入上不出现——但那是**一个生产者的成员顺序**，不是契约：`event-views.schema.json#/$defs/
/// file.changed` 不约束成员顺序，将来多一个 `file.changed` 视图生产者、或既有生产者换个顺序，都是合法
/// 输入。而返回 `None` 的后果是调用方**保留原文**，也就是未协商 `core.file-diff.v1` 的连接照常收到那两个
/// 键（fail-open，不是拒绝），因此「删掉若干成员后剩下的成员仍然拼得对」必须对收到的**任何**顶层对象
/// 成立，不能依赖顺序。
fn without_keys(text: &str, keys: &[&str]) -> Option<String> {
    let bytes = text.as_bytes();
    let mut cursor = 0usize;
    skip_whitespace(bytes, &mut cursor);
    if bytes.get(cursor) != Some(&b'{') {
        return None;
    }
    cursor += 1;
    let mut out = String::with_capacity(text.len());
    out.push('{');
    // 两个标记各管一件事，**不能**合并成一个：`first` 只决定「本成员前要不要补逗号」，`expect_member`
    // 是**语法位置**（此处是否该出现一个成员）。被删的成员走的是 `continue`——它同样让「上一个成员」
    // 结束，因此只影响逗号的补充、不影响语法位置：成员末尾一律把 `expect_member` 置 `false`。合并成
    // 一个标记时（曾经的写法），删掉**首位**成员后那个标记不变，下一次循环读到的合法分隔逗号会被当成
    // 「开头就是逗号」而让整段文本判失败，调用方因此保留原文（未协商能力的连接照常收到被删键，
    // 即 fail-open）——见 `event/tests.rs` 的两种成员顺序用例。
    let mut first = true;
    let mut expect_member = true;
    loop {
        skip_whitespace(bytes, &mut cursor);
        let byte = *bytes.get(cursor)?;
        if byte == b'}' {
            out.push('}');
            return Some(out);
        }
        if byte == b',' {
            // 逗号只在成员之间出现：语法位置说「此处该出现成员」（对象开头、或紧跟在逗号之后）时，
            // 开头就是逗号不是合法对象。
            if expect_member {
                return None;
            }
            cursor += 1;
            // 逗号之后**必须**紧跟一个成员——下面就地强制（要求 `"` 开头），因此这里不需要（也没有读点）
            // 把 `expect_member` 置回 `true`：唯一能回到循环顶部并读到它的路径是「本成员解析成功」，
            // 而那条路径在成员末尾一律置 `false`。
        } else if !expect_member {
            // 成员之间必须有逗号（第一个成员前面没有）。
            return None;
        }
        skip_whitespace(bytes, &mut cursor);
        if bytes.get(cursor) != Some(&b'"') {
            return None;
        }
        let key_start = cursor;
        let key_end = scan_string(bytes, cursor)?;
        cursor = key_end;
        skip_whitespace(bytes, &mut cursor);
        if bytes.get(cursor) != Some(&b':') {
            return None;
        }
        cursor += 1;
        skip_whitespace(bytes, &mut cursor);
        let value_end = scan_value(bytes, cursor)?;
        cursor = value_end;
        // 本成员（无论保留还是删掉）到此结束：语法位置变成「刚消费完一个成员」，下一个成员必须由逗号
        // 引出。这一句是上面注释里那个坑的另一半——删除路径也必须走到它。
        expect_member = false;
        // 键与值之间的空白属于被删成员的跨度；其余成员（`key_start..value_end`）原样保留。
        let key = &text[key_start + 1..key_end - 1];
        if keys.contains(&key) {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str(&text[key_start..value_end]);
    }
}

/// 跳过 JSON 文本里的空白。
fn skip_whitespace(bytes: &[u8], cursor: &mut usize) {
    while matches!(bytes.get(*cursor), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        *cursor += 1;
    }
}

/// 从起始 `"` 扫描一个 JSON 字符串，返回**收尾引号之后**的下标。
fn scan_string(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start + 1;
    while let Some(byte) = bytes.get(cursor) {
        match byte {
            b'\\' => cursor += 2,
            b'"' => return Some(cursor + 1),
            _ => cursor += 1,
        }
    }
    None
}

/// 从值的起始字节扫描一个 JSON 值，返回它的结束下标（顶层对象的成员边界：`}` 或 `,`）。
fn scan_value(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start;
    let mut depth = 0usize;
    while let Some(byte) = bytes.get(cursor) {
        match byte {
            b'"' => cursor = scan_string(bytes, cursor)?,
            b'{' | b'[' => {
                depth += 1;
                cursor += 1;
            }
            b'}' | b']' => {
                if depth == 0 {
                    // 走到自己这一层对象的收尾：值在此结束（不吞掉这个 `}`）。
                    return Some(cursor);
                }
                depth -= 1;
                cursor += 1;
            }
            b',' if depth == 0 => return Some(cursor),
            _ => cursor += 1,
        }
    }
    if depth == 0 { Some(cursor) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acp_core::model::JsonValueText;

    const SESSION: &str = "9f1c2d3e-4a5b-4c6d-8e7f-90a1b2c3d4e5";

    /// Access 侧 imported 结果只做本地 feature 门控：未协商 typed-media 时消息的 `channel`/`partial`
    /// 与 `audio_ref` 必须消失；请求未包含的资源必须清空；其余字段逐字保留。
    #[test]
    fn imported_read_is_gated_to_the_local_negotiation() {
        let document = format!(
            r#"{{"sessionId":"{SESSION}","resources":{{"messages":[{{"messageId":"5d5ebeeb-eea9-453f-9e66-363c86cbd83e","sessionId":"{SESSION}","role":"agent","content":[{{"type":"text","text":"hi"}},{{"type":"audio_ref","mimeType":"audio/wav","byteLength":"3","displayState":"available"}}],"status":"completed","createdAt":"2026-01-01T00:00:00.000Z","turnId":null,"channel":"reply","partial":{{"state":"completed","reason":null}}}}],"pending_interactions":[],"config_options":[]}},"hasEarlier":false,"historyHasEarlier":false}}"#
        );
        let include = HistoryInclude {
            messages: true,
            ..HistoryInclude::default()
        };
        let result = project_imported_read(
            ImportedRead {
                document: JsonValueText::new(&document).expect("json"),
            },
            &SessionId::new(SESSION).expect("session"),
            include,
            false,
            false,
            true,
        )
        .expect("projection");
        let resources = result.resources;
        let messages = resources.messages.expect("messages");
        assert_eq!(messages[0].channel, None);
        assert_eq!(messages[0].partial, None);
        assert_eq!(
            messages[0].content.len(),
            1,
            "未协商 typed-media 时媒体块应被移除"
        );
        assert!(matches!(
            messages[0].content[0],
            AgentContentBlock::Text { .. }
        ));
        assert!(resources.pending_interactions.is_none());
        assert!(resources.config_options.is_none());
        assert!(resources.history.is_none());
        assert_eq!(result.history_has_earlier, None);
    }

    #[test]
    fn imported_read_rejects_a_result_for_another_session() {
        let document = r#"{"sessionId":"00000000-0000-4000-8000-000000000001","resources":{},"hasEarlier":false}"#;
        let error = project_imported_read(
            ImportedRead {
                document: JsonValueText::new(document).expect("json"),
            },
            &SessionId::new(SESSION).expect("session"),
            HistoryInclude::default(),
            true,
            true,
            true,
        );
        assert!(error.is_err());
    }

    /// 本机没有该会话（`NotFound`）时，imported 回源的 `OwnerOffline` 必须原样上抛给
    /// `SessionReadFault::Port`，由协议适配器映射成 `resource.remote_unavailable`；不能被误当成
    /// `session.not_found`，也不能吞成空页。
    struct OfflineImported;

    #[async_trait]
    impl SessionReadSource for OfflineImported {
        async fn read_session(
            &self,
            _actor: &Actor,
            _query: HistoryQuery,
        ) -> Result<HistoryPage, PortError> {
            Err(PortError::NotFound(acp_core::model::EntityRef::Session(
                SessionId::new(SESSION).expect("session"),
            )))
        }

        async fn effective_interactions(
            &self,
            _actor: &Actor,
            _reference: &acp_core::model::SessionReference,
        ) -> Result<Vec<CoreEffectiveInteraction>, PortError> {
            Ok(Vec::new())
        }

        async fn read_imported(
            &self,
            _actor: &Actor,
            _session: &SessionId,
            _query: ImportedReadQuery,
        ) -> Result<Option<ImportedRead>, PortError> {
            Err(PortError::Unavailable(
                acp_core::model::UnavailableKind::OwnerOffline,
            ))
        }
    }

    #[tokio::test]
    async fn imported_owner_offline_is_forwarded_without_becoming_not_found() {
        let session = SessionId::new(SESSION).expect("session");
        let request = SessionReadRequest {
            session,
            include: HistoryInclude {
                messages: true,
                ..HistoryInclude::default()
            },
            before_history: None,
            before_messages: None,
            page_limit: DEFAULT_PAGE_MESSAGES,
            typed_media: false,
            negotiate_acp: false,
            negotiate_file_diff: false,
        };
        let actor = Actor::LocalCli;
        let error = read_session(&OfflineImported, &actor, request)
            .await
            .expect_err("Owner 离线时不能返回页面");
        match error {
            SessionReadFault::Port(PortError::Unavailable(
                acp_core::model::UnavailableKind::OwnerOffline,
            )) => {}
            other => panic!("expected OwnerOffline port error, got {other:?}"),
        }
    }
}

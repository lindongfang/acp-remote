//! 订阅面：`sync.subscribe` / `sync.snapshot_request` / `sync.ack` 的判定与快照下发
//! （`docs/SYNC_PROTOCOL.md` §9.1–§9.5；变更 `sync-server-connection` 的任务 4.1/4.2/4.4–4.7）。
//!
//! 一条连接一份 [`ConnectionState`]（键是 `connectionId`），由 `conn::session` 串行驱动：本模块的
//! `route` 对同一条连接从不并发调用，因此状态机不需要额外的每连接锁（只用一个表锁保护映射本身）。
//!
//! ## 三个判定点
//!
//! 1. **观察授权**（`session.list`）：core 是唯一判定点（`Broker::authorize` 对 `Actor::Device` 的语义
//!    是 `scopes.contains(command)`），本层不自己拼 scope 判定，也不在 core 之外维护第二份可见性策略。
//!    拒绝一律回 `authorization.scope_denied`（消息级，连接保持可用）——与 `sync_visible_sessions` 的
//!    授权口径逐字相同，因此"设备无权读会话"与"设备无权同步"不会给出两种不同的结论。
//! 2. **cursor 判定**（[`cursor`]）：四种拒绝原因可区分、绝不钳制到 head；序号超出 v1 序号域时不落进
//!    这四种里任何一种，而按协议错误（`protocol.schema_invalid`，§3.3）拒绝。
//! 3. **屏障与恢复位置**：`sync.subscribe` 提交的 cursor 是**唯一**的恢复位置（§9.5：服务端保存的
//!    设备 ACK 最大值不得决定新连接的恢复点）。本层保存的 ACK 只用于诊断与保留策略，**不参与投递**。
//!
//! ## 事件投递腿（任务 4.3/4.8）
//!
//! 重放与实时投递都发 `event` 帧，而 §10.1 的 `event` body 里 `origin`（`kind` 必填，取值
//! `agent|device|daemon|local_cli|node`）与 `causationRequestId` 是 **required** 字段
//! （`schemas/sync/v1/event.schema.json`）。这两项在 `owned_event` 里是 `origin_kind`/`causation`
//! 两列（`storage-sqlite` 的 `EVENT_COLUMNS` 选中它们、`event_from_row` 读出行），core 的读面
//! `ReadView::sync_replay` 一次给出定位 + 持久化正文 + 两个身份字段（`origin.deviceId` 只对
//! `kind = device` 非空、`origin.nodeId` 只对 `kind = node` 非空，都经 `causation` 回查
//! `owned_command.actor_id`；`docs/CORE_PORTS_AND_STORAGE.md` §5.2 的 1.6 条目）。**无法归因的
//! 历史行由该读面失败关闭**（`Corrupt`），因此投递层不会收到 `{kind:"device", deviceId:null}`
//! 这种 §10.1 禁止发送方产生的帧；协商了 `core.event-origin.v1` 的连接还额外收到 `nodeId` 键。
//!
//! `origin.deviceId`/`origin.nodeId` 的另外一个约束是**不得由「有没有会话」推断**：core 的读面只取
//! 列值与回查结果，投递层也不得自己拼——本层只做 core → wire 的逐字段映射
//! （[`crate::sync::event::frame`]，它在 `event_body` 之上追加 `acp.raw-payload.v1` 门控）。
//!
//! 因此 [`SubscribeRoute::advance`] 的真实形状是：
//!
//! - 保留窗口判定（`cursor_expired` 的唯一来源）→ 逐页重放（`UseCases::sync_replay`，页长取本连接
//!   协商的 `maxReplayEventsPerBatch`）→ 按 `globalSequence` 升序的 `event` 帧 → `sync.caught_up`；
//! - 屏障建立之后才允许实时事件进队（[`crate::sync::fanout`] 以「屏障已建立」为闸），因此重放期间
//!   到达的事件**排在屏障之后**、绝不越过序号更早的已持久化事件；
//! - 权限过滤（设备 scope 判定在 core 的 `Broker::authorize`）造成的序号空洞合法且不报错，屏障游标
//!   高于该段即可由客户端 ACK（§9.5）；
//! - 读面失败（含 [`PortError::Corrupt`]）与该帧组装不出来时**显式失败关闭**：`internal.unavailable`
//!   （retryable）+ 可见诊断，绝不伪造 `kind`/`deviceId`/`nodeId`、也绝不静默跳过；
//! - 高水位停读由出站账本负责（[`ConnectionHandle::saturated`]）：停读信号传到本层即中断本批重放，
//!   客户端按最后收到的 `event` 序号重连续投（§14）。
//!
//! 快照链路（`sync.reset_required` → `sync.snapshot_request` → `begin`/`chunk`/`end` 与摘要）与重放共用
//! 同一道推进（快照 `end` 之后同样走 [`SubscribeRoute::advance`]），已完整落地（[`crate::sync::snapshot`]）。
//!
//! ## 依赖
//!
//! 本模块调用 `core::use_cases`（`sync_device_handshake_view`/`sync_replay`/`sync_visible_sessions`/
//! `workspaces`/`profiles`/`ids`）与 `core::broker` 的公开面（`Broker::authorize`/`Broker::replay`）以及
//! `sync-protocol` 的 DTO；不 import `storage-sqlite`、不调用 `server::node_link`、不依赖 `app`
//! （`AGENTS.md` §5、`docs/MODULE_ARCHITECTURE.md` §4.9）。**事件投递只走 `UseCases::sync_replay`**
//! （任务 1.6 落地后收拢）：`Broker::replay` 的直连只剩保留窗口探测一处（`reset_required` 的唯一来源，
//! 该结论不在 `sync_replay` 的判定范围内，见 `ReadView::sync_replay` 的口径注释）。

pub mod cursor;
pub mod event;
pub mod fanout;
#[cfg(test)]
mod fanout_tests;

#[cfg(test)]
pub(crate) mod fixture;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use acp_core::broker::{Broker, Denied};
use acp_core::model::{
    Actor, CommittedEvent, DeviceId, EventOrigin as CoreEventOrigin, GlobalCursor, NodeId,
    PortError, RemoteSessionRef, Sequence,
};
use acp_core::ports::ReplayLimit;
use acp_core::use_cases::UseCases;
use serde::de::DeserializeOwned;
use sync_protocol::common::{Cursor, Nullable, RawObject, Uuid};
use sync_protocol::envelope::{Envelope, MessageType};
use sync_protocol::error::{Body, ErrorCode};
use sync_protocol::sync::{Ack, CaughtUp, ResetReason, ResetRequired, SnapshotRequest, Subscribe};
use tracing::{debug, info, warn};

use crate::sync::conn::registry::{ConnectionRegistry, SendFault};
use crate::sync::conn::{ConnectionHandle, MessageRoute, RouteOutcome};
use crate::sync::snapshot::{self, SnapshotInputs};
use crate::sync::subscribe::cursor::{CursorDecision, CursorFault, SubmittedCursor};

/// 本次连接协商出的 feature 集合（`auth.server_challenge.selectedFeatures` 的同值）。
///
/// 门控只认它：`core.local-catalog.v1` 决定目录资源是否下发，`resource.remote-origin.v1` 决定已导入
/// 会话与其来源信息是否可见。**未登记的连接（`on_authenticated` 未被调用）取空集**——两个门控都关闭，
/// 这是失败关闭的取值。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NegotiatedFeatures(Vec<String>);

impl NegotiatedFeatures {
    /// 构造（调用方给出的是 `handshake::negotiate_features` 的排序去重结果）。
    pub fn new(features: Vec<String>) -> Self {
        Self(features)
    }

    /// 是否协商了该 feature。
    pub fn contains(&self, feature: &str) -> bool {
        self.0.iter().any(|candidate| candidate == feature)
    }

    /// 协商结果的原始清单（诊断用）。
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }
}

/// Owner 可达性（`origin.online` 的唯一来源）。
///
/// `sync_visible_sessions` 要求调用方按自己的连接事实给出它（core 不缓存在线状态：变化经
/// `EventPublisher` 的 `session.origin.online_changed` 暴露，`MODULE_ARCHITECTURE.md` §4.1）。本适配器
/// 自己**没有** Owner 连接面（`server::sync` 不得调用 `server::node_link`，`AGENTS.md` §5），因此这条
/// 运行时事实由组合根注入——组合根同时看得见两边的连接表，是唯一能不引入第二份缓存的接线点。
pub trait OwnerReachability: Send + Sync + 'static {
    /// 该 Owner 节点当前是否可达。
    fn reachable(&self, owner: &NodeId) -> bool;
}

/// 一条连接的订阅面状态。
#[derive(Debug, Default)]
struct ConnectionState {
    /// 协商出的 feature（认证完成时交接）。
    features: NegotiatedFeatures,
    /// 该连接的设备 actor（`sync_replay` 的授权入参；屏障建立时一并记下，实时扇出复用同一份）。
    ///
    /// **必须是同一个 actor**：实时与重放走同一个 `session.list` 授权判定，两处各建一个 actor 会让
    /// "同一设备在两条路径上可见性不同"成为可能（scope 在连接期内可能被撤销，见 `put_device`）。
    device: Option<acp_core::model::DeviceId>,
    scopes: acp_core::model::ScopeSet,
    /// 已发给对端的最高游标（`sync.caught_up` 的屏障）：`sync.ack` 的上限（§9.5）。
    barrier: Option<GlobalCursor>,
    /// 本连接的设备 ACK 最大值（单调不减；只服务诊断与保留策略，**不**决定恢复位置）。
    acked: Option<GlobalCursor>,
    /// 本连接是否已经下发过 `cache_incompatible` 的重置确权（防客户端回声形成循环，见
    /// [`SubscribeRoute::on_snapshot_request`]）。
    cache_reset_sent: bool,
}

/// 订阅面的路由：`conn` 的 `MessageRoute` 实现之一（组合根与快照面、命令面组合）。
pub struct SubscribeRoute {
    core: Arc<UseCases>,
    broker: Arc<Broker>,
    registry: Arc<ConnectionRegistry>,
    owner: Arc<dyn OwnerReachability>,
    states: Mutex<BTreeMap<String, ConnectionState>>,
}

impl std::fmt::Debug for SubscribeRoute {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SubscribeRoute")
            .field("connections", &lock(&self.states).len())
            .finish_non_exhaustive()
    }
}

impl SubscribeRoute {
    /// 装配。`registry` 是连接注册表（连接生命周期判定与后续事件扇出共用）；`owner` 由组合根注入。
    pub fn new(
        core: Arc<UseCases>,
        broker: Arc<Broker>,
        registry: Arc<ConnectionRegistry>,
        owner: Arc<dyn OwnerReachability>,
    ) -> Self {
        Self {
            core,
            broker,
            registry,
            owner,
            states: Mutex::new(BTreeMap::new()),
        }
    }

    /// 当前被登记状态的连接数（用例观测口：表必须随连接结束回收）。
    #[cfg(test)]
    pub(crate) fn tracked(&self) -> usize {
        lock(&self.states).len()
    }

    // -----------------------------------------------------------------------------------------
    // 实时扇出的观测口（`fanout` 模块用；本模块仍是这些状态的唯一写方）
    // -----------------------------------------------------------------------------------------

    /// 已建立屏障的连接（实时投递的唯一目标集；顺序按 `(hostId, deviceId)` 稳定）。
    ///
    /// **屏障是实时投递的闸**：没有屏障 = 该连接还在重放/快照中，或已被拒绝——两种情况都由路由线程
    /// 自己推进，实时循环碰它会 interleaving 出"实时事件越过序号更早的已持久化事件"的帧序
    /// （4.3/4.8 的判据）。句柄不在注册表里的条目一并跳过（连接已结束）。
    ///
    /// 每项带该连接的设备 actor：`sync_replay` 的授权入参（与重放路径同一个 actor、同一道
    /// `session.list` 授权，因此实时与重放的可见性判定不可能给出两种结论）。
    pub fn live_targets(
        &self,
        registry: &ConnectionRegistry,
    ) -> Vec<(Arc<ConnectionHandle>, Actor)> {
        let states = lock(&self.states);
        registry
            .handles()
            .into_iter()
            .filter_map(|handle| {
                let state = states.get(handle.connection_id().as_str())?;
                let device = state.device.as_ref()?;
                state.barrier.as_ref()?;
                Some((
                    handle,
                    Actor::Device {
                        device: device.clone(),
                        scopes: state.scopes.clone(),
                    },
                ))
            })
            .collect()
    }

    /// 该连接是否协商了 `core.event-origin.v1`（`origin` 身份字段的唯一门控；扇出侧按连接判）。
    pub fn origin_feature_of(&self, session: &ConnectionHandle) -> bool {
        self.features(session).contains(event::EVENT_ORIGIN_FEATURE)
    }

    /// 事件行 → `event` 帧素材的读面（重放与实时扇出共用同一个实现，字段装配因此不可能漂移）。
    pub fn reader(&self) -> Arc<CoreEventReader> {
        Arc::new(CoreEventReader {
            core: Arc::clone(&self.core),
        })
    }
}

/// 事件行 → [`acp_core::ports::SyncEventRecord`] 的读面（实时扇出与增量重放的**唯一**字段来源）。
///
/// 用 trait 而不是直接传 `&UseCases`：扇出循环跑在自己的任务里，替身因此能在用例中替换真实读面
/// （不需要起 SQLite 就能断言实时帧的每个字段）。
#[async_trait::async_trait]
pub trait EventReader: Send + Sync {
    /// 取回这条事件的 `event` 帧素材；`Ok(None)` = 库里读不到这一行（降级为"客户端按 cursor 重放"）。
    async fn read(
        &self,
        event: &CommittedEvent,
        actor: &Actor,
    ) -> Result<Option<acp_core::ports::SyncEventRecord>, PortError>;
}

/// 生产读面：一次 `sync_replay` 读，从这一行自身取回全部来源字段（与重放逐字节同源）。
pub struct CoreEventReader {
    core: Arc<UseCases>,
}

#[async_trait::async_trait]
impl EventReader for CoreEventReader {
    async fn read(
        &self,
        event: &CommittedEvent,
        actor: &Actor,
    ) -> Result<Option<acp_core::ports::SyncEventRecord>, PortError> {
        // `after` 停在这一行的前一位：窗口读因此必然含这一行（除非库被并发清理 → 空窗口 → `None`）。
        // epoch 取本机事件库 epoch（`CommittedEvent.origin_epoch` 是**会话级** origin epoch，与事件库
        // epoch 不同源，不能拿来当 `serverEpoch`）；读 head 用 `sync_device_handshake_view`——它是 core
        // 已登记的"本机水位与 epoch"读面，未知 deviceId 的视图同样结构完整。
        let head = self
            .core
            .sync_device_handshake_view(&unknown_device())
            .await
            .map_err(|_| PortError::Corrupt("server epoch"))?
            .head;
        let previous = event.global_sequence.get().saturating_sub(1);
        let cursor = GlobalCursor::new(
            head.server_epoch,
            Sequence::new(previous).map_err(|_| PortError::Corrupt("global sequence"))?,
        );
        let page = self
            .core
            .sync_replay(actor, Some(cursor), ReplayLimit::new(1))
            .await?;
        Ok(page
            .events
            .into_iter()
            .find(|record| record.event.id == event.id))
    }
}

/// 一个必然不存在于信任库的 deviceId（只为读 head 与 epoch；未知 deviceId 的视图同样结构完整）。
fn unknown_device() -> DeviceId {
    DeviceId::new("00000000-0000-7000-8000-000000000000").expect("占位 device id 形状合法")
}

#[async_trait::async_trait]
impl MessageRoute for SubscribeRoute {
    async fn route(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        // 顺带回收已结束连接的状态（注册表是"连接还活着吗"的唯一判据；回收只在有消息要处理时发生，
        // 因此不需要额外的生命周期回调）。
        self.reclaim(session.connection_id().as_str());
        match message.message_type() {
            MessageType::SyncSubscribe => self.on_subscribe(session, message).await,
            MessageType::SyncSnapshotRequest => self.on_snapshot_request(session, message).await,
            MessageType::SyncAck => self.on_ack(session, message).await,
            _ => RouteOutcome::Unclaimed,
        }
    }

    /// 认证完成：记下本次协商结果（feature 门控的唯一来源）。
    fn on_authenticated(&self, session: &ConnectionHandle, features: &[String]) {
        let mut states = lock(&self.states);
        states
            .entry(session.connection_id().as_str().to_owned())
            .or_default()
            .features = NegotiatedFeatures::new(features.to_vec());
    }
}

impl SubscribeRoute {
    // -----------------------------------------------------------------------------------------
    // sync.subscribe
    // -----------------------------------------------------------------------------------------

    /// `sync.subscribe`：cursor 判定 → 首次同步（快照流程）或增量续传（§9.2/§9.3）。
    async fn on_subscribe(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        let Some(request) = decode_request::<Subscribe>(message) else {
            // `cursor` 在场但形状非法是 `sync.cursor_invalid{malformed}`（§9.2），不是
            // `protocol.schema_invalid`：两者对端可见的后果不同，不能让一条坏游标退化成通用 schema 错误。
            if has_malformed_cursor(message) {
                return self
                    .cursor_invalid(session, message, CursorFault::Malformed, None)
                    .await;
            }
            return self.schema_invalid(session, message).await;
        };
        let submitted = SubmittedCursor::from_decoded(Ok(request.cursor.as_ref().cloned()));
        let Some((actor, head)) = self.device_context(session, message).await else {
            return RouteOutcome::Claimed;
        };
        // 观察授权（`session.list`）：core 是唯一判定点（与 `sync_visible_sessions` 同口径）。
        if let Err(error) = self.observe(&actor).await {
            debug!(
                event = "sync.subscribe_denied",
                device_id = session.device_id().as_str(),
                error = ?error,
                "the device is not allowed to observe the session list"
            );
            return self
                .deny(session, message, ErrorCode::AuthorizationScopeDenied)
                .await;
        }
        let decision = cursor::decide(&submitted, &head);
        self.apply_cursor_decision(session, message, decision, &head, &actor)
            .await
    }

    /// cursor 判定的落地：回 `sync.cursor_invalid`（需要时）→ 主动下发 `sync.reset_required`（需要时）→
    /// 首次同步等待客户端的 `sync.snapshot_request`，有效游标进增量续传。
    ///
    /// [`CursorDecision::SequenceOutOfRange`] 走的是 `protocol.schema_invalid`（§3.3），因此它**不是**
    /// `sync.cursor_invalid` 的一种：`cursor::fault_for` 对它返回 `None`，由下面的专属臂定案。
    async fn apply_cursor_decision(
        &self,
        session: &ConnectionHandle,
        message: &Envelope,
        decision: CursorDecision,
        head: &GlobalCursor,
        actor: &Actor,
    ) -> RouteOutcome {
        if let Some(fault) = cursor::fault_for(&decision) {
            self.cursor_invalid(session, message, fault, cursor::reset_for(&decision))
                .await;
            // 拒绝一律不建立任何可投递状态（绝不按 head 续投）。
            self.clear_barrier(session);
            return RouteOutcome::Claimed;
        }
        match decision {
            CursorDecision::FirstSync => {
                // §9.4 的序列：服务端先发 `sync.reset_required`，客户端据此发 `sync.snapshot_request`。
                self.send_reset_required(session, ResetReason::InitialSync);
                info!(
                    event = "sync.subscribe_first",
                    device_id = session.device_id().as_str(),
                    head = head.global_sequence.get(),
                    "the device asked for its first sync"
                );
                RouteOutcome::Claimed
            }
            CursorDecision::Resume(cursor) => {
                // 只登记 actor：屏障必须等重放真正走到空页才闭合（§9.3 的顺序不变量 ——
                // 否则整个重放窗口内该连接都是实时目标，实时帧会与重放帧在出站队列里交错）。
                self.note_connection_actor(session, Some(actor));
                self.advance(session, message, &cursor).await;
                RouteOutcome::Claimed
            }
            CursorDecision::SequenceOutOfRange => {
                // §3.3：超出 v1 上界的序号是协议错误（`protocol.schema_invalid`），不是 cursor 判定
                // ——§9.2 的四种 `reason` 表达不了"取值根本无法表示"。与其它拒绝一样不建立任何状态。
                self.clear_barrier(session);
                self.schema_invalid(session, message).await
            }
            CursorDecision::Rejected { .. } => {
                // 由上面的 `fault_for` 分支覆盖；保留臂只为让判定表穷尽且可审计。
                self.clear_barrier(session);
                RouteOutcome::Claimed
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // sync.snapshot_request
    // -----------------------------------------------------------------------------------------

    /// `sync.snapshot_request`：客户端在收到 `sync.reset_required` 之后请求快照（§9.4）。
    ///
    /// `cache_incompatible` 是四种 `reason` 里唯一**只能由客户端自报**的一种（v1 没有"客户端缓存版本"
    /// 的上报字段，快照 `schemaVersion` 固定为 1）。服务端在此把客户端自报的"缓存不兼容"确权为一次
    /// `sync.reset_required{cache_incompatible}`：重置由服务端主动发（客户端不必、也不得从错误码推断），
    /// 且**每条连接只确权一次**——客户端收到 `reset_required` 会回声一条 `snapshot_request`，逐次确权会
    /// 形成无限往返。
    async fn on_snapshot_request(
        &self,
        session: &ConnectionHandle,
        message: &Envelope,
    ) -> RouteOutcome {
        let Some(request) = decode_request::<SnapshotRequest>(message) else {
            return self.schema_invalid(session, message).await;
        };
        if request.reason == ResetReason::CacheIncompatible && !self.mark_cache_reset(session) {
            self.send_reset_required(session, ResetReason::CacheIncompatible);
        }
        self.send_snapshot(session, message).await
    }

    /// 组装并下发整条快照，然后按 §9.4 继续（屏障之后的事件与 `sync.caught_up`）。
    async fn send_snapshot(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        let Some((actor, head)) = self.device_context(session, message).await else {
            return RouteOutcome::Claimed;
        };
        // 与 `sync.subscribe` 同一道闸：观察授权先于任何读取（`workspaces`/`profiles` 是本机管理读面，
        // 以 `Actor::LocalCli` 代读，因此设备侧的授权只能在这里判）。core 是唯一判定点。
        if let Err(error) = self.observe(&actor).await {
            debug!(
                event = "sync.snapshot_denied",
                device_id = session.device_id().as_str(),
                error = ?error,
                "the device is not allowed to observe the session list"
            );
            return self
                .deny(session, message, ErrorCode::AuthorizationScopeDenied)
                .await;
        }
        let features = self.features(session);
        let owner = Arc::clone(&self.owner);
        let reachable =
            move |reference: &RemoteSessionRef| owner.reachable(&reference.owner_node_id);
        let inputs = SnapshotInputs {
            actor: &actor,
            features: &features,
            reachable: &reachable,
        };
        let plan = match snapshot::plan(&self.core, inputs, &head).await {
            Ok(plan) => plan,
            Err(error) => {
                return self.projection_fault(session, message, &error).await;
            }
        };
        match snapshot::send(session, &plan) {
            Ok(sent) => {
                info!(
                    event = "sync.snapshot_sent",
                    device_id = session.device_id().as_str(),
                    snapshot_id = plan.snapshot_id().as_str(),
                    chunks = sent.chunk_count,
                    "the sync snapshot was queued"
                );
                // 只登记 actor：快照的头就是即将开始的投递起点，但屏障仍要等 advance 读到空页才闭合
                // （§9.3：重放期间到达的实时事件必须排在屏障之后）。
                let barrier = GlobalCursor::new(head.server_epoch.clone(), head.global_sequence);
                self.note_connection_actor(session, Some(&actor));
                self.advance(session, message, &barrier).await;
            }
            Err(snapshot::SnapshotFault::Local) => {
                warn!(
                    event = "sync.snapshot_local_fault",
                    device_id = session.device_id().as_str(),
                    snapshot_id = plan.snapshot_id().as_str(),
                    "the snapshot cannot be assembled locally"
                );
                return self
                    .internal(session, message, "the snapshot cannot be assembled")
                    .await;
            }
            Err(snapshot::SnapshotFault::Send(fault)) => {
                // 高水位/连接结束：快照未发完整条，`snapshot_end` 也没有发出。客户端因此丢弃暂存区、重连
                // 后重新请求（§9.4）；连接是否继续由会话的慢消费者看门狗按 §14 决定，本层不重复处置。
                debug!(
                    event = "sync.snapshot_paused",
                    device_id = session.device_id().as_str(),
                    fault = ?fault,
                    "the snapshot stopped early; the client must request it again"
                );
                self.clear_barrier(session);
            }
        }
        RouteOutcome::Claimed
    }

    // -----------------------------------------------------------------------------------------
    // sync.ack
    // -----------------------------------------------------------------------------------------

    /// `sync.ack`：累积、单调不减、不得越过本连接已发送的最高游标，且**不推进事件库 head**（§9.5）。
    ///
    /// 四种判定各自的对端可见后果：
    ///
    /// - 序号超出 v1 序号域（§3.3 的上界）→ `protocol.schema_invalid`（协议错误，不是 cursor 判定；
    ///   与 `sync.subscribe` 共用同一判定，见 [`cursor::sequence_out_of_range`]）；
    /// - epoch 与当前事件库不符 → `sync.cursor_invalid{epoch_mismatch}`（序号不可比，接受它没有意义）；
    /// - 序号**可表示**且超过本连接已发送的最高游标（含"本连接什么都没发过"）→
    ///   `sync.cursor_invalid{beyond_head}`（`reason` 词表里唯一能表达"比本机已知水位更靠前"的取值）；
    /// - 回退或重复 → **忽略**并记诊断，不改动已知进度、也不回错误（§9.5 明确"回退值被忽略并记录诊断"）。
    ///
    /// ACK 只写本连接的内存状态：它不表示用户已阅读，也不改变任何业务状态，更不决定下一条连接从哪里恢复
    /// （恢复位置一律取客户端在 `sync.subscribe` 里提交的 cursor）。
    async fn on_ack(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        let Some(request) = decode_request::<Ack>(message) else {
            return self.schema_invalid(session, message).await;
        };
        let Some((_actor, head)) = self.device_context(session, message).await else {
            return RouteOutcome::Claimed;
        };
        // §3.3：序号域校验先于任何语义比较（与 `sync.subscribe` 的 `cursor::decide` 同一口径）。
        if cursor::sequence_out_of_range(&request.cursor) {
            return self.schema_invalid(session, message).await;
        }
        if request.cursor.server_epoch.as_str() != head.server_epoch.as_str() {
            return self
                .cursor_invalid(session, message, CursorFault::EpochMismatch, None)
                .await;
        }
        let Some(cursor) = cursor::to_core_cursor(&request.cursor) else {
            // 序号域与 epoch 都在上面校验过：走到这里说明形状合法却转换不出来，是接线错误，
            // 按内部不可用失败关闭（不把它错报成某一条 cursor 判定）。
            warn!(
                event = "sync.ack_cursor_unrepresentable",
                device_id = session.device_id().as_str(),
                "the acknowledgement cursor passes the wire checks but cannot be converted"
            );
            return self
                .internal(
                    session,
                    message,
                    "the acknowledgement cursor cannot be read",
                )
                .await;
        };
        let key = session.connection_id().as_str().to_owned();
        let accepted = {
            let mut states = lock(&self.states);
            let state = states.entry(key).or_default();
            match state.barrier.as_ref() {
                Some(barrier) if cursor.global_sequence <= barrier.global_sequence => {
                    match state.acked.as_ref() {
                        Some(existing) if existing.global_sequence >= cursor.global_sequence => {
                            AckVerdict::Regressed
                        }
                        _ => {
                            let accepted = cursor.clone();
                            state.acked = Some(accepted);
                            AckVerdict::Advanced
                        }
                    }
                }
                _ => AckVerdict::BeyondSent,
            }
        };
        match accepted {
            AckVerdict::Advanced => {
                debug!(
                    event = "sync.ack_accepted",
                    device_id = session.device_id().as_str(),
                    global_sequence = cursor.global_sequence.get(),
                    "the device acknowledged the barrier"
                );
                RouteOutcome::Claimed
            }
            AckVerdict::Regressed => {
                // 诊断保留：回退 ACK 是客户端状态机出问题的信号，但按 §9.5 不改动服务端已知进度。
                warn!(
                    event = "sync.ack_ignored",
                    device_id = session.device_id().as_str(),
                    global_sequence = cursor.global_sequence.get(),
                    known = self
                        .known_ack(session)
                        .map(|known| known.global_sequence.get()),
                    "the acknowledgement does not advance the device progress"
                );
                RouteOutcome::Claimed
            }
            AckVerdict::BeyondSent => {
                self.cursor_invalid(session, message, CursorFault::BeyondHead, None)
                    .await
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // 投递推进（增量重放 + barrier，见模块文档）
    // -----------------------------------------------------------------------------------------

    /// 屏障之后的推进（任务 4.3/4.8 的落点）：重放批次 → 按 `globalSequence` 升序的 `event` 帧 →
    /// `sync.caught_up` → 之后由实时扇出继续投递（`sync.fanout`）。
    ///
    /// 三段判据各自不可省：
    ///
    /// 1. **`cursor_expired` 的唯一来源是存储层的保留窗口判定**：窗口下界只在存储层可见（§9.2 的
    ///    "已早于保留窗口"），因此读视图给出的 `reset_required` 在这里翻译成 `sync.cursor_invalid`
    ///    （`details.reason` 取同一词表的对应值）与主动的 `sync.reset_required`。这一步**先于**任何
    ///    事件读取——被拒的游标绝不进入"从下界开始读"那条钳制分支。
    /// 2. **读到的每一页都按 `globalSequence` 升序投递**：同一批内存储层已排序，跨批由 `next` 续读游标
    ///    保证单调（每批的 `next` 停在本批最后一条）。权限过滤（`Broker::authorize` 的设备 scope 判定）
    ///    造成的序号空洞**合法且不报错**（§9.1/§10.1 的 spec）：跳过的序号不进 `event` 帧、不回错误，
    ///    屏障游标高于该段即可由客户端 ACK。
    /// 3. **空读才是屏障**：读不出更多事件时把本批的 `head` 记为屏障并发 `sync.caught_up`。屏障**之后**
    ///    才允许实时事件进队（实时扇出以"屏障已建立"为闸，见 [`crate::sync::fanout`]），因此重放期间
    ///    到达的事件排在屏障之后、不会越过序号更早的已持久化事件。
    ///
    /// 读面失败（含历史行无法归因的 [`PortError::Corrupt`]）一律显式失败关闭：回
    /// `internal.unavailable`（retryable）并记**可见**诊断，绝不伪造 `kind`/`deviceId`/`nodeId`、
    /// 也绝不静默跳过那一批。
    async fn advance(&self, session: &ConnectionHandle, message: &Envelope, from: &GlobalCursor) {
        let Some(actor) = self.connection_actor(session).await else {
            return;
        };
        // ① 保留窗口判定（`limit = 1` 的探测读：结论只取决于游标位置，与页大小无关）。
        match self
            .broker
            .replay(Some(from.clone()), ReplayLimit::new(1))
            .await
        {
            Ok(batch) if batch.reset_required.is_some() => {
                let reason = batch.reset_required;
                warn!(
                    event = "sync.replay_reset_required",
                    device_id = session.device_id().as_str(),
                    reason = ?reason,
                    "the cursor cannot be resumed from this position"
                );
                self.clear_barrier(session);
                match reason.and_then(cursor::decide_from_reset) {
                    Some(CursorDecision::Rejected { fault, reset }) => {
                        self.cursor_invalid(session, message, fault, reset).await;
                    }
                    // `cache_incompatible` 不是 cursor 判定（`decide_from_reset` 对它返回 `None`），
                    // 走到这里说明读视图与 `head()` 之间的结论不一致，按内部不可用失败关闭。
                    _ => {
                        let _ = self
                            .internal(session, message, "the resume position cannot be confirmed")
                            .await;
                    }
                }
                return;
            }
            Err(error) => {
                warn!(
                    event = "sync.replay_failed",
                    device_id = session.device_id().as_str(),
                    error = ?error,
                    "the replay window cannot be read"
                );
                self.clear_barrier(session);
                let _ = self
                    .internal(session, message, "the replay window cannot be read")
                    .await;
                return;
            }
            Ok(_) => {}
        }
        // ② 逐页投递。页长取**本连接协商**的 `maxReplayEventsPerBatch`（§14 的三项可下调值之一：
        //    下发给对端一套、自己按另一套判定会变成两套口径）。
        let limit = ReplayLimit::new(session.limits().max_replay_events_per_batch());
        let mut cursor = from.clone();
        loop {
            if session.saturated() {
                // 高水位停读（§14）：出站账本已经收不下更多帧。已发出的部分留在队列里，客户端按
                // 最后收到的 `event` 序号 ACK；重连后从该 cursor 续投，不丢不重。
                debug!(
                    event = "sync.delivery_paused",
                    device_id = session.device_id().as_str(),
                    global_sequence = cursor.global_sequence.get(),
                    fault = ?SendFault::HighWater,
                    "the outbound ledger stopped the replay; the client resumes from the last event"
                );
                self.clear_barrier(session);
                return;
            }
            match self
                .core
                .sync_replay(&actor, Some(cursor.clone()), limit)
                .await
            {
                Ok(page) => {
                    for record in &page.events {
                        // 未协商 `core.event-origin.v1`：`kind = node` 的事件整条不投递（§10.1：
                        // MUST NOT 因客户端版本伪造或降级 kind），其余照发（旧形状只有两个键）。
                        if record.origin_kind == CoreEventOrigin::Node
                            && !self.origin_feature(session)
                        {
                            debug!(
                                event = "sync.event_skipped_node_origin",
                                device_id = session.device_id().as_str(),
                                event_id = record.event.id.as_str(),
                                global_sequence = record.event.global_sequence.get(),
                                "the connection did not negotiate core.event-origin.v1; the node-origin event is not delivered"
                            );
                            continue;
                        }
                        match crate::sync::event::frame(record, &self.features(session)) {
                            Ok(body) => {
                                if let Err(fault) = session.send(MessageType::Event, &body) {
                                    debug!(
                                        event = "sync.event_not_sent",
                                        device_id = session.device_id().as_str(),
                                        event_id = record.event.id.as_str(),
                                        fault = ?fault,
                                        "the outbound ledger refused the event; the client resumes from it"
                                    );
                                    self.clear_barrier(session);
                                    return;
                                }
                            }
                            Err(fault) => {
                                // 组装不出合法帧（含身份字段形状不符）：显式失败关闭。
                                // 这里绝不伪造 kind/deviceId、也不跳过该条假装没看见。
                                warn!(
                                    event = "sync.event_unassemblable",
                                    device_id = session.device_id().as_str(),
                                    event_id = record.event.id.as_str(),
                                    global_sequence = record.event.global_sequence.get(),
                                    fault = %fault,
                                    "the persisted event cannot be mapped to a v1 event frame"
                                );
                                self.clear_barrier(session);
                                self.refuse_for_now(session, message);
                                return;
                            }
                        }
                    }
                    if let Some(last) = page.events.last() {
                        cursor = GlobalCursor::new(
                            page.head.server_epoch.clone(),
                            last.event.global_sequence,
                        );
                    }
                    // 空页 = 到 head 了：记屏障并发 `sync.caught_up`。屏障游标取**本批读视图的 head**
                    // （同一只读事务内的一致性水位），它高于任何被过滤/跳过的序号段。
                    if page.events.is_empty() {
                        let barrier = page.head.clone();
                        self.set_barrier(session, barrier.clone());
                        debug!(
                            event = "sync.caught_up_sent",
                            device_id = session.device_id().as_str(),
                            global_sequence = barrier.global_sequence.get(),
                            "the replay reached the head; the barrier is now open for live events"
                        );
                        self.send_caught_up(session, &barrier);
                        return;
                    }
                }
                Err(error) => {
                    warn!(
                        event = "sync.event_read_failed",
                        device_id = session.device_id().as_str(),
                        global_sequence = cursor.global_sequence.get(),
                        error = ?error,
                        "the sync event read surface failed; the delivery is closed down"
                    );
                    self.clear_barrier(session);
                    let _ = self
                        .internal(session, message, "the replay window cannot be read")
                        .await;
                    return;
                }
            }
        }
    }

    /// 本连接的设备 actor（`sync_replay` 的授权入参；持久化信任行缺失时失败关闭）。
    async fn connection_actor(&self, session: &ConnectionHandle) -> Option<Actor> {
        let view = match self
            .core
            .sync_device_handshake_view(session.device_id())
            .await
        {
            Ok(view) => view,
            Err(error) => {
                warn!(
                    event = "sync.device_view_failed",
                    device_id = session.device_id().as_str(),
                    error = ?error,
                    "the persisted device snapshot cannot be read"
                );
                return None;
            }
        };
        let Some(device) = view.device.as_ref() else {
            warn!(
                event = "sync.device_missing",
                device_id = session.device_id().as_str(),
                "the authenticated device has no persisted trust row"
            );
            return None;
        };
        Some(Actor::Device {
            device: session.device_id().clone(),
            scopes: device.scopes().clone(),
        })
    }

    /// 本连接是否协商了 `core.event-origin.v1`（`origin` 身份字段的唯一门控）。
    fn origin_feature(&self, session: &ConnectionHandle) -> bool {
        self.features(session).contains(event::EVENT_ORIGIN_FEATURE)
    }

    // -----------------------------------------------------------------------------------------
    // 状态与对端可见的装配
    // -----------------------------------------------------------------------------------------

    /// 该设备的持久化信任行与本机水位（`serverEpoch` 与 cursor 判定的共同来源）。
    ///
    /// 设备行缺失（认证后被删除，或接线错误）一律失败关闭：本层不凭握手自报值继续。
    async fn device_context(
        &self,
        session: &ConnectionHandle,
        message: &Envelope,
    ) -> Option<(Actor, GlobalCursor)> {
        let view = match self
            .core
            .sync_device_handshake_view(session.device_id())
            .await
        {
            Ok(view) => view,
            Err(error) => {
                warn!(
                    event = "sync.device_view_failed",
                    device_id = session.device_id().as_str(),
                    error = ?error,
                    "the persisted device snapshot cannot be read"
                );
                let _ = self
                    .internal(session, message, "the device state cannot be read")
                    .await;
                return None;
            }
        };
        let Some(device) = view.device.as_ref() else {
            warn!(
                event = "sync.device_missing",
                device_id = session.device_id().as_str(),
                "the authenticated device has no persisted trust row"
            );
            let _ = self
                .deny(session, message, ErrorCode::AuthorizationScopeDenied)
                .await;
            return None;
        };
        let actor = Actor::Device {
            device: session.device_id().clone(),
            scopes: device.scopes().clone(),
        };
        Some((actor, view.head))
    }

    /// 观察授权（`session.list`）：core 是唯一判定点（`Actor::Device` 分支即 `scopes.contains()`）。
    async fn observe(&self, actor: &Actor) -> Result<(), PortError> {
        let request = self.core.ids().request_id();
        self.broker
            .authorize(actor, "session.list", None, &request)
            .await
            .map_err(Denied::into_port_error)
    }

    /// 本连接协商出的 feature（未登记时为空集：两个门控都关闭）。
    fn features(&self, session: &ConnectionHandle) -> NegotiatedFeatures {
        lock(&self.states)
            .get(session.connection_id().as_str())
            .map(|state| state.features.clone())
            .unwrap_or_default()
    }

    /// 闭合屏障：把「已发给对端的最高游标」记下来，实时扇出从此可以投这条连接。
    ///
    /// **只在重放真正读到 head（空页）时调用**。提前闭合（例如订阅刚到达、或快照刚发完）会让整个
    /// 重放窗口内的这条连接都成为实时目标，实时帧于是与重放帧在出站队列里交错，破坏 §9.3 的
    /// 「重放期间到达的事件排在屏障之后」这一顺序不变量。
    ///
    /// 设备的 actor 不在这里落：它在连接进入订阅时就已确定，且生命周期与杠杆点不同，见
    /// [`Self::note_connection_actor`]。
    fn set_barrier(&self, session: &ConnectionHandle, cursor: GlobalCursor) {
        let mut states = lock(&self.states);
        let state = states
            .entry(session.connection_id().as_str().to_owned())
            .or_default();
        state.barrier = Some(cursor);
    }

    /// 记下该连接的设备 actor（`live_targets` 的授权入参），**不与屏障闭合混为一谈**。
    ///
    /// `sync_replay` 的授权判定用同一个 actor、同一道 `session.list`，因此实时与重放的可见性判定
    /// 不可能给出两种结论。actor 在连接刚进入订阅时就已确定；屏障则必须等重放真正读到 head 才闭合
    /// （§9.3 的顺序不变量），两者生命周期不同，故分开记账。
    fn note_connection_actor(&self, session: &ConnectionHandle, actor: Option<&Actor>) {
        if let Some(Actor::Device { device, scopes }) = actor {
            let mut states = lock(&self.states);
            let state = states
                .entry(session.connection_id().as_str().to_owned())
                .or_default();
            state.device = Some(device.clone());
            state.scopes = scopes.clone();
        }
    }

    /// 清除屏障：本连接没有任何已发送的游标（拒绝、快照未发完整条、或投递失败关闭）。
    fn clear_barrier(&self, session: &ConnectionHandle) {
        let mut states = lock(&self.states);
        if let Some(state) = states.get_mut(session.connection_id().as_str()) {
            state.barrier = None;
        }
    }

    /// 本连接已知的设备 ACK 最大值（诊断）。
    fn known_ack(&self, session: &ConnectionHandle) -> Option<GlobalCursor> {
        lock(&self.states)
            .get(session.connection_id().as_str())
            .and_then(|state| state.acked.clone())
    }

    /// 标记"本连接已经确权过一次 `cache_incompatible` 重置"；返回是否**已经**确权过。
    fn mark_cache_reset(&self, session: &ConnectionHandle) -> bool {
        let mut states = lock(&self.states);
        let state = states
            .entry(session.connection_id().as_str().to_owned())
            .or_default();
        let already = state.cache_reset_sent;
        state.cache_reset_sent = true;
        already
    }

    /// 回收已结束连接的状态（注册表是"连接还活着吗"的唯一判据）。
    fn reclaim(&self, current: &str) {
        let alive: Vec<String> = self
            .registry
            .handles()
            .iter()
            .map(|handle| handle.connection_id().as_str().to_owned())
            .collect();
        let mut states = lock(&self.states);
        states.retain(|key, _| key == current || alive.contains(key));
    }

    /// `sync.caught_up`（屏障）。
    fn send_caught_up(&self, session: &ConnectionHandle, barrier: &GlobalCursor) {
        let Some(cursor) = snapshot::wire_cursor(barrier) else {
            return;
        };
        let body = CaughtUp { cursor };
        if let Err(fault) = session.send(MessageType::SyncCaughtUp, &body) {
            debug!(
                event = "sync.caught_up_not_sent",
                device_id = session.device_id().as_str(),
                fault = ?fault,
                "the caught_up barrier could not be queued"
            );
        }
    }

    /// 主动下发 `sync.reset_required`（§9.2：客户端不得仅凭错误码推断重置）。
    fn send_reset_required(&self, session: &ConnectionHandle, reason: ResetReason) {
        let body = ResetRequired {
            reason,
            snapshot_available: true,
        };
        if let Err(fault) = session.send(MessageType::SyncResetRequired, &body) {
            debug!(
                event = "sync.reset_required_not_sent",
                device_id = session.device_id().as_str(),
                reason = reason.as_str(),
                fault = ?fault,
                "the reset request could not be queued"
            );
        }
    }

    /// `sync.cursor_invalid`（消息级拒绝，连接保持可用）。
    ///
    /// `details.reason` 是 §12.2 的登记字段且取值域是四种原因；`reset` 非空时**同时**主动下发
    /// `sync.reset_required`（错误码只表达"这条游标不可用"，重置是独立的一条消息）。
    async fn cursor_invalid(
        &self,
        session: &ConnectionHandle,
        message: &Envelope,
        fault: CursorFault,
        reset: Option<ResetReason>,
    ) -> RouteOutcome {
        let body = error_body(
            ErrorCode::SyncCursorInvalid,
            "the cursor cannot be used for resume",
            cursor_reason_details(fault),
            Some(message.message_id().clone()),
        );
        self.send_error(session, body, "cursor_invalid");
        if let Some(reason) = reset {
            self.send_reset_required(session, reason);
        }
        RouteOutcome::Claimed
    }

    /// `protocol.schema_invalid`（消息级拒绝，连接保持可用）。
    async fn schema_invalid(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        let body = error_body(
            ErrorCode::ProtocolSchemaInvalid,
            "the message does not match the v1 schema",
            RawObject::empty(),
            Some(message.message_id().clone()),
        );
        self.send_error(session, body, "schema_invalid");
        RouteOutcome::Claimed
    }

    /// `authorization.scope_denied`（消息级拒绝，连接保持可用）。
    async fn deny(
        &self,
        session: &ConnectionHandle,
        message: &Envelope,
        code: ErrorCode,
    ) -> RouteOutcome {
        let body = error_body(
            code,
            "the device is not allowed to observe this host",
            RawObject::empty(),
            Some(message.message_id().clone()),
        );
        self.send_error(session, body, "scope_denied");
        RouteOutcome::Claimed
    }

    /// `internal.unavailable`（消息级拒绝，连接保持可用；`retryable` 由错误码登记值决定）。
    async fn internal(
        &self,
        session: &ConnectionHandle,
        message: &Envelope,
        text: &'static str,
    ) -> RouteOutcome {
        let body = error_body(
            ErrorCode::InternalUnavailable,
            text,
            RawObject::empty(),
            Some(message.message_id().clone()),
        );
        self.send_error(session, body, "internal");
        RouteOutcome::Claimed
    }

    /// 该次投递失败关闭（历史行无法归因 / 帧组装不出来）：显式告诉对端"这回拿不到事件流"，
    /// 不谎报屏障、也不静默丢一批。`retryable` 由错误码登记值决定（`internal.unavailable` = true）。
    fn refuse_for_now(&self, session: &ConnectionHandle, message: &Envelope) {
        let body = error_body(
            ErrorCode::InternalUnavailable,
            "the host cannot deliver events on this connection right now",
            RawObject::empty(),
            Some(message.message_id().clone()),
        );
        self.send_error(session, body, "event_delivery_failed");
    }

    /// 快照投影失败：按内部不可用失败关闭（不静默少发资源）。
    async fn projection_fault(
        &self,
        session: &ConnectionHandle,
        message: &Envelope,
        error: &PortError,
    ) -> RouteOutcome {
        warn!(
            event = "sync.snapshot_projection_failed",
            device_id = session.device_id().as_str(),
            error = ?error,
            "the snapshot resources cannot be projected as-is"
        );
        self.internal(session, message, "the snapshot cannot be assembled")
            .await
    }

    /// 入队一条 `error` 帧（装配失败只记日志：本机缺陷不该改变会话状态）。
    fn send_error(&self, session: &ConnectionHandle, body: Option<Body>, event: &'static str) {
        let Some(body) = body else {
            warn!(
                event = "sync.error_body_failed",
                detail = event,
                "the error body cannot be assembled"
            );
            return;
        };
        if let Err(fault) = session.send(MessageType::Error, &body) {
            debug!(
                event = "sync.error_not_sent",
                detail = event,
                fault = ?fault,
                "the error message could not be queued"
            );
        }
    }
}

/// `sync.ack` 的判定结果（三种处置各自有对端可见后果）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AckVerdict {
    /// 前进：记录为新的设备进度。
    Advanced,
    /// 回退或重复：忽略并记诊断。
    Regressed,
    /// 超过本连接已发送的最高游标：`sync.cursor_invalid`。
    BeyondSent,
}

/// 解码消息 body（失败按 schema 错误处理，调用方决定错误码）。
fn decode_request<T: DeserializeOwned>(message: &Envelope) -> Option<T> {
    serde_json::from_str(message.body().get()).ok()
}

/// `sync.subscribe` 的 body 解码失败时，判断失败是否只出在 `cursor` 的形状上（§9.2 的 `malformed`）。
///
/// 判定必须在**失败路径**上重读帧：`Subscribe` 的类型化解码把"游标形状非法"与"信封其余字段不合法"
/// 折进同一个 `Err`，而两者对端可见的错误码不同（`sync.cursor_invalid{malformed}` vs
/// `protocol.schema_invalid`）。重读只看三件事：`cursor` 键在场、值不是 `null`、值不是合法 `Cursor`；
/// `scope` 或其它字段错在哪不影响这条判定（游标的形状问题是更具体的那一类）。
fn has_malformed_cursor(message: &Envelope) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(message.body().get()) else {
        return false;
    };
    let Some(cursor) = value.get("cursor") else {
        return false;
    };
    !cursor.is_null() && serde_json::from_value::<Cursor>(cursor.clone()).is_err()
}

/// `sync.cursor_invalid` 的 `details.reason` 形状（供用例与诊断直接比较，避免两处手写词表）。
pub fn cursor_reason_details(fault: CursorFault) -> RawObject {
    RawObject::parse(&format!("{{\"reason\":\"{}\"}}", fault.as_str()))
        .unwrap_or_else(|_| RawObject::empty())
}

/// `error` 的 body（`schemas/sync/v1/error.schema.json` 的 `errorBody`）。
///
/// `conn::wire::error_body` 是同一形状，但那个模块的可见性是 `conn` 私有（`mod wire;`），而本工作包的
/// 写范围不含 `conn/mod.rs`；因此这里按同一口径重写一份（只允许 §12.2 登记的 `details` 字段、
/// `retryable` 一律取错误码在 registry 里登记的语义、`correlationId` 指向触发本次错误的入站消息）。
fn error_body(
    code: ErrorCode,
    message: &str,
    details: RawObject,
    correlation_id: Option<Uuid>,
) -> Option<Body> {
    let retryable = code.default_retryable();
    let fallback = Body::new(code, code.as_str(), retryable).ok();
    let mut body = Body::new(code, message, retryable).ok().or(fallback)?;
    body.correlation_id = Nullable::from_option(correlation_id);
    body.details = details;
    Some(body)
}

/// 锁中毒时取回内层数据而不是 panic：订阅状态表在任何 panic 之后仍要能继续服务其它连接。
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

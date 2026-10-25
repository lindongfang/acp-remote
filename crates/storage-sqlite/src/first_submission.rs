//! owned 首条提交、派发围栏与启动收敛（`docs/CORE_PORTS_AND_STORAGE.md` §5.11/§7.3、
//! `design.md` D7/D8）。
//!
//! 四个不变量：
//!
//! - **先存再派发**：`commit_first_submission` 在同一 `BEGIN IMMEDIATE` 里写会话行、首条用户消息、
//!   命令受理、turn、`owned_first_submission` 与附件正式引用；任一步失败全回滚，`session.start` 不会
//!   被受理，也不会派发。
//! - **身份只有一份**：`user_message_id` 唯一；回显投影到同一身份，不追加第二条首条消息。
//! - **阶段只能前进**：`saved_not_dispatched → dispatch_started → terminal` 由条件更新（CAS）推进；
//!   `ever_dispatched` 一旦为 1 不再回落。
//! - **不自动重发**：启动收敛只把可证明未派发记为失败（等待显式继续）、把跨越派发边界的记为
//!   `uncertain`；它绝不自动调用 Agent。
//!
//! 附件认领只核验已封存事实并把字节 `rename` 到同一内容寻址路径，不复制、不全量重哈希。

use async_trait::async_trait;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, Sqlite, Transaction};

use acp_core::model::{
    Actor, AgentSessionId, CommandResult, CommandStatus, CommandTerminalRecord, ConflictKind,
    Digest, EntityRef, EventKind, EventOrigin, FIRST_USER_MESSAGE_SOURCE, FirstSubmissionAdmission,
    FirstSubmissionConvergence, FirstSubmissionRecord, FirstSubmissionSettlement,
    FirstSubmissionStage, FirstSubmissionWrite, FirstUserMessage, MessageId, ModeId, PortError,
    PublicError, RequestId, RequestOutcome, RequestTerminal, ResourceGeneration, SessionId,
    StagedClaim, StoredPolicy, SubmissionId, Timestamp, TurnId, TurnState, ViewJson,
};
use acp_core::model::{DispatchAdmission, DispatchFence};
use acp_core::ports::{FirstSubmission, RetentionPolicy};

use crate::error::StorageError;
use crate::session_store::{
    Db, SessionEventWrite, SqliteStore, apply_terminal, int, opt_text, text, write_session_event,
};

fn opt_int(row: &SqliteRow, column: &'static str) -> Result<Option<i64>, StorageError> {
    row.try_get::<Option<i64>, _>(column)
        .map_err(|_| StorageError::ColumnValue {
            column,
            expected: "integer or null",
        })
}

fn record_from_row(row: &SqliteRow) -> Result<FirstSubmissionRecord, StorageError> {
    let stage: FirstSubmissionStage =
        text(row, "stage")?
            .parse()
            .map_err(|_| StorageError::ColumnValue {
                column: "owned_first_submission.stage",
                expected: "saved_not_dispatched|dispatch_started|terminal",
            })?;
    let ever_dispatched = int(row, "ever_dispatched")? != 0;
    let terminal_outcome = opt_text(row, "terminal_outcome")?;
    let terminal = match terminal_outcome {
        None => None,
        Some(outcome) => {
            let outcome: RequestOutcome =
                outcome.parse().map_err(|_| StorageError::ColumnValue {
                    column: "owned_first_submission.terminal_outcome",
                    expected: "completed|failed|uncertain",
                })?;
            let at = Timestamp::new(&text(row, "terminal_at")?).map_err(|_| {
                StorageError::ColumnValue {
                    column: "owned_first_submission.terminal_at",
                    expected: "timestamp",
                }
            })?;
            let error = match opt_text(row, "error_code")? {
                None => None,
                Some(code) => Some(
                    PublicError::try_new(
                        &code,
                        &opt_text(row, "error_message")?.unwrap_or_default(),
                        opt_int(row, "retryable")?.unwrap_or(0) != 0,
                        ViewJson::new(
                            &opt_text(row, "error_details_json")?
                                .unwrap_or_else(|| "{}".to_owned()),
                        )
                        .map_err(|_| StorageError::Corrupt("stored error details are not json"))?,
                    )
                    .map_err(|_| StorageError::Corrupt("stored terminal error is invalid"))?,
                ),
            };
            Some(RequestTerminal {
                request_id: text(row, "current_request_id")?.parse().map_err(|_| {
                    StorageError::ColumnValue {
                        column: "owned_first_submission.current_request_id",
                        expected: "canonical uuid",
                    }
                })?,
                outcome,
                at,
                error,
            })
        }
    };
    Ok(FirstSubmissionRecord {
        session: text(row, "session_id")?
            .parse()
            .map_err(|_| StorageError::ColumnValue {
                column: "owned_first_submission.session_id",
                expected: "canonical uuid",
            })?,
        first_submission_id: text(row, "first_submission_id")?.parse().map_err(|_| {
            StorageError::ColumnValue {
                column: "owned_first_submission.first_submission_id",
                expected: "canonical uuid",
            }
        })?,
        user_message_id: text(row, "user_message_id")?.parse().map_err(|_| {
            StorageError::ColumnValue {
                column: "owned_first_submission.user_message_id",
                expected: "canonical uuid",
            }
        })?,
        first_request_id: text(row, "first_request_id")?.parse().map_err(|_| {
            StorageError::ColumnValue {
                column: "owned_first_submission.first_request_id",
                expected: "canonical uuid",
            }
        })?,
        current_request_id: text(row, "current_request_id")?.parse().map_err(|_| {
            StorageError::ColumnValue {
                column: "owned_first_submission.current_request_id",
                expected: "canonical uuid",
            }
        })?,
        stage,
        ever_dispatched,
        agent_session: opt_text(row, "agent_session_id")?
            .map(|text| {
                AgentSessionId::new(&text).map_err(|_| StorageError::ColumnValue {
                    column: "owned_first_submission.agent_session_id",
                    expected: "agent session id",
                })
            })
            .transpose()?,
        resource_generation: opt_text(row, "resource_epoch")?
            .map(|value| {
                value
                    .parse::<u64>()
                    .map(ResourceGeneration::new)
                    .map_err(|_| StorageError::ColumnValue {
                        column: "owned_first_submission.resource_epoch",
                        expected: "non-negative decimal integer",
                    })
            })
            .transpose()?,
        // D8.1：显式继续省略 `mode`/`configOptions` 时沿用记录的选择，因此只读投影必须带上它们。
        // `config` 保留记录原文（`{}` = 未选择）：`Text`/`Select` 的区分只属于适配器，存储层不反推。
        mode_id: opt_text(row, "mode_id")?
            .map(|value| {
                ModeId::new(&value).map_err(|_| StorageError::ColumnValue {
                    column: "owned_first_submission.mode_id",
                    expected: "mode id",
                })
            })
            .transpose()?,
        config: ViewJson::new(&text(row, "config_json")?)
            .map_err(|_| StorageError::Corrupt("stored config selection is not a json object"))?,
        input_digest: text(row, "input_digest")?.parse().map_err(|_| {
            StorageError::ColumnValue {
                column: "owned_first_submission.input_digest",
                expected: "canonical base64url32",
            }
        })?,
        terminal,
    })
}

const FIRST_SUBMISSION_COLUMNS: &str = "session_id, first_submission_id, user_message_id, \
     first_request_id, current_request_id, actor_kind, actor_id, request_fingerprint, input_json, \
     input_digest, mode_id, config_json, stage, ever_dispatched, agent_session_id, resource_epoch, \
     terminal_outcome, terminal_at, error_code, error_message, error_details_json, retryable, \
     created_at, updated_at";

async fn load_row(
    tx: &mut Transaction<'_, Sqlite>,
    session: &SessionId,
) -> Result<Option<SqliteRow>, PortError> {
    Ok(sqlx::query(&format!(
        "SELECT {FIRST_SUBMISSION_COLUMNS} FROM owned_first_submission WHERE session_id = ?1"
    ))
    .bind(session.as_str())
    .fetch_optional(&mut **tx)
    .await
    .db()?)
}

/// 读取首条用户消息：身份来自 `owned_first_submission.user_message_id`（唯一），**不**按
/// `channel='user' ORDER BY created_at LIMIT 1` 猜——后续普通输入也会写 `channel='user'` 行，
/// 按时间取最早一条在并发/同毫秒时间戳下不再是首条。
///
/// 没有首条关联（旧会话/普通会话）→ `Ok(None)`；关联存在但消息行缺失或不可读 → `Corrupt`
/// （显式失败，不回退时间排序、不编造内容）。
async fn load_first_message(
    ex: &mut sqlx::SqliteConnection,
    session: &SessionId,
) -> Result<Option<FirstUserMessage>, PortError> {
    let Some(user_message) = sqlx::query_scalar::<_, String>(
        "SELECT user_message_id FROM owned_first_submission WHERE session_id = ?1",
    )
    .bind(session.as_str())
    .fetch_optional(&mut *ex)
    .await
    .db()?
    else {
        return Ok(None);
    };
    let row = sqlx::query(
        "SELECT message_id, turn_id, content_json, created_at FROM owned_message \
         WHERE message_id = ?1 AND session_id = ?2 AND channel = 'user'",
    )
    .bind(&user_message)
    .bind(session.as_str())
    .fetch_optional(&mut *ex)
    .await
    .db()?;
    let Some(row) = row else {
        return Err(PortError::Corrupt(
            "the first submission user message is missing",
        ));
    };
    Ok(Some(FirstUserMessage {
        session: session.clone(),
        message: text(&row, "message_id")?
            .parse()
            .map_err(|_| PortError::Corrupt("owned_message.message_id is malformed"))?,
        turn: opt_text(&row, "turn_id")?
            .map(|text| {
                text.parse::<TurnId>()
                    .map_err(|_| PortError::Corrupt("owned_message.turn_id is malformed"))
            })
            .transpose()?,
        content: acp_core::model::JsonValueText::new(&text(&row, "content_json")?)
            .map_err(|_| PortError::Corrupt("owned_message.content_json is not json"))?,
        created_at: text(&row, "created_at")?
            .parse()
            .map_err(|_| PortError::Corrupt("owned_message.created_at is malformed"))?,
    }))
}

fn actor_columns(actor: &Actor) -> (&'static str, String) {
    (actor.kind().as_str(), actor.id_text())
}

pub(crate) fn media_kind_of(media_type: &str) -> &'static str {
    if media_type.starts_with("image/") {
        "image"
    } else if media_type.starts_with("audio/") {
        "audio"
    } else {
        "file"
    }
}

fn config_json(config: &[acp_core::model::FirstSubmissionConfig]) -> Result<String, PortError> {
    let mut object = serde_json::Map::new();
    for entry in config {
        let value = match (&entry.value.as_boolean(), entry.value.as_text()) {
            (Some(boolean), _) => serde_json::Value::Bool(*boolean),
            (_, Some(text)) => serde_json::Value::String(text.to_owned()),
            _ => return Err(PortError::InvalidRequest("config value has no wire form")),
        };
        object.insert(entry.id.as_str().to_owned(), value);
    }
    serde_json::to_string(&serde_json::Value::Object(object))
        .map_err(|_| PortError::InvalidRequest("config selection is not serializable"))
}

/// `Actor` → `owned_event.origin_kind`（§10.1：命令诱发事件按执行者归因）。
fn actor_origin(actor: &Actor) -> EventOrigin {
    match actor {
        Actor::LocalCli => EventOrigin::LocalCli,
        Actor::Device { .. } => EventOrigin::Device,
        Actor::Node { .. } => EventOrigin::Node,
        Actor::PairingClaimant { .. } => EventOrigin::Daemon,
    }
}

/// `PublicError` → `publicError` 视图（`common.schema.json#/$defs/publicError`）。
fn error_view(error: Option<&PublicError>) -> serde_json::Value {
    let Some(error) = error else {
        return serde_json::json!({
            "code": "internal.unavailable",
            "message": "first submission failed",
            "retryable": false,
            "details": {},
        });
    };
    let details = serde_json::from_str::<serde_json::Value>(error.details().as_str())
        .unwrap_or_else(|_| serde_json::json!({}));
    serde_json::json!({
        "code": error.code(),
        "message": error.message(),
        "retryable": error.retryable(),
        "details": details,
    })
}

/// `session.start` 终态的冻结结果投影（`SYNC_PROTOCOL.md` §11.9 的六字段）。
///
/// 同一份投影既是 `command.completed` 的 `view.result`，也是 `owned_command.result_json` 的取值：
/// 两者必须同源，否则 `command.status` 的重放结果会与事件流里的结果不一致。
fn first_submission_result(
    record: &FirstSubmissionRecord,
    request: &RequestId,
    outcome: RequestOutcome,
    dispatched: bool,
) -> serde_json::Value {
    let stage = if dispatched {
        FirstSubmissionStage::Terminal
    } else {
        FirstSubmissionStage::SavedNotDispatched
    };
    serde_json::json!({
        "stage": stage.as_str(),
        "firstSubmissionId": record.first_submission_id.as_str(),
        "firstRequestId": record.first_request_id.as_str(),
        "currentRequestId": request.as_str(),
        "everDispatched": record.ever_dispatched,
        // §11.9：只有「可证明未派发 ∧ 首个命令已有 failed 终态」才为真。
        "canContinue": !dispatched && outcome == RequestOutcome::Failed,
    })
}

/// `command.completed`/`command.failed`/`command.uncertain` 的 `view`（§10.3）。
fn terminal_view(
    request: &RequestId,
    outcome: RequestOutcome,
    error: Option<&PublicError>,
    result: Option<&serde_json::Value>,
) -> ViewJson {
    let value = match outcome {
        RequestOutcome::Completed => serde_json::json!({
            "requestId": request.as_str(),
            "result": result.cloned().unwrap_or(serde_json::Value::Null),
        }),
        RequestOutcome::Failed => serde_json::json!({
            "requestId": request.as_str(),
            "error": error_view(error),
        }),
        RequestOutcome::Uncertain => serde_json::json!({
            "requestId": request.as_str(),
            "reason": error.map(PublicError::message).unwrap_or("first submission may have been dispatched"),
            "mayHaveReachedAgent": true,
        }),
    };
    ViewJson::new(&value.to_string()).unwrap_or_else(|_| ViewJson::empty_object())
}

/// `turn.completed`/`turn.failed` 的 `view`（§10.3：`turnId` + `state`，失败带终态错误）。
fn turn_terminal_view(
    turn: &TurnId,
    outcome: RequestOutcome,
    error: Option<&PublicError>,
) -> ViewJson {
    let value = if outcome == RequestOutcome::Completed {
        serde_json::json!({ "turnId": turn.as_str(), "state": TurnState::Completed.as_str() })
    } else {
        serde_json::json!({
            "turnId": turn.as_str(),
            "state": TurnState::Failed.as_str(),
            "error": error_view(error),
        })
    };
    ViewJson::new(&value.to_string()).unwrap_or_else(|_| ViewJson::empty_object())
}

/// `user.message.delta.view.text` 的 §10.3 上限（字符数）。
///
/// 初始事件只是首条在事件流里的可见性投影，正文权威仍是 `owned_message.content_json`；超长输入因此按该
/// 上限截断，而不是让整条事件因视图超限无法落盘。
const FIRST_MESSAGE_TEXT_MAX_CHARS: usize = 262_144;

/// 首条用户消息进入事件流时的文本投影：只拼接 `text` 块。
///
/// 其它块（附件引用、扩展容器）不在这里文本化——它们的权威仍是 `owned_message.content_json` 原文，
/// 事件只是让首条在事件流里可见（D7「正式消息投影与初始事件在同事务写」）。
fn first_message_text(content_json: &str) -> Result<String, PortError> {
    let blocks: Vec<serde_json::Value> = serde_json::from_str(content_json)
        .map_err(|_| PortError::Corrupt("owned_message.content_json is not a json array"))?;
    let mut text = String::new();
    let mut remaining = FIRST_MESSAGE_TEXT_MAX_CHARS;
    for block in blocks {
        if remaining == 0 {
            break;
        }
        if block.get("type").and_then(|value| value.as_str()) == Some("text")
            && let Some(chunk) = block.get("text").and_then(|value| value.as_str())
        {
            let taken: String = chunk.chars().take(remaining).collect();
            remaining -= taken.chars().count();
            text.push_str(&taken);
        }
    }
    Ok(text)
}

/// 终态结果对应的 `CommandStatus`（§11.2）。
fn command_status(outcome: RequestOutcome) -> CommandStatus {
    match outcome {
        RequestOutcome::Completed => CommandStatus::Completed,
        RequestOutcome::Failed => CommandStatus::Failed,
        RequestOutcome::Uncertain => CommandStatus::Uncertain,
    }
}

/// 本会话首条提交对应的 turn（`causation = first_request_id`，由首次受理写入）。
async fn first_submission_turn(
    tx: &mut Transaction<'_, Sqlite>,
    session: &SessionId,
    first_request: &RequestId,
) -> Result<Option<TurnId>, PortError> {
    let turn: Option<String> = sqlx::query_scalar(
        "SELECT turn_id FROM owned_turn WHERE session_id = ?1 AND causation = ?2 \
         ORDER BY queue_index ASC LIMIT 1",
    )
    .bind(session.as_str())
    .bind(first_request.as_str())
    .fetch_optional(&mut **tx)
    .await
    .db()?;
    turn.map(|text| {
        TurnId::new(&text).map_err(|_| PortError::Corrupt("owned_turn.turn_id is malformed"))
    })
    .transpose()
}

/// 既有受理行的状态（`None` = 尚无该 `(actor, requestId)` 行）。
async fn command_status_of(
    tx: &mut Transaction<'_, Sqlite>,
    actor: &Actor,
    request: &RequestId,
) -> Result<Option<String>, PortError> {
    let (kind, id) = actor_columns(actor);
    Ok(sqlx::query_scalar(
        "SELECT status FROM owned_command WHERE actor_kind = ?1 AND actor_id = ?2 AND request_id = ?3",
    )
    .bind(kind)
    .bind(id.as_str())
    .bind(request.as_str())
    .fetch_optional(&mut **tx)
    .await
    .db()?)
}

/// 写一条命令终态：**先**判定既有行仍是 `accepted`，再写 terminal event，最后经 [`apply_terminal`] 推进行。
///
/// 终态行只认 `accepted`（§11.2「同一个 request 最多产生一个 command terminal event」）：重复/并发的
/// 结算在写任何事件之前就得到 `Conflict(Consumed)`，因此不会追加第二条 `command.*`，也不会改写
/// `current_request_id`。`existing` 是调用方在同一事务里已读到的状态，避免二次查询。
#[allow(clippy::too_many_arguments)]
async fn settle_command(
    tx: &mut Transaction<'_, Sqlite>,
    session: &SessionId,
    record: &FirstSubmissionRecord,
    request: &RequestId,
    outcome: RequestOutcome,
    error: Option<&PublicError>,
    at: &Timestamp,
    actor: &Actor,
    existing: Option<&str>,
    dispatched: bool,
    request_fingerprint: Option<&Digest>,
    window: &RetentionPolicy,
) -> Result<(), PortError> {
    let (kind, id) = actor_columns(actor);
    match existing {
        Some("accepted") => {}
        Some(_) => return Err(PortError::Conflict(ConflictKind::Consumed)),
        None => {
            let Some(fingerprint) = request_fingerprint else {
                // 无法证明的受理行（例如继续尝试的崩溃窗口）：不伪造指纹，也不改不存在的行。
                return Ok(());
            };
            sqlx::query(
                "INSERT INTO owned_command (actor_kind, actor_id, request_id, session_id, command, \
                 kind, expected_version, request_fingerprint, accepted_at, status) \
                 VALUES (?1, ?2, ?3, ?4, 'session.start', 'mutation', NULL, ?5, ?6, 'accepted')",
            )
            .bind(kind)
            .bind(id.as_str())
            .bind(request.as_str())
            .bind(session.as_str())
            .bind(fingerprint.as_str())
            .bind(at.as_str())
            .execute(&mut **tx)
            .await
            .db()?;
        }
    }
    let turn = first_submission_turn(tx, session, &record.first_request_id).await?;
    // §11.9：`completed` 的 `view.result` 与 `owned_command.result_json` 是同一份六字段投影。
    let result = (outcome == RequestOutcome::Completed)
        .then(|| first_submission_result(record, request, outcome, dispatched));
    let event_type = match outcome {
        RequestOutcome::Completed => "command.completed",
        RequestOutcome::Failed => "command.failed",
        RequestOutcome::Uncertain => "command.uncertain",
    };
    let view = terminal_view(request, outcome, error, result.as_ref());
    let event_id = write_session_event(
        tx,
        session,
        &SessionEventWrite {
            event_type,
            kind: EventKind::Structured,
            policy: StoredPolicy::Durable,
            origin: actor_origin(actor),
            turn: turn.as_ref(),
            causation: Some(request),
            view: &view,
            at,
            window,
        },
    )
    .await?;
    let result_json = result
        .map(|value| {
            ViewJson::new(&value.to_string())
                .map_err(|_| PortError::Corrupt("terminal result is not a json object"))
        })
        .transpose()?;
    let terminal = CommandTerminalRecord::try_new(
        command_status(outcome),
        Some(at.clone()),
        Some(event_id.clone()),
        result_json.map(CommandResult::new),
        error.cloned(),
    )
    .map_err(|_| {
        PortError::InvalidRequest("a command terminal requires a consistent result and error")
    })?;
    // 行的推进只走既有 writer：`result_json`/`terminal_event_id`/`Conflict(Consumed)` 与 owned 批量
    // 提交路径逐字同口径。
    apply_terminal(
        tx,
        Some(session),
        request,
        &terminal,
        &event_id,
        Some(actor),
    )
    .await
}

/// 写一次首条结算：首条关联 → 领域状态 → **领域事件** → 命令终态（§11.2 的 sequence 顺序）。
///
/// turn 终态只由**本事务确实推进的**首条 turn 产生：结算前 turn 已是终态（例如 `agent_host::
/// finish_turn` + broker `commit_chunk` 已经收敛）时不再改 `ended_at`/会话版本，也不再写第二个
/// `turn.*`。`session.updated` 仍只由 core 生成（§6 第 1 条、`MODULE_ARCHITECTURE.md` §4.7）：
/// 存储层只写自己推进的 turn 终态事件，不伪造会话摘要事件。
#[allow(clippy::too_many_arguments)]
async fn write_settlement(
    tx: &mut Transaction<'_, Sqlite>,
    session: &SessionId,
    record: &FirstSubmissionRecord,
    request: &RequestId,
    outcome: RequestOutcome,
    error: Option<&PublicError>,
    at: &Timestamp,
    actor: &Actor,
    dispatched: bool,
    request_fingerprint: Option<&Digest>,
    window: &RetentionPolicy,
) -> Result<(), PortError> {
    // 终态只认 `accepted`：重复/并发结算在写任何行或事件之前就 Conflict，不改 `current_request_id`。
    let existing = command_status_of(tx, actor, request).await?;
    if existing
        .as_deref()
        .is_some_and(|status| status != "accepted")
    {
        return Err(PortError::Conflict(ConflictKind::Consumed));
    }
    let (code, message, details, retryable) = match error {
        Some(error) => (
            Some(error.code().to_owned()),
            Some(error.message().to_owned()),
            Some(error.details().as_str().to_owned()),
            Some(i64::from(error.retryable())),
        ),
        None => (None, None, None, None),
    };
    let stage = if dispatched {
        "terminal"
    } else {
        "saved_not_dispatched"
    };
    sqlx::query(
        "UPDATE owned_first_submission SET current_request_id = ?1, stage = ?2, \
         terminal_outcome = ?3, terminal_at = ?4, error_code = ?5, error_message = ?6, \
         error_details_json = ?7, retryable = ?8, updated_at = ?9 WHERE session_id = ?10",
    )
    .bind(request.as_str())
    .bind(stage)
    .bind(outcome.as_str())
    .bind(at.as_str())
    .bind(code)
    .bind(message)
    .bind(details)
    .bind(retryable)
    .bind(at.as_str())
    .bind(session.as_str())
    .execute(&mut **tx)
    .await
    .db()?;
    // turn/会话摘要：只有已跨越派发边界的终态才结束本轮；可证明未派发的失败保留 queued turn，
    // 等待显式继续复用同一 turn/正文。uncertain 保持运行事实（可能仍有任务）。
    //
    // **不重复结束已终态的 turn**：`agent_host::finish_turn` + broker 的 `commit_chunk` 已经写过该
    // turn 的终态、`ended_at` 与 `session.updated`（版本 +1）。这里用 CAS 只推进仍非终态的首条 turn，
    // 且会话状态/版本只在本次确实推进了 turn 时更新、turn 事件只在该转换成功时写——否则结算会变成
    // 同一 turn 的第二个终态 writer。
    let ended_turn = match outcome {
        RequestOutcome::Completed | RequestOutcome::Failed if dispatched => {
            let turn_state = if outcome == RequestOutcome::Completed {
                TurnState::Completed
            } else {
                TurnState::Failed
            };
            let affected = sqlx::query(
                "UPDATE owned_turn SET state = ?1, ended_at = ?2 WHERE session_id = ?3 \
                 AND causation = (SELECT first_request_id FROM owned_first_submission WHERE session_id = ?3) \
                 AND state NOT IN ('completed','failed','cancelled')",
            )
            .bind(turn_state.as_str())
            .bind(at.as_str())
            .bind(session.as_str())
            .execute(&mut **tx)
            .await
            .db()?
            .rows_affected();
            if affected == 1 {
                let turn = first_submission_turn(tx, session, &record.first_request_id).await?;
                if turn.is_some() {
                    sqlx::query(
                        "UPDATE owned_session SET state = 'idle', version = version + 1, \
                         updated_at = ?1 WHERE session_id = ?2",
                    )
                    .bind(at.as_str())
                    .bind(session.as_str())
                    .execute(&mut **tx)
                    .await
                    .db()?;
                }
                turn
            } else {
                None
            }
        }
        _ => None,
    };
    // §11.2：领域事件先于命令终态落盘；订阅方因此先看到本轮结束，再看到命令结果。
    if let Some(turn) = &ended_turn {
        let event_type = if outcome == RequestOutcome::Completed {
            "turn.completed"
        } else {
            "turn.failed"
        };
        let view = turn_terminal_view(turn, outcome, error);
        write_session_event(
            tx,
            session,
            &SessionEventWrite {
                event_type,
                kind: EventKind::State,
                policy: StoredPolicy::Durable,
                origin: actor_origin(actor),
                turn: Some(turn),
                causation: Some(request),
                view: &view,
                at,
                window,
            },
        )
        .await?;
    }
    settle_command(
        tx,
        session,
        record,
        request,
        outcome,
        error,
        at,
        actor,
        existing.as_deref(),
        dispatched,
        request_fingerprint,
        window,
    )
    .await
}

impl SqliteStore {
    /// 首次提交事务主体（不含提交/认领收尾）。
    async fn commit_first_submission_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        write: &FirstSubmissionWrite,
        claimed: Option<&[acp_core::model::ClaimedAttachment]>,
    ) -> Result<FirstSubmissionAdmission, PortError> {
        // 同一会话已有首条：幂等返回既有记录（不追加第二条、不重建准备）。
        if let Some(row) = load_row(tx, &write.session).await? {
            let record = record_from_row(&row)?;
            return Ok(FirstSubmissionAdmission::AlreadyCommitted(record));
        }
        let existing_session: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM owned_session WHERE session_id = ?1")
                .bind(write.session.as_str())
                .fetch_optional(&mut **tx)
                .await
                .db()?;
        if existing_session.is_some() {
            return Err(PortError::Conflict(ConflictKind::AlreadyExists));
        }
        let (kind, actor_id) = actor_columns(&write.actor);
        let existing_command: Option<(String, String)> = sqlx::query_as(
            "SELECT request_fingerprint, command FROM owned_command \
             WHERE actor_kind = ?1 AND actor_id = ?2 AND request_id = ?3",
        )
        .bind(kind)
        .bind(&actor_id)
        .bind(write.request_id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .db()?;
        if let Some((fingerprint, command)) = existing_command {
            if fingerprint != write.request_fingerprint.as_str() || command != "session.start" {
                return Err(PortError::Conflict(ConflictKind::IdempotencyConflict));
            }
        }

        // ---- 正式会话行（首条受理后即存在；没有派发仍可读）
        let (mode_id, mode_name) = match &write.mode {
            Some(mode) => (
                Some(mode.mode_id().as_str().to_owned()),
                Some(mode.display_name().to_owned()),
            ),
            None => (None, None),
        };
        sqlx::query(
            "INSERT INTO owned_session (session_id, title, agent_id, agent_name, state, origin_epoch, \
             current_mode_id, current_mode_name, version, created_at, updated_at, agent_session_id, \
             workspace_cwd, workspace_directory_id) \
             VALUES (?1, NULL, ?2, ?3, 'running', ?4, ?5, ?6, 1, ?7, ?7, NULL, ?8, ?9)",
        )
        .bind(write.session.as_str())
        .bind(write.agent.agent_id().as_str())
        .bind(write.agent.name())
        .bind(write.origin_epoch.as_str())
        .bind(mode_id)
        .bind(mode_name)
        .bind(write.at.as_str())
        .bind(write.workspace_cwd.as_deref())
        .bind(write.workspace_directory.as_ref().map(|id| id.as_str()))
        .execute(&mut **tx)
        .await
        .db()?;
        if let (Some(directory_id), Some(cwd)) = (&write.workspace_directory, &write.workspace_cwd)
        {
            sqlx::query(
                "INSERT INTO owned_directory (directory_id, canonical_path, created_at, last_used_at) \
                 VALUES (?1, ?2, ?3, ?3) ON CONFLICT(directory_id) DO UPDATE SET \
                 last_used_at = excluded.last_used_at",
            )
            .bind(directory_id.as_str())
            .bind(cwd)
            .bind(write.at.as_str())
            .execute(&mut **tx)
            .await
            .db()?;
        }

        // ---- 稳定用户消息身份（channel = 'user'）：不需要 Agent 回显即可读取
        sqlx::query(
            "INSERT INTO owned_message (message_id, session_id, turn_id, channel, state, \
             content_json, partial_reason, created_at, updated_at) \
             VALUES (?1, ?2, ?3, 'user', 'completed', ?4, NULL, ?5, ?5)",
        )
        .bind(write.user_message_id.as_str())
        .bind(write.session.as_str())
        .bind(write.turn.as_str())
        .bind(write.input.json().as_str())
        .bind(write.at.as_str())
        .execute(&mut **tx)
        .await
        .db()?;

        // ---- turn（queued：已受理、尚未派发；派发围栏提交时转 running）
        sqlx::query(
            "INSERT INTO owned_turn (turn_id, session_id, state, queue_index, causation, started_at, ended_at) \
             VALUES (?1, ?2, 'queued', 1, ?3, NULL, NULL)",
        )
        .bind(write.turn.as_str())
        .bind(write.session.as_str())
        .bind(write.request_id.as_str())
        .execute(&mut **tx)
        .await
        .db()?;

        // ---- 命令受理（幂等键 (actor, requestId)）
        sqlx::query(
            "INSERT INTO owned_command (actor_kind, actor_id, request_id, session_id, command, kind, \
             expected_version, request_fingerprint, accepted_at, status) \
             VALUES (?1, ?2, ?3, ?4, 'session.start', 'mutation', NULL, ?5, ?6, 'accepted') \
             ON CONFLICT(actor_kind, actor_id, request_id) DO NOTHING",
        )
        .bind(kind)
        .bind(&actor_id)
        .bind(write.request_id.as_str())
        .bind(write.session.as_str())
        .bind(write.request_fingerprint.as_str())
        .bind(write.at.as_str())
        .execute(&mut **tx)
        .await
        .db()?;

        // ---- 首条关联（阶段固定为可证明未派发）
        sqlx::query(
            "INSERT INTO owned_first_submission (session_id, first_submission_id, user_message_id, \
             first_request_id, current_request_id, actor_kind, actor_id, request_fingerprint, \
             input_json, input_digest, mode_id, config_json, stage, ever_dispatched, \
             agent_session_id, resource_epoch, terminal_outcome, terminal_at, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'saved_not_dispatched', 0, \
             NULL, NULL, NULL, NULL, ?12, ?12)",
        )
        .bind(write.session.as_str())
        .bind(write.first_submission_id.as_str())
        .bind(write.user_message_id.as_str())
        .bind(write.request_id.as_str())
        .bind(kind)
        .bind(&actor_id)
        .bind(write.request_fingerprint.as_str())
        .bind(write.input.json().as_str())
        .bind(write.input.digest().as_str())
        .bind(write.mode.as_ref().map(|mode| mode.mode_id().as_str()))
        .bind(config_json(&write.config)?)
        .bind(write.at.as_str())
        .execute(&mut **tx)
        .await
        .db()?;

        // ---- 初始事件（D7「正式消息投影与初始事件在同事务写」）
        //
        // 首条用户消息的正文权威仍是 `owned_message.content_json`；这条事件只负责让它在事件流里可见，
        // 并用 `source` 标明来源不是 Agent 回显（`FIRST_USER_MESSAGE_SOURCE`）。它必须与受理同事务、
        // 序号早于该命令的终态事件（终态在结算事务里才写）。
        let initial_view = {
            let text = first_message_text(write.input.json().as_str())?;
            ViewJson::new(
                &serde_json::json!({
                    "messageId": write.user_message_id.as_str(),
                    "turnId": write.turn.as_str(),
                    "deltaIndex": "0",
                    "text": text,
                    "source": FIRST_USER_MESSAGE_SOURCE,
                })
                .to_string(),
            )
            .map_err(|_| PortError::Corrupt("first message view is not a json object"))?
        };
        let window = self.window();
        write_session_event(
            tx,
            &write.session,
            &SessionEventWrite {
                event_type: "user.message.delta",
                kind: EventKind::Delta,
                policy: StoredPolicy::ShortTerm,
                origin: actor_origin(&write.actor),
                turn: Some(&write.turn),
                causation: Some(&write.request_id),
                view: &initial_view,
                at: &write.at,
                window: &window,
            },
        )
        .await?;

        // ---- 附件认领（已封存事实 → 正式行引用；不复制、不全量重哈希）
        if let (Some(claim), Some(attachments)) = (&write.manifest, claimed) {
            for (position, attachment) in attachments.iter().enumerate() {
                let attachment_id = self
                    .claim_attachment(
                        tx,
                        &write.session,
                        write.user_message_id.clone(),
                        position,
                        attachment,
                        &claim.submission,
                        claim.generation,
                        &write.at,
                    )
                    .await?;
                let _ = attachment_id;
            }
        }

        crate::session_store::pin_message_position(tx, &write.session, &write.user_message_id)
            .await?;
        let row = load_row(tx, &write.session)
            .await?
            .ok_or(PortError::Corrupt(
                "first submission row vanished after insert",
            ))?;
        let record = record_from_row(&row)?;
        Ok(FirstSubmissionAdmission::Accepted(record))
    }

    /// 认领一个已封存附件：解析/复用 `owned_attachment`（内容寻址），写会话引用与消息媒体引用。
    ///
    /// 字节的 `rename` 走 [`crate::staging::PreparationStaging::claim_bytes`]：只有本清单真的搬走字节
    /// 才记录，失败回滚因此只还原本清单自己搬走的摘要。
    #[allow(clippy::too_many_arguments)]
    async fn claim_attachment(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        session: &SessionId,
        message: MessageId,
        position: usize,
        claimed: &acp_core::model::ClaimedAttachment,
        submission: &SubmissionId,
        generation: ResourceGeneration,
        at: &Timestamp,
    ) -> Result<acp_core::model::AttachmentId, PortError> {
        let existing: Option<(String, i64, String)> = sqlx::query_as(
            "SELECT attachment_id, byte_length, media_type FROM owned_attachment WHERE sha256 = ?1",
        )
        .bind(claimed.sha256.as_str())
        .fetch_optional(&mut **tx)
        .await
        .db()?;
        let (attachment_id, byte_length, media_type) = match existing {
            Some((id, length, media)) => {
                let id: acp_core::model::AttachmentId = id.parse().map_err(|_| {
                    PortError::Corrupt("owned_attachment.attachment_id is malformed")
                })?;
                (id, length, media)
            }
            None => {
                // 同一受管内容区内的 rename：不复制、不重新计算摘要。目标路径已存在时（同内容已被
                // 更早的会话提交）不动字节，也不记入回滚集合。
                self.staging.claim_bytes(submission, &claimed.sha256)?;
                let id = claimed.attachment.clone();
                sqlx::query(
                    "INSERT INTO owned_attachment (attachment_id, sha256, byte_length, media_type, \
                     relative_path, created_at, last_used_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                )
                .bind(id.as_str())
                .bind(claimed.sha256.as_str())
                .bind(
                    i64::try_from(claimed.length)
                        .map_err(|_| PortError::Corrupt("attachment length is too large"))?,
                )
                .bind(&claimed.media_type)
                .bind(format!("attachments/{}", claimed.sha256.as_str()))
                .bind(at.as_str())
                .execute(&mut **tx)
                .await
                .db()?;
                (id, claimed.length as i64, claimed.media_type.clone())
            }
        };
        if i64::try_from(claimed.length).ok() != Some(byte_length) {
            return Err(PortError::Conflict(ConflictKind::AlreadyExists));
        }
        sqlx::query(
            "INSERT INTO owned_attachment_link (session_id, attachment_id, generation, sha256) \
             VALUES (?1, ?2, ?3, ?4) ON CONFLICT(session_id, attachment_id) DO NOTHING",
        )
        .bind(session.as_str())
        .bind(attachment_id.as_str())
        .bind(
            i64::try_from(generation.get())
                .map_err(|_| PortError::Corrupt("resource generation is too large"))?,
        )
        .bind(claimed.sha256.as_str())
        .execute(&mut **tx)
        .await
        .db()?;
        sqlx::query(
            "INSERT INTO owned_media_ref (message_id, position, attachment_id, kind) \
             VALUES (?1, ?2, ?3, ?4) ON CONFLICT(message_id, position) DO NOTHING",
        )
        .bind(message.as_str())
        .bind(i64::try_from(position).unwrap_or(i64::MAX))
        .bind(attachment_id.as_str())
        .bind(media_kind_of(&media_type))
        .execute(&mut **tx)
        .await
        .db()?;
        Ok(attachment_id)
    }
}

#[async_trait]
impl FirstSubmission for SqliteStore {
    async fn commit_first_submission(
        &self,
        write: FirstSubmissionWrite,
    ) -> Result<FirstSubmissionAdmission, PortError> {
        self.writable()?;
        let device = match &write.actor {
            Actor::Device { device } => device.clone(),
            _ => {
                return Err(PortError::InvalidRequest(
                    "a first submission requires an authenticated device",
                ));
            }
        };
        // 先做一次只读幂等检查，避免为已受理的会话创建暂存认领。
        {
            let mut read = self.acquire_read().await?;
            let existing = sqlx::query_scalar::<_, String>(
                "SELECT first_submission_id FROM owned_first_submission WHERE session_id = ?1",
            )
            .bind(write.session.as_str())
            .fetch_optional(&mut *read)
            .await
            .db()?;
            drop(read);
            if existing.is_some() {
                let record = self
                    .first_submission(write.session.clone())
                    .await?
                    .ok_or(PortError::Corrupt("first submission vanished"))?;
                return Ok(FirstSubmissionAdmission::AlreadyCommitted(record));
            }
        }
        let claim = match &write.manifest {
            None => None,
            Some(manifest) => Some(StagedClaim {
                device,
                session: write.session.clone(),
                submission: manifest.submission.clone(),
                runtime_epoch: manifest.runtime_epoch.clone(),
                generation: manifest.generation,
            }),
        };
        let claimed = match &claim {
            Some(claim) => Some(self.staging.begin_claim(claim)?),
            None => None,
        };
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let outcome = self
            .commit_first_submission_tx(&mut tx, &write, claimed.as_deref())
            .await;
        match outcome {
            Ok(admission) => {
                if let Err(error) = tx.commit().await.db() {
                    if let Some(claim) = &claim {
                        self.staging.finish_claim(&claim.submission, false);
                    }
                    return Err(error.into());
                }
                if let Some(claim) = &claim {
                    self.staging.finish_claim(&claim.submission, true);
                }
                Ok(admission)
            }
            Err(error) => {
                drop(tx);
                if let Some(claim) = &claim {
                    self.staging.finish_claim(&claim.submission, false);
                }
                Err(error)
            }
        }
    }

    async fn first_submission(
        &self,
        session: SessionId,
    ) -> Result<Option<FirstSubmissionRecord>, PortError> {
        let mut read = self.acquire_read().await?;
        let row = sqlx::query(&format!(
            "SELECT {FIRST_SUBMISSION_COLUMNS} FROM owned_first_submission WHERE session_id = ?1"
        ))
        .bind(session.as_str())
        .fetch_optional(&mut *read)
        .await
        .db()?;
        row.as_ref()
            .map(record_from_row)
            .transpose()
            .map_err(Into::into)
    }

    async fn first_user_message(
        &self,
        session: SessionId,
    ) -> Result<Option<FirstUserMessage>, PortError> {
        let mut read = self.acquire_read().await?;
        load_first_message(&mut read, &session).await
    }

    async fn begin_dispatch(&self, fence: DispatchFence) -> Result<DispatchAdmission, PortError> {
        self.writable()?;
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let Some(row) = load_row(&mut tx, &fence.session).await? else {
            return Ok(DispatchAdmission::NotEligible);
        };
        let record = record_from_row(&row)?;
        if record.first_submission_id != fence.first_submission_id
            || record.stage != FirstSubmissionStage::SavedNotDispatched
            || record.ever_dispatched
        {
            return Ok(DispatchAdmission::NotEligible);
        }
        match fence.retry_of.as_ref() {
            None => {
                if fence.request_id != record.first_request_id
                    || record.current_request_id != record.first_request_id
                {
                    return Ok(DispatchAdmission::NotEligible);
                }
            }
            Some(previous) => {
                if previous == &fence.request_id || previous != &record.current_request_id {
                    return Ok(DispatchAdmission::NotEligible);
                }
                let status: Option<String> = sqlx::query_scalar(
                    "SELECT status FROM owned_command WHERE session_id = ?1 AND request_id = ?2 \
                     AND command = 'session.start'",
                )
                .bind(fence.session.as_str())
                .bind(previous.as_str())
                .fetch_optional(&mut *tx)
                .await
                .db()?;
                if status.as_deref() != Some("failed") {
                    return Ok(DispatchAdmission::NotEligible);
                }
            }
        }
        // 继续尝试的受理行：同一 (actor, requestId) 必须一致；已终态则不得再次派发。
        let (kind, actor_id) = actor_columns(&fence.actor);
        let existing: Option<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT request_fingerprint, command, status FROM owned_command \
             WHERE actor_kind = ?1 AND actor_id = ?2 AND request_id = ?3",
        )
        .bind(kind)
        .bind(&actor_id)
        .bind(fence.request_id.as_str())
        .fetch_optional(&mut *tx)
        .await
        .db()?;
        match existing {
            Some((fingerprint, command, status)) => {
                if fingerprint != fence.request_fingerprint.as_str() || command != "session.start" {
                    return Err(PortError::Conflict(ConflictKind::IdempotencyConflict));
                }
                if status.as_deref() != Some("accepted") {
                    return Ok(DispatchAdmission::NotEligible);
                }
            }
            None => {
                sqlx::query(
                    "INSERT INTO owned_command (actor_kind, actor_id, request_id, session_id, \
                     command, kind, expected_version, request_fingerprint, accepted_at, status) \
                     VALUES (?1, ?2, ?3, ?4, 'session.start', 'mutation', NULL, ?5, ?6, 'accepted')",
                )
                .bind(kind)
                .bind(&actor_id)
                .bind(fence.request_id.as_str())
                .bind(fence.session.as_str())
                .bind(fence.request_fingerprint.as_str())
                .bind(fence.at.as_str())
                .execute(&mut *tx)
                .await
                .db()?;
            }
        }
        // 跨越派发边界即开启**新尝试**：`terminal_*`/错误列描述的是「当前尝试」的终态，上一次
        // （failed）尝试的取值必须在同一事务里清掉。留着它会让本次尝试的结算被误判为「已终态」，
        // 命令永远停在 accepted（D8.1：每次继续都是新的 requestId，其终态由 own 记录提供）。
        let affected = sqlx::query(
            "UPDATE owned_first_submission SET stage = 'dispatch_started', ever_dispatched = 1, \
             current_request_id = ?1, agent_session_id = ?2, resource_epoch = ?3, updated_at = ?4, \
             terminal_outcome = NULL, terminal_at = NULL, error_code = NULL, \
             error_message = NULL, error_details_json = NULL, retryable = NULL \
             WHERE session_id = ?5 AND first_submission_id = ?6 \
             AND stage = 'saved_not_dispatched' AND ever_dispatched = 0",
        )
        .bind(fence.request_id.as_str())
        .bind(fence.agent_session.as_str())
        .bind(fence.resource_generation.get().to_string())
        .bind(fence.at.as_str())
        .bind(fence.session.as_str())
        .bind(fence.first_submission_id.as_str())
        .execute(&mut *tx)
        .await
        .db()?
        .rows_affected();
        if affected != 1 {
            return Ok(DispatchAdmission::NotEligible);
        }
        sqlx::query(
            "UPDATE owned_turn SET state = 'running', started_at = COALESCE(started_at, ?1) \
             WHERE session_id = ?2 AND state = 'queued'",
        )
        .bind(fence.at.as_str())
        .bind(fence.session.as_str())
        .execute(&mut *tx)
        .await
        .db()?;
        sqlx::query(
            "UPDATE owned_session SET state = 'running', updated_at = ?1 WHERE session_id = ?2",
        )
        .bind(fence.at.as_str())
        .bind(fence.session.as_str())
        .execute(&mut *tx)
        .await
        .db()?;
        tx.commit().await.db()?;
        Ok(DispatchAdmission::Granted)
    }

    async fn settle_first_submission(
        &self,
        settlement: FirstSubmissionSettlement,
    ) -> Result<FirstSubmissionRecord, PortError> {
        self.writable()?;
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let Some(row) = load_row(&mut tx, &settlement.session).await? else {
            return Err(PortError::NotFound(EntityRef::Session(settlement.session)));
        };
        let record = record_from_row(&row)?;
        // §11.2/D8：同一 requestId 重放只读原终态——无论终态是派发后的 `terminal` 还是可证明未派发的
        // 失败，都不再追加事件、不改 `current_request_id`。
        //
        // `DispatchStarted` 不算「原终态」：那一列记录的是**上一次**尝试的终态（继续的前提就是它
        // failed），新尝试的 requestId 已由派发围栏写入 `current_request_id`。若在这里提前返回，
        // 这次尝试永远拿不到自己的命令终态（客户端只能等到观察预算耗尽）。
        if record.current_request_id == settlement.request_id
            && record.stage != FirstSubmissionStage::DispatchStarted
            && (record.stage == FirstSubmissionStage::Terminal || record.terminal.is_some())
        {
            return Ok(record);
        }
        let dispatched = record.stage == FirstSubmissionStage::DispatchStarted;
        if dispatched && settlement.request_id != record.current_request_id {
            return Err(PortError::InvalidRequest(
                "a dispatched attempt can only settle its own request id",
            ));
        }
        if !dispatched && settlement.outcome != RequestOutcome::Failed {
            return Err(PortError::InvalidRequest(
                "only a provably-not-dispatched attempt may settle before the dispatch fence",
            ));
        }
        if settlement.outcome != RequestOutcome::Completed && settlement.error.is_none() {
            return Err(PortError::InvalidRequest(
                "a failed or uncertain settlement requires a public error",
            ));
        }
        write_settlement(
            &mut tx,
            &settlement.session,
            &record,
            &settlement.request_id,
            settlement.outcome,
            settlement.error.as_ref(),
            &settlement.at,
            &settlement.actor,
            dispatched,
            Some(&settlement.request_fingerprint),
            &self.window(),
        )
        .await?;
        let row = load_row(&mut tx, &settlement.session)
            .await?
            .ok_or(PortError::Corrupt(
                "first submission vanished after settlement",
            ))?;
        let record = record_from_row(&row)?;
        tx.commit().await.db()?;
        Ok(record)
    }

    async fn converge_first_submission(
        &self,
        session: SessionId,
        at: Timestamp,
    ) -> Result<FirstSubmissionConvergence, PortError> {
        self.writable()?;
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let Some(row) = load_row(&mut tx, &session).await? else {
            return Ok(FirstSubmissionConvergence::NotPreparation);
        };
        let record = record_from_row(&row)?;
        match record.stage {
            FirstSubmissionStage::Terminal => {
                tx.commit().await.db()?;
                return Ok(FirstSubmissionConvergence::Terminal(record));
            }
            FirstSubmissionStage::DispatchStarted => {
                let error = PublicError::coded(
                    "command.uncertain",
                    "重启后无法确认首条是否已派发，不会自动重发",
                    false,
                )
                .map_err(PortError::from)?;
                write_settlement(
                    &mut tx,
                    &session,
                    &record,
                    &record.current_request_id,
                    RequestOutcome::Uncertain,
                    Some(&error),
                    &at,
                    &actor_from_columns(&text(&row, "actor_kind")?, &text(&row, "actor_id")?)?,
                    true,
                    None,
                    &self.window(),
                )
                .await?;
                let row = load_row(&mut tx, &session)
                    .await?
                    .ok_or(PortError::Corrupt("first submission vanished"))?;
                let record = record_from_row(&row)?;
                tx.commit().await.db()?;
                return Ok(FirstSubmissionConvergence::Uncertain(record));
            }
            FirstSubmissionStage::SavedNotDispatched => {
                if record
                    .terminal
                    .as_ref()
                    .is_some_and(|terminal| terminal.outcome == RequestOutcome::Failed)
                {
                    tx.commit().await.db()?;
                    return Ok(FirstSubmissionConvergence::ProvablyNotDispatched(record));
                }
                let error = PublicError::coded(
                    "internal.unavailable",
                    "重启后确认首条未派发，等待显式继续",
                    true,
                )
                .map_err(PortError::from)?;
                let actor_kind = text(&row, "actor_kind")?;
                let actor_id = text(&row, "actor_id")?;
                let actor = actor_from_columns(&actor_kind, &actor_id)?;
                write_settlement(
                    &mut tx,
                    &session,
                    &record,
                    &record.current_request_id,
                    RequestOutcome::Failed,
                    Some(&error),
                    &at,
                    &actor,
                    false,
                    None,
                    &self.window(),
                )
                .await?;
                let row = load_row(&mut tx, &session)
                    .await?
                    .ok_or(PortError::Corrupt("first submission vanished"))?;
                let record = record_from_row(&row)?;
                tx.commit().await.db()?;
                Ok(FirstSubmissionConvergence::ProvablyNotDispatched(record))
            }
        }
    }
}

fn actor_from_columns(kind: &str, id: &str) -> Result<Actor, PortError> {
    match kind {
        "device" => acp_core::model::DeviceId::new(id)
            .map(|device| Actor::Device { device })
            .map_err(|_| PortError::Corrupt("first submission actor id is malformed")),
        "node" => {
            let (node, access) = id
                .split_once('/')
                .ok_or(PortError::Corrupt("node actor id is malformed"))?;
            Ok(Actor::Node {
                node: acp_core::model::NodeId::new(node)
                    .map_err(|_| PortError::Corrupt("node actor id is malformed"))?,
                access_node: acp_core::model::NodeId::new(access)
                    .map_err(|_| PortError::Corrupt("node actor id is malformed"))?,
            })
        }
        "cli" => Ok(Actor::LocalCli),
        _ => Err(PortError::Corrupt("first submission actor kind is unknown")),
    }
}

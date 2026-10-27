//! 本机会话生命周期的 Sync 面（`core.local-session-lifecycle.v1`）：
//! `session.view`/`session.view.result` 控制帧、owned 摘要的 `localActivity` 投影与
//! `session.activity.changed` 的投递边界（`docs/SYNC_PROTOCOL.md` §9.8/§11.8）。
//!
//! 三条边界：
//!
//! - **控制帧不启动 Agent、不写事件、不推进会话版本**：声明只更新运行期查看者与租约；激活必须另行派发
//!   `session.resume`。全部判定（版本、租期、替换与离页）由 core 的
//!   [`LocalSessionLifecycle`](acp_core::lifecycle::LocalSessionLifecycle) 承担，本模块只做 wire 形状、
//!   认证前提、feature 门控与限流。
//! - **租约只存在于连接期**：断开/撤销/替换立即清理（[`ViewRoute::on_closed`]），`control.ping` 不经过
//!   本模块，因此不会续租。
//! - **独立限流**：单连接 120 帧/分钟，只统计客户端 → 服务端的 `session.view`；`session.view.result`
//!   是服务端应答，不计数。持续超限与业务命令同规则，达到连续阈值才以 `4429` 关闭连接。

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use acp_core::broker::SessionDeleteAdmission;
use acp_core::lifecycle::{LocalSessionLifecycle, RetentionSnapshot};
use acp_core::model::{
    Actor, CommandStatus, ConnectionId, Digest, PortError, PublicError, RequestId,
    SessionDeletionBlocker, SessionDeletionEligibility, SessionDeletionOutcome,
    SessionDeletionWrite, SessionId, Timestamp,
};
use acp_core::ports::{Clock, ReclaimCandidate, ReleaseOutcome, SessionQuery};
use acp_core::use_cases::UseCases;
use sync_protocol::common::{
    LocalActivity, Nullable, RawObject, Text, Timestamp as WireTimestamp, Uuid,
};
use sync_protocol::envelope::{Envelope, MessageType};
use sync_protocol::error::{Body, ErrorCode};
use sync_protocol::sync::{
    SessionView, SessionViewResult as WireSessionViewResult, ViewStatus as WireViewStatus,
};
use tracing::debug;

use crate::sync::command::limits::MAX_CONSECUTIVE_RATE_LIMIT_REJECTIONS;
use crate::sync::conn::{ConnectionHandle, MessageRoute, RouteOutcome, close};
use crate::transport::net::ratelimit::MAX_TRACKED_KEYS;

/// `core.local-session-lifecycle.v1`：本机生命周期投影与激活的唯一 feature ID。
pub const LOCAL_LIFECYCLE_FEATURE: &str = "core.local-session-lifecycle.v1";

/// `session.delete` 的 closed payload 是 `{}`；摘要固定对应这份规范字节。
pub fn session_delete_fingerprint() -> Digest {
    Digest::new("RBNvo1WzZ4oRRq0W9-hknpT7T8If536DEMBg9hyq_4o")
        .expect("the session.delete fingerprint is canonical")
}

/// 一次扫描候选及其生命周期预期。
///
/// 候选本身只说明扫描时安全；进入删除 gate 后必须重新确认资源代、保留期起点和待删除阶段都没有
/// 变化，再重新执行到期与保护检查。
#[derive(Debug, Clone)]
pub struct RetentionDeleteCandidate {
    candidate: ReclaimCandidate,
    snapshot: RetentionSnapshot,
}

/// 为扫描器返回的候选冻结当前生命周期预期。
pub fn retention_delete_candidate(
    lifecycle: &LocalSessionLifecycle,
    candidate: ReclaimCandidate,
) -> Result<Option<RetentionDeleteCandidate>, PortError> {
    let Some(snapshot) = lifecycle.retention_snapshot(&candidate.session) else {
        return Ok(None);
    };
    Ok(Some(RetentionDeleteCandidate {
        candidate,
        snapshot,
    }))
}

/// 单连接查看控制帧速率（§11.8/§14：120 帧/分钟，只统计 `session.view`）。
pub const VIEWS_PER_MINUTE: u32 = 120;
/// 查看控制帧的限流窗口（固定 1 分钟）。
pub const VIEW_WINDOW: Duration = Duration::from_secs(60);

/// 一条连接的查看控制帧计数窗口。
#[derive(Debug, Default)]
struct ViewRateState {
    attempts: VecDeque<Instant>,
    consecutive_limited: u32,
}

/// 按连接计数的查看控制帧限流器（键是连接，不是设备：额度属于「这条连接的页面」）。
#[derive(Debug)]
struct ViewRateLimiter {
    limit: u32,
    window: Duration,
    state: Mutex<HashMap<String, ViewRateState>>,
}

impl ViewRateLimiter {
    fn new(limit: u32, window: Duration) -> Self {
        Self {
            limit,
            window,
            state: Mutex::new(HashMap::new()),
        }
    }

    /// 记录一次 `session.view` 并返回「是否允许」；被拒时返回建议退避时长。
    ///
    /// 生产路径用 [`Self::check_at`] 以便注入服务端时钟；本包装留给不关心时钟的调用方与测试。
    #[allow(dead_code)]
    fn check(&self, connection: &str) -> Result<(), (Duration, bool)> {
        self.check_at(connection, Instant::now())
    }

    fn check_at(&self, connection: &str, now: Instant) -> Result<(), (Duration, bool)> {
        let mut state = lock(&self.state);
        if state.len() >= MAX_TRACKED_KEYS && !state.contains_key(connection) {
            // 键上限：失败关闭而不是重置既有计数（与业务命令限流同一纪律）。
            return Err((self.window, false));
        }
        let entry = state.entry(connection.to_owned()).or_default();
        while let Some(front) = entry.attempts.front() {
            if now.duration_since(*front) >= self.window {
                entry.attempts.pop_front();
            } else {
                break;
            }
        }
        if entry.attempts.len() >= self.limit as usize {
            entry.consecutive_limited += 1;
            let close_now = entry.consecutive_limited >= MAX_CONSECUTIVE_RATE_LIMIT_REJECTIONS;
            let retry_after = entry
                .attempts
                .front()
                .map(|front| self.window.saturating_sub(now.duration_since(*front)))
                .unwrap_or(self.window);
            return Err((retry_after, close_now));
        }
        entry.consecutive_limited = 0;
        entry.attempts.push_back(now);
        Ok(())
    }

    fn forget(&self, connection: &str) {
        lock(&self.state).remove(connection);
    }
}

/// 一条连接的查看控制状态：本次协商是否选中 feature，以及限流的连接级明细。
#[derive(Debug, Default)]
struct ViewConnectionState {
    negotiated: bool,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `session.view` / `session.view.result` 的路由。
pub struct ViewRoute {
    core: Arc<UseCases>,
    lifecycle: Arc<LocalSessionLifecycle>,
    clock: Arc<dyn Clock>,
    rate: ViewRateLimiter,
    states: Mutex<HashMap<String, ViewConnectionState>>,
}

impl std::fmt::Debug for ViewRoute {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ViewRoute")
            .field("connections", &lock(&self.states).len())
            .finish_non_exhaustive()
    }
}

impl ViewRoute {
    pub fn new(
        core: Arc<UseCases>,
        lifecycle: Arc<LocalSessionLifecycle>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            core,
            lifecycle,
            clock,
            rate: ViewRateLimiter::new(VIEWS_PER_MINUTE, VIEW_WINDOW),
            states: Mutex::new(HashMap::new()),
        }
    }

    fn negotiated(&self, handle: &ConnectionHandle) -> bool {
        lock(&self.states)
            .get(handle.connection_id().as_str())
            .map(|state| state.negotiated)
            .unwrap_or(false)
    }

    fn actor(handle: &ConnectionHandle) -> Actor {
        Actor::Device {
            device: handle.device_id().clone(),
        }
    }

    fn now(&self) -> Option<Timestamp> {
        Timestamp::new(self.clock.now().as_str()).ok()
    }

    /// 会话是否属于本机 owned 路径（不泄露远程来源是否存在）。
    async fn owned_session(&self, actor: &Actor, session: &SessionId) -> bool {
        self.core
            .list_sessions(actor, SessionQuery::default())
            .await
            .map(|sessions| {
                sessions
                    .iter()
                    .any(|summary| summary.session_id() == session)
            })
            .unwrap_or(false)
    }

    fn send_frame_error(
        &self,
        handle: &ConnectionHandle,
        code: ErrorCode,
        message: &str,
        retryable: bool,
        request: &Uuid,
        details: RawObject,
    ) {
        let Ok(text) = Text::<1024>::parse(message) else {
            return;
        };
        let body = Body {
            code,
            message: text,
            retryable,
            correlation_id: Nullable::value(request.clone()),
            details,
        };
        let _ = handle.send(MessageType::Error, &body);
    }

    fn rate_limited(
        &self,
        handle: &ConnectionHandle,
        request: &Uuid,
        retry_after: Duration,
        close_now: bool,
    ) {
        debug!(
            event = "sync.session_view_rate_limited",
            connection_id = handle.connection_id().as_str(),
            retry_after_ms = retry_after.as_millis() as u64,
            close = close_now,
            "the connection exceeded its session.view rate"
        );
        let details = RawObject::parse(&format!(
            r#"{{"retryAfterMs":{}}}"#,
            retry_after.as_millis() as u64
        ))
        .unwrap_or_else(|_| RawObject::empty());
        self.send_frame_error(
            handle,
            ErrorCode::ResourceRateLimited,
            "the session view rate limit was exceeded",
            true,
            request,
            details,
        );
        if close_now {
            handle.request_close(close::RATE_LIMITED, "session.view rate limit exceeded");
        }
    }

    fn schema_invalid(&self, handle: &ConnectionHandle, message: &Envelope) {
        self.send_frame_error(
            handle,
            ErrorCode::ProtocolSchemaInvalid,
            "the session.view frame does not match the v1 schema",
            false,
            message.message_id(),
            RawObject::empty(),
        );
    }

    async fn on_view(&self, handle: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        // 认证前提（与业务命令同一把闸）：设备必须仍在本机持久信任表里。
        let actor = Self::actor(handle);
        match self
            .core
            .sync_device_handshake_view(handle.device_id())
            .await
        {
            Ok(view) if view.device.is_some() => {}
            Ok(_) => {
                self.send_frame_error(
                    handle,
                    ErrorCode::AuthDeviceUnknown,
                    "the device is not known to this host",
                    false,
                    message.message_id(),
                    RawObject::empty(),
                );
                return RouteOutcome::Claimed;
            }
            Err(error) => {
                debug!(
                    event = "sync.session_view_device_failed",
                    connection_id = handle.connection_id().as_str(),
                    error = ?error,
                    "the persisted device snapshot cannot be read"
                );
                self.send_frame_error(
                    handle,
                    ErrorCode::InternalUnavailable,
                    "the device state cannot be read",
                    true,
                    message.message_id(),
                    RawObject::empty(),
                );
                return RouteOutcome::Claimed;
            }
        }
        if !self.negotiated(handle) {
            self.send_frame_error(
                handle,
                ErrorCode::ProtocolTypeUnsupported,
                "the local session lifecycle feature was not negotiated",
                false,
                message.message_id(),
                RawObject::empty(),
            );
            return RouteOutcome::Claimed;
        }
        // 独立限流：只统计客户端 → 服务端的 session.view。
        if let Err((retry_after, close_now)) = self
            .rate
            .check_at(handle.connection_id().as_str(), Instant::now())
        {
            self.rate_limited(handle, message.message_id(), retry_after, close_now);
            return RouteOutcome::Claimed;
        }
        let Ok(view) = serde_json::from_str::<SessionView>(message.body().get()) else {
            self.schema_invalid(handle, message);
            return RouteOutcome::Claimed;
        };
        let session = match view.session_id.as_ref() {
            Some(id) => match SessionId::new(id.as_str()) {
                Ok(session) => {
                    if !self.owned_session(&actor, &session).await {
                        self.send_frame_error(
                            handle,
                            ErrorCode::SessionNotFound,
                            "the session is not available on this host",
                            false,
                            message.message_id(),
                            RawObject::empty(),
                        );
                        return RouteOutcome::Claimed;
                    }
                    Some(session)
                }
                Err(_) => {
                    self.schema_invalid(handle, message);
                    return RouteOutcome::Claimed;
                }
            },
            None => None,
        };
        let Some(now) = self.now() else {
            self.send_frame_error(
                handle,
                ErrorCode::InternalUnavailable,
                "the service clock is unavailable",
                true,
                message.message_id(),
                RawObject::empty(),
            );
            return RouteOutcome::Claimed;
        };
        let Ok(connection) = ConnectionId::new(handle.connection_id().as_str()) else {
            return RouteOutcome::Claimed;
        };
        let decision = match self
            .lifecycle
            .declare_view(&actor, connection, session, view.view_version.get(), now)
            .await
        {
            Ok(decision) => decision,
            Err(error) => {
                debug!(
                    event = "sync.session_view_rejected",
                    connection_id = handle.connection_id().as_str(),
                    error = ?error,
                    "the view declaration cannot be recorded"
                );
                self.send_frame_error(
                    handle,
                    ErrorCode::InternalUnavailable,
                    "the view declaration cannot be recorded",
                    true,
                    message.message_id(),
                    RawObject::empty(),
                );
                return RouteOutcome::Claimed;
            }
        };
        let status = match decision.status {
            acp_core::ports::ViewStatus::Accepted => WireViewStatus::Accepted,
            acp_core::ports::ViewStatus::Stale => WireViewStatus::Stale,
        };
        let lease = match decision.lease_expires_at.as_ref() {
            Some(at) => match WireTimestamp::parse(at.as_str()) {
                Ok(at) => Nullable::value(at),
                Err(_) => Nullable::null(),
            },
            None => Nullable::null(),
        };
        // `stale` 必须没有租期；`accepted` 的租期构造不出来时按内部错误失败关闭，不伪造成功。
        if status == WireViewStatus::Accepted && lease.is_null() {
            self.send_frame_error(
                handle,
                ErrorCode::InternalUnavailable,
                "the view lease cannot be projected",
                true,
                message.message_id(),
                RawObject::empty(),
            );
            return RouteOutcome::Claimed;
        }
        let body = WireSessionViewResult {
            session_id: view.session_id,
            view_version: view.view_version,
            status,
            lease_expires_at: lease,
        };
        if handle.send(MessageType::SessionViewResult, &body).is_err() {
            debug!(
                event = "sync.session_view_result_undeliverable",
                connection_id = handle.connection_id().as_str(),
                "the view result cannot be queued"
            );
        }
        RouteOutcome::Claimed
    }
}

#[async_trait::async_trait]
impl MessageRoute for ViewRoute {
    async fn route(&self, handle: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        match message.message_type() {
            MessageType::SessionView => self.on_view(handle, message).await,
            // `session.view.result` 是服务端 → 客户端的方向：客户端发来时不认领，由连接面按
            // `protocol.type_unsupported` 拒绝（它是合法 v1 类型，只是方向不对）。
            _ => RouteOutcome::Unclaimed,
        }
    }

    fn on_authenticated(&self, handle: &ConnectionHandle, features: &[String]) {
        let negotiated = features
            .iter()
            .any(|feature| feature == LOCAL_LIFECYCLE_FEATURE);
        lock(&self.states)
            .entry(handle.connection_id().as_str().to_owned())
            .or_default()
            .negotiated = negotiated;
    }

    async fn on_closed(&self, handle: &ConnectionHandle) {
        self.rate.forget(handle.connection_id().as_str());
        lock(&self.states).remove(handle.connection_id().as_str());
        let Ok(connection) = ConnectionId::new(handle.connection_id().as_str()) else {
            return;
        };
        let actor = Self::actor(handle);
        let now = self.now().unwrap_or_else(|| {
            // 关闭路径上拿不到服务端时钟时也必须清理：用零时刻，租约只会因此更早失效。
            Timestamp::new("1970-01-01T00:00:00.000Z").expect("canonical epoch")
        });
        if let Err(error) = self.lifecycle.release_view(&actor, connection, now).await {
            debug!(
                event = "sync.session_view_release_failed",
                connection_id = handle.connection_id().as_str(),
                error = ?error,
                "the connection view cannot be released"
            );
        }
    }
}

/// owned 会话的 `localActivity` 批量投影（与 `session.list`/快照同一代目录）。
///
/// 未协商 feature 时返回空表：调用方据此让 `localActivity` 键缺席，而不是发 `null`。
pub async fn activity_map(
    lifecycle: &LocalSessionLifecycle,
    actor: &Actor,
    negotiated: bool,
) -> Result<HashMap<SessionId, LocalActivity>, acp_core::model::PortError> {
    if !negotiated {
        return Ok(HashMap::new());
    }
    Ok(lifecycle
        .activity_snapshot(actor)
        .await?
        .into_iter()
        .map(|view| {
            (
                view.summary.session_id().clone(),
                wire_activity(view.activity),
            )
        })
        .collect())
}

/// core 的归约值 → wire 词表（同一取值集合，两处类型各自冻结）。
fn wire_activity(activity: acp_core::model::LocalActivity) -> LocalActivity {
    match activity {
        acp_core::model::LocalActivity::Idle => LocalActivity::Idle,
        acp_core::model::LocalActivity::Active => LocalActivity::Active,
        acp_core::model::LocalActivity::Pending => LocalActivity::Pending,
    }
}

/// 单个会话的 `localActivity`（事件投递用；未协商或非 owned 时为 `None`）。
pub async fn activity_of(
    lifecycle: &LocalSessionLifecycle,
    actor: &Actor,
    session: &SessionId,
    owned: bool,
    negotiated: bool,
) -> Option<LocalActivity> {
    if !negotiated || !owned {
        return None;
    }
    lifecycle
        .current_activity(actor, session.clone())
        .await
        .ok()
        .map(wire_activity)
}

/// 执行一条已经持久受理的 `session.delete`。
///
/// 扫描器和 Sync 命令面共用这条顺序：在每会话 lifecycle gate 内先用完整删除写集
/// 做只读资格预检，再取当前资源代并真实释放；只有适配器确认 `Released` 才调用
/// 最终删除事务。最终事务仍会复核资格，以覆盖预检后的新事实。`Failed` 与
/// `Uncertain` 分别写成持久终态并保留会话和容量占位。
/// 调用方只能传入 `Prepared` 分支的受理，重复请求不得再次进入这里。
pub async fn execute_session_delete(
    core: &UseCases,
    lifecycle: &LocalSessionLifecycle,
    at: Timestamp,
    actor: &Actor,
    request: &RequestId,
    request_fingerprint: &Digest,
    session: &SessionId,
) -> Result<(), PortError> {
    execute_session_delete_inner(
        core,
        lifecycle,
        SessionDeleteContext {
            at,
            actor,
            request,
            request_fingerprint,
            session,
        },
        false,
        None,
    )
    .await
}

/// 执行扫描器候选；资源代或保留窗口在取得 gate 后发生变化时放弃本次删除。
pub async fn execute_retention_session_delete(
    core: &UseCases,
    lifecycle: &LocalSessionLifecycle,
    at: Timestamp,
    actor: &Actor,
    request: &RequestId,
    request_fingerprint: &Digest,
    candidate: &RetentionDeleteCandidate,
) -> Result<(), PortError> {
    execute_session_delete_inner(
        core,
        lifecycle,
        SessionDeleteContext {
            at,
            actor,
            request,
            request_fingerprint,
            session: &candidate.candidate.session,
        },
        candidate.candidate.delete_pending,
        Some(candidate),
    )
    .await
}

/// Accepted deletion command shared by direct and retention-triggered execution.
struct SessionDeleteContext<'a> {
    at: Timestamp,
    actor: &'a Actor,
    request: &'a RequestId,
    request_fingerprint: &'a Digest,
    session: &'a SessionId,
}

async fn execute_session_delete_inner(
    core: &UseCases,
    lifecycle: &LocalSessionLifecycle,
    command: SessionDeleteContext<'_>,
    require_pending: bool,
    retention_candidate: Option<&RetentionDeleteCandidate>,
) -> Result<(), PortError> {
    core.with_owned_deletion_submission_gate(command.session, || {
        execute_session_delete_submission_locked(
            core,
            lifecycle,
            command,
            require_pending,
            retention_candidate,
        )
    })
    .await
}

async fn execute_session_delete_submission_locked(
    core: &UseCases,
    lifecycle: &LocalSessionLifecycle,
    command: SessionDeleteContext<'_>,
    require_pending: bool,
    retention_candidate: Option<&RetentionDeleteCandidate>,
) -> Result<(), PortError> {
    let SessionDeleteContext {
        at,
        actor,
        request,
        request_fingerprint,
        session,
    } = command;
    let mut delete_barrier = lifecycle.enter_accepted_delete(session).await;
    let gate = lifecycle.acquire_owner_lifecycle_gate(session).await;
    if let Some(candidate) = retention_candidate {
        let current_snapshot = lifecycle.retention_snapshot_locked(&gate, session)?;
        let same_snapshot = current_snapshot.as_ref() == Some(&candidate.snapshot);
        let current_candidate = if same_snapshot {
            lifecycle
                .retention_candidate_locked(&gate, session, &at)
                .await?
        } else {
            None
        };
        let still_current = current_candidate.as_ref().is_some_and(|current| {
            current.session == candidate.candidate.session
                && current.delete_pending == candidate.candidate.delete_pending
                && current.last_protected_at == candidate.candidate.last_protected_at
        });
        if !same_snapshot || !still_current {
            let error = PublicError::coded(
                "session.busy",
                "the session changed before its retention deletion could be executed",
                true,
            )
            .map_err(PortError::from)?;
            lifecycle
                .fail_delete_release_in_gate(
                    core,
                    gate.clone_lifecycle_handle(),
                    session,
                    lifecycle
                        .resource_generation_locked(&gate, session)?
                        .unwrap_or(candidate.snapshot.generation),
                    None,
                    actor,
                    request,
                    error,
                )
                .await?;
            return Ok(());
        }
    }
    let pending_generation = lifecycle.delete_pending_generation_locked(&gate, session)?;
    if require_pending && pending_generation.is_none() {
        let error = PublicError::coded(
            "session.not_found",
            "the pending deletion was cancelled before it could be retried",
            false,
        )
        .map_err(PortError::from)?;
        core.settle_session_delete_in_held_submission_gate_locked(
            lifecycle,
            gate.clone_lifecycle_handle(),
            session,
            actor,
            request,
            CommandStatus::Failed,
            Some(error),
        )
        .await?;
        return Ok(());
    }
    let write = SessionDeletionWrite::new(
        session.clone(),
        actor.clone(),
        request.clone(),
        request_fingerprint.clone(),
        at.clone(),
    );
    match core.session_deletion_preflight(&write).await {
        Ok(SessionDeletionEligibility::Eligible) => {}
        Ok(SessionDeletionEligibility::Blocked(SessionDeletionBlocker::NotFound)) => {
            if let Some(generation) = pending_generation {
                lifecycle.complete_delete_pending_locked(&gate, session, generation)?;
            }
            let error =
                PublicError::coded("session.not_found", "the session does not exist", false)
                    .map_err(PortError::from)?;
            core.settle_session_delete_in_held_submission_gate_locked(
                lifecycle,
                gate.clone_lifecycle_handle(),
                session,
                actor,
                request,
                CommandStatus::Failed,
                Some(error),
            )
            .await?;
            return Ok(());
        }
        Ok(SessionDeletionEligibility::Blocked(
            SessionDeletionBlocker::NotEmpty | SessionDeletionBlocker::Protected,
        )) => {
            if let Some(generation) = pending_generation {
                lifecycle.cancel_delete_pending_locked(&gate, session, generation)?;
            }
            let error = PublicError::coded(
                "session.busy",
                "the session gained content or unresolved work before it could be released",
                true,
            )
            .map_err(PortError::from)?;
            core.settle_session_delete_in_held_submission_gate_locked(
                lifecycle,
                gate.clone_lifecycle_handle(),
                session,
                actor,
                request,
                CommandStatus::Failed,
                Some(error),
            )
            .await?;
            return Ok(());
        }
        Err(error) => {
            let public = PublicError::coded(
                "internal.unavailable",
                "the session deletion eligibility could not be checked before release",
                true,
            )
            .map_err(PortError::from)?;
            core.settle_session_delete_in_held_submission_gate_locked(
                lifecycle,
                gate.clone_lifecycle_handle(),
                session,
                actor,
                request,
                CommandStatus::Failed,
                Some(public),
            )
            .await?;
            return Err(error);
        }
    }

    let Some(generation) = pending_generation.or_else(|| lifecycle.resource_generation(session))
    else {
        let error = PublicError::coded(
            "internal.unavailable",
            "the session resource cannot be released",
            true,
        )
        .map_err(PortError::from)?;
        core.settle_session_delete_in_held_submission_gate_locked(
            lifecycle,
            gate.clone_lifecycle_handle(),
            session,
            actor,
            request,
            CommandStatus::Failed,
            Some(error),
        )
        .await?;
        return Ok(());
    };

    let release = lifecycle
        .release_session_for_delete_locked(&gate, actor, session.clone(), generation, at.clone())
        .await;
    if let Some(delete_barrier) = delete_barrier.take() {
        delete_barrier.release();
    }
    match release {
        Ok(ReleaseOutcome::Released | ReleaseOutcome::AlreadyReleased) => {
            let delete_epoch =
                lifecycle.delete_epoch_locked(&gate, session, generation, request)?;
            lifecycle.continue_deletion_after_release_locked(
                &gate,
                session,
                generation,
                ReleaseOutcome::Released,
                at.clone(),
            )?;
            let outcome = core
                .finish_delete_in_held_lifecycle_gate_locked(
                    lifecycle,
                    gate.clone_lifecycle_handle(),
                    write,
                )
                .await;
            match outcome {
                Ok(SessionDeletionOutcome::Deleted { .. }) => {
                    lifecycle.complete_delete_pending_locked(&gate, session, generation)?;
                    Ok(())
                }
                Ok(SessionDeletionOutcome::Replayed(record))
                    if record.status() == CommandStatus::Completed =>
                {
                    lifecycle.complete_delete_pending_locked(&gate, session, generation)?;
                    Ok(())
                }
                Ok(_) => {
                    lifecycle.cancel_delete_pending_locked(&gate, session, generation)?;
                    Ok(())
                }
                Err(error) => {
                    let public = PublicError::coded(
                        "internal.unavailable",
                        "the empty session could not be deleted",
                        true,
                    )
                    .map_err(PortError::from)?;
                    lifecycle
                        .finish_failed_delete_in_held_lifecycle_gate_locked(
                            core,
                            &gate,
                            session,
                            actor,
                            request,
                            generation,
                            delete_epoch,
                            public,
                        )
                        .await?;
                    Err(error)
                }
            }
        }
        Ok(ReleaseOutcome::Failed) => {
            let error = PublicError::coded(
                "internal.unavailable",
                "the session resource could not be released",
                true,
            )
            .map_err(PortError::from)?;
            lifecycle
                .fail_delete_release_in_gate(
                    core,
                    gate.clone_lifecycle_handle(),
                    session,
                    generation,
                    None,
                    actor,
                    request,
                    error,
                )
                .await?;
            Ok(())
        }
        Ok(ReleaseOutcome::Uncertain) | Err(_) => {
            let error = PublicError::coded(
                "command.uncertain",
                "the session resource release outcome cannot be confirmed",
                false,
            )
            .map_err(PortError::from)?;
            core.settle_delete_in_held_lifecycle_gate_locked(
                lifecycle,
                gate.clone_lifecycle_handle(),
                session,
                actor,
                request,
                CommandStatus::Uncertain,
                Some(error),
            )
            .await?;
            lifecycle.complete_delete_pending_locked(&gate, session, generation)?;
            Ok(())
        }
    }
}

/// 周期性消费保留期与容量候选（组合根 spawn，关闭信号到达即返回）。
///
/// 保留期到期的空会话先持久受理 `session.delete`，再走共用释放与删除顺序；同一条路径上的非空会话
/// 只释放 Agent 资源。容量候选永远只释放，不得因容量压力提前删除空会话。查看者、任务与交互保护判定
/// 仍在 core，本层不重算。
pub async fn reclaim_loop(
    core: Arc<UseCases>,
    lifecycle: Arc<LocalSessionLifecycle>,
    clock: Arc<dyn Clock>,
    actor: Actor,
    interval: Duration,
    shutdown: crate::transport::net::Shutdown,
) {
    loop {
        tokio::select! {
            biased;
            () = shutdown.clone().wait() => return,
            () = tokio::time::sleep(interval) => {}
        }
        let now = clock.now();
        let candidates = match lifecycle.retention_due(&actor, now.clone()).await {
            Ok(due) => due,
            Err(error) => {
                debug!(event = "sync.lifecycle_reclaim_failed", error = ?error, "the retention candidates cannot be read");
                continue;
            }
        };
        let due_sessions = candidates
            .iter()
            .map(|candidate| candidate.session.clone())
            .collect::<Vec<_>>();
        for candidate in candidates {
            let session_id = candidate.session.clone();
            let candidate = match retention_delete_candidate(lifecycle.as_ref(), candidate) {
                Ok(Some(candidate)) => candidate,
                Ok(None) => continue,
                Err(error) => {
                    debug!(
                        event = "sync.lifecycle_candidate_snapshot_failed",
                        session_id = session_id.as_str(),
                        error = ?error,
                        "the retention candidate snapshot cannot be captured"
                    );
                    continue;
                }
            };
            match core.session_has_content(&candidate.candidate.session).await {
                Ok(false) => {
                    let request = core.new_request_id();
                    let candidate_fingerprint = session_delete_fingerprint();
                    match core
                        .admit_session_delete(
                            &actor,
                            request.clone(),
                            candidate_fingerprint.clone(),
                            candidate.candidate.session.clone(),
                        )
                        .await
                    {
                        Ok(SessionDeleteAdmission::Prepared) => {
                            let result = execute_retention_session_delete(
                                core.as_ref(),
                                lifecycle.as_ref(),
                                clock.now(),
                                &actor,
                                &request,
                                &candidate_fingerprint,
                                &candidate,
                            )
                            .await;
                            if let Err(error) = result {
                                debug!(
                                    event = "sync.lifecycle_delete_failed",
                                    session_id = candidate.candidate.session.as_str(),
                                    error = ?error,
                                    "the lifecycle delete could not be completed"
                                );
                            }
                        }
                        Ok(SessionDeleteAdmission::InFlight) => debug!(
                            event = "sync.lifecycle_delete_in_flight",
                            session_id = candidate.candidate.session.as_str(),
                            "the lifecycle delete is already in flight"
                        ),
                        Ok(SessionDeleteAdmission::Settled(_)) => debug!(
                            event = "sync.lifecycle_delete_settled",
                            session_id = candidate.candidate.session.as_str(),
                            "the lifecycle delete was already settled"
                        ),
                        Err(error) => debug!(
                            event = "sync.lifecycle_delete_admission_failed",
                            session_id = candidate.candidate.session.as_str(),
                            error = ?error,
                            "the lifecycle delete could not be admitted"
                        ),
                    }
                }
                Ok(true) => {
                    match lifecycle
                        .release_retention_candidate(&actor, &candidate.candidate, clock.now())
                        .await
                    {
                        Ok(outcome) => debug!(
                            event = "sync.lifecycle_reclaimed",
                            session_id = candidate.candidate.session.as_str(),
                            outcome = ?outcome,
                            "a nonempty retention candidate was released"
                        ),
                        Err(error) => debug!(
                            event = "sync.lifecycle_release_failed",
                            session_id = candidate.candidate.session.as_str(),
                            error = ?error,
                            "a nonempty retention candidate cannot be released"
                        ),
                    }
                }
                Err(error) => debug!(
                    event = "sync.lifecycle_content_classification_failed",
                    session_id = candidate.candidate.session.as_str(),
                    error = ?error,
                    "the retention candidate content cannot be classified"
                ),
            }
        }
        if !lifecycle.has_capacity_pressure() {
            continue;
        }
        match lifecycle.reclaim_candidate(&actor, clock.now()).await {
            Ok(Some(candidate)) if !due_sessions.contains(&candidate.session) => {
                let session = candidate.session.clone();
                match lifecycle
                    .release_capacity_candidate(&actor, &candidate, clock.now())
                    .await
                {
                    Ok(outcome) => debug!(
                        event = "sync.lifecycle_capacity_reclaimed",
                        session_id = session.as_str(),
                        outcome = ?outcome,
                        "a capacity candidate was released"
                    ),
                    Err(error) => debug!(
                        event = "sync.lifecycle_capacity_release_failed",
                        session_id = session.as_str(),
                        error = ?error,
                        "a capacity candidate cannot be released"
                    ),
                }
            }
            Ok(_) => {}
            Err(error) => debug!(
                event = "sync.lifecycle_capacity_candidate_failed",
                error = ?error,
                "the capacity reclaim candidate cannot be read"
            ),
        }
    }
}

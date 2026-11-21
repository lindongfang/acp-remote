//! Short protocol state transitions. No SQL, CPU-heavy raw parsing or application callback.
use super::{
    AcpError, ConfigSourceStamp, ConnectionScope, Incoming, OutboundKind, OwnedRawValue,
    PermissionRequest, PermissionSelection, RawMessage, Received, Request, RequestKind,
    Reservation, ResponseDeadline, RpcId, SessionId, Strategy, UnsupportedRequest, UpdateOrigin,
    WriteFailure, WriteFence, WriteIdentity, WriteProgress, WriteReport, ZeroReason, raw,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex, MutexGuard, Weak},
};
use tokio::{
    sync::{Mutex as AsyncMutex, Notify},
    task::JoinHandle,
    time::Instant,
};
pub(super) const MAX_PENDING: usize = 64;
pub(super) struct Core {
    pub scope: ConnectionScope,
    pub state: Mutex<State>,
    pub notify: Arc<Notify>,
    pub events_notify: Notify,
    pub io_tasks: Mutex<Vec<Arc<TaskSlot>>>,
    pub collector: Mutex<Option<Arc<TaskSlot>>>,
    pub shutdown: AsyncMutex<()>,
    pub installation_complete: std::sync::atomic::AtomicBool,
    pub installation_notify: Notify,
    #[cfg(test)]
    pub hooks: Option<Arc<super::AcpObservationHooks>>,
    #[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
    pub capacity_close_fault:
        Mutex<Option<Arc<super::capacity_close_write_fault::CloseWriteFault>>>,
    #[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
    pub capacity_resume_fault:
        Mutex<Option<Arc<super::capacity_close_write_fault::ResumeEnqueueFault>>>,
}
pub(super) struct TaskSlot {
    pub handle: AsyncMutex<Option<JoinHandle<()>>>,
}
impl Core {
    pub fn lock(&self) -> MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(v) => v,
            Err(p) => {
                let mut v = p.into_inner();
                v.stop(Some(AcpError::Io));
                v
            }
        }
    }
    pub fn stop(&self, error: Option<AcpError>) {
        self.lock().stop(error);
        self.notify.notify_waiters();
    }
    pub fn cancel(&self, cell: &Arc<WriteCell>, reason: ZeroReason) {
        let mut s = self.lock();
        s.expire(Instant::now());
        let progress = cell.lock().progress;
        if matches!(progress, WriteProgress::Finished(_)) {
            return;
        }
        if accepted_count(progress) == 0 && !cell.lock().in_poll {
            s.finish_zero(cell, reason);
        } else {
            let accepted = accepted_count(progress);
            s.spent(cell);
            s.finish(
                cell,
                WriteReport::Unknown {
                    accepted,
                    cause: WriteFailure::Cancelled,
                },
            );
            s.stop(Some(AcpError::Cancelled));
        }
        drop(s);
        self.notify.notify_waiters();
    }
}
pub(super) struct CellState {
    pub progress: WriteProgress,
    pub cancel: Option<ZeroReason>,
    pub in_poll: bool,
}
pub(super) struct WriteCell {
    pub key: u64,
    pub identity: WriteIdentity,
    pub state: Mutex<CellState>,
    pub notify: Notify,
}
impl WriteCell {
    pub fn lock(&self) -> MutexGuard<'_, CellState> {
        match self.state.lock() {
            Ok(v) => v,
            Err(p) => p.into_inner(),
        }
    }
}
pub(super) enum Payload {
    Request {
        method: &'static str,
        params: OwnedRawValue,
    },
    Raw(OwnedRawValue),
}
pub(super) struct Command {
    pub cell: Arc<WriteCell>,
    pub bytes: Vec<u8>,
    pub fence: Arc<dyn WriteFence>,
    pub deadline: Instant,
    pub automatic_close: bool,
    #[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
    pub capacity_close_fault: Option<Arc<super::capacity_close_write_fault::CloseWriteFault>>,
}
pub(super) struct Pending {
    pub cell: Arc<WriteCell>,
    pub kind: RequestKind,
    pub deadline: ResponseDeadline,
    pub snapshot: bool,
    pub replay_complete: bool,
    pub accepted: bool,
}
#[derive(Clone, Copy)]
pub(super) enum ServerStatus {
    Waiting,
    Reserved(u64),
}
pub(super) struct ServerPending {
    pub digest: [u8; 32],
    pub status: ServerStatus,
}
#[derive(Clone)]
pub(super) struct ReceiptCut {
    pub order: u64,
    pub at: Instant,
    pub eligible: Vec<(RpcId, u64)>,
    pub windows: Vec<(SessionId, u64, UpdateOrigin)>,
    pub remaining: usize,
}
#[allow(
    clippy::struct_excessive_bools,
    reason = "Independent startup, one-shot initialize and terminal publication flags accompany the stop phase"
)]
pub(super) struct State {
    pub wake: Arc<Notify>,
    pub started: bool,
    pub observed: bool,
    pub acknowledged: u64,
    pub initialize_attempted: bool,
    pub stopped: bool,
    pub terminal: bool,
    pub error: Option<AcpError>,
    pub strategy: Option<Strategy>,
    pub pending: HashMap<RpcId, Pending>,
    pub server: HashMap<RpcId, ServerPending>,
    pub seen: HashSet<RpcId>,
    pub cells: HashMap<u64, Weak<WriteCell>>,
    pub queue: VecDeque<Command>,
    pub events: VecDeque<Received>,
    pub rejected: VecDeque<RawMessage>,
    pub held: Option<ReceiptCut>,
    pub expiry_waiting: HashSet<u64>,
    pub next_id: i64,
    next_child_id: u64,
    pub next_cell: u64,
    pub receipt: u64,
    pub sequence: u64,
}
impl State {
    pub fn new(wake: Arc<Notify>) -> Self {
        Self {
            wake,
            started: false,
            observed: false,
            acknowledged: 0,
            initialize_attempted: false,
            stopped: false,
            terminal: false,
            error: None,
            strategy: None,
            pending: HashMap::new(),
            server: HashMap::new(),
            seen: HashSet::new(),
            cells: HashMap::new(),
            queue: VecDeque::new(),
            events: VecDeque::new(),
            rejected: VecDeque::new(),
            held: None,
            expiry_waiting: HashSet::new(),
            next_id: 0,
            next_child_id: 0,
            next_cell: 0,
            receipt: 0,
            sequence: 0,
        }
    }
    fn cell(
        &mut self,
        core: &Arc<Core>,
        id: Option<RpcId>,
        kind: OutboundKind,
        stamp: Option<ConfigSourceStamp>,
    ) -> Result<Arc<WriteCell>, AcpError> {
        self.next_cell = self.next_cell.checked_add(1).ok_or(AcpError::Capacity)?;
        let cell = Arc::new(WriteCell {
            key: self.next_cell,
            identity: WriteIdentity {
                scope: core.scope.clone(),
                id,
                kind,
                stamp,
            },
            state: Mutex::new(CellState {
                progress: WriteProgress::Reserved,
                cancel: None,
                in_poll: false,
            }),
            notify: Notify::new(),
        });
        self.cells.retain(|_, w| w.strong_count() != 0);
        self.cells.insert(cell.key, Arc::downgrade(&cell));
        Ok(cell)
    }
    pub fn reserve_request(
        &mut self,
        core: &Arc<Core>,
        request: Request,
        deadline: ResponseDeadline,
    ) -> Result<Reservation, AcpError> {
        if matches!(deadline,ResponseDeadline::At(at) if at<=Instant::now()) {
            return Err(AcpError::Deadline);
        }
        if self.pending.len() >= MAX_PENDING {
            return Err(AcpError::Capacity);
        }
        if matches!(request.kind, RequestKind::ChildHistory)
            && self.strategy.as_ref().is_none_or(|s| {
                !s.child_history_enabled
                    .load(std::sync::atomic::Ordering::Acquire)
            })
        {
            return Err(AcpError::UnsupportedCapability);
        }
        if let RequestKind::Resume(ref session) | RequestKind::Replay(ref session) = request.kind
            && self.pending.values().any(
                |p| matches!(&p.kind,RequestKind::Resume(s)|RequestKind::Replay(s) if s==session),
            )
        {
            return Err(AcpError::SessionReadBusy);
        }
        let id = if matches!(request.kind, RequestKind::ChildHistory) {
            self.next_child_id = self
                .next_child_id
                .checked_add(1)
                .ok_or(AcpError::Capacity)?;
            RpcId::String(format!("child-history/{}", self.next_child_id).into())
        } else {
            self.next_id = self
                .next_id
                .checked_add(1)
                .filter(|n| *n <= 9_007_199_254_740_991)
                .ok_or(AcpError::Capacity)?;
            RpcId::Integer(self.next_id)
        };
        let cell = self.cell(
            core,
            Some(id.clone()),
            OutboundKind::Request(request.kind.clone()),
            request.stamp,
        )?;
        self.pending.insert(
            id,
            Pending {
                cell: cell.clone(),
                kind: request.kind,
                deadline,
                snapshot: false,
                replay_complete: false,
                accepted: false,
            },
        );
        Ok(Reservation {
            core: core.clone(),
            cell,
            payload: Payload::Request {
                method: request.method,
                params: request.params,
            },
            armed: true,
        })
    }
    pub fn note_server_request(&mut self, id: RpcId, digest: [u8; 32]) -> Result<(), AcpError> {
        if self.seen.contains(&id) {
            return Err(AcpError::InvalidMessage);
        }
        self.seen.insert(id.clone());
        if !self.stopped && self.server.len() >= MAX_PENDING {
            return Err(AcpError::Capacity);
        }
        if !self.stopped {
            self.server.insert(
                id,
                ServerPending {
                    digest,
                    status: ServerStatus::Waiting,
                },
            );
        }
        Ok(())
    }
    pub fn reserve_control(
        &mut self,
        core: &Arc<Core>,
        kind: OutboundKind,
        payload: OwnedRawValue,
    ) -> Result<Reservation, AcpError> {
        self.expire(Instant::now());
        if self.stopped {
            return Err(AcpError::Closed);
        }
        if self.strategy.is_none() {
            return Err(AcpError::NotInitialized);
        }
        let id = match &kind {
            OutboundKind::PermissionReply { id, .. } | OutboundKind::UnsupportedReply(id) => {
                Some(id.clone())
            }
            _ => None,
        };
        let cell = self.cell(core, id, kind, None)?;
        Ok(Reservation {
            core: core.clone(),
            cell,
            payload: Payload::Raw(payload),
            armed: true,
        })
    }
    pub fn finish_zero(&mut self, cell: &Arc<WriteCell>, reason: ZeroReason) {
        if matches!(cell.lock().progress, WriteProgress::Finished(_)) {
            return;
        }
        if let Some(id) = cell.identity.id() {
            match cell.identity.kind() {
                OutboundKind::Request(_) => {
                    if self.pending.get(id).is_some_and(|p| p.cell.key == cell.key) {
                        self.pending.remove(id);
                    }
                }
                OutboundKind::PermissionReply { .. } | OutboundKind::UnsupportedReply(_) => {
                    if !self.stopped
                        && let Some(p) = self.server.get_mut(id)
                        && matches!(p.status,ServerStatus::Reserved(k) if k==cell.key)
                    {
                        p.status = ServerStatus::Waiting;
                    }
                }
                OutboundKind::Cancel(_) => {}
            }
        }
        self.finish(cell, WriteReport::Zero { reason });
    }
    pub fn finish(&mut self, cell: &Arc<WriteCell>, report: WriteReport) {
        let mut c = cell.lock();
        if !matches!(c.progress, WriteProgress::Finished(_)) {
            c.progress = WriteProgress::Finished(report);
        }
        drop(c);
        self.cells.remove(&cell.key);
        self.expiry_waiting.remove(&cell.key);
        cell.notify.notify_waiters();
        self.wake.notify_waiters();
    }
    pub fn spent(&mut self, cell: &WriteCell) {
        if matches!(
            cell.identity.kind(),
            OutboundKind::PermissionReply { .. } | OutboundKind::UnsupportedReply(_)
        ) && let Some(id) = cell.identity.id()
            && self
                .server
                .get(id)
                .is_some_and(|p| matches!(p.status,ServerStatus::Reserved(k) if k==cell.key))
        {
            self.server.remove(id);
        }
    }
    pub fn accepted(&mut self, cell: &WriteCell) {
        if let Some(id) = cell.identity.id()
            && let Some(p) = self.pending.get_mut(id)
            && p.cell.key == cell.key
        {
            p.accepted = true;
        }
    }
    pub fn stop(&mut self, error: Option<AcpError>) {
        self.wake.notify_waiters();
        if let Some(e) = error {
            self.error.get_or_insert(e);
        }
        if self.stopped {
            return;
        }
        self.stopped = true;
        let cells = self
            .cells
            .values()
            .filter_map(Weak::upgrade)
            .collect::<Vec<_>>();
        for cell in cells {
            let c = cell.lock();
            let progress = c.progress;
            let in_poll = c.in_poll;
            drop(c);
            match progress {
                WriteProgress::Finished(_) => {}
                WriteProgress::Reserved
                | WriteProgress::Queued
                | WriteProgress::Writing { accepted: 0, .. }
                    if !in_poll =>
                {
                    self.finish_zero(&cell, ZeroReason::ConnectionClosed);
                }
                _ => {
                    let accepted = accepted_count(progress);
                    self.spent(&cell);
                    self.finish(
                        &cell,
                        WriteReport::Unknown {
                            accepted,
                            cause: WriteFailure::ConnectionLost,
                        },
                    );
                }
            }
        }
        self.queue.clear();
    }
    pub fn expire(&mut self, now: Instant) {
        let due = self
            .pending
            .iter()
            .filter_map(|(id, p)| match p.deadline {
                ResponseDeadline::At(at) if at <= now => Some((id.clone(), p.cell.clone(), at)),
                _ => None,
            })
            .collect::<Vec<_>>();
        for (id, cell, at) in due {
            let held = self.held.as_ref().is_some_and(|h| {
                h.at < at
                    && h.remaining > 0
                    && h.eligible.iter().any(|(i, k)| i == &id && *k == cell.key)
            });
            if held {
                self.expiry_waiting.insert(cell.key);
                continue;
            }
            let progress = cell.lock().progress;
            match progress {
                WriteProgress::Reserved
                | WriteProgress::Queued
                | WriteProgress::Writing { accepted: 0, .. } => {
                    self.finish_zero(&cell, ZeroReason::Deadline);
                }
                WriteProgress::Finished(WriteReport::Zero { .. }) => {}
                WriteProgress::Finished(WriteReport::Written)
                    if matches!(
                        cell.identity.kind(),
                        OutboundKind::Request(RequestKind::ChildHistory)
                    ) =>
                {
                    self.pending.remove(&id);
                }
                WriteProgress::Finished(WriteReport::Written | WriteReport::Unknown { .. }) => {
                    self.stop(Some(AcpError::Deadline));
                }
                _ => {
                    self.finish(
                        &cell,
                        WriteReport::Unknown {
                            accepted: accepted_count(progress),
                            cause: WriteFailure::Deadline,
                        },
                    );
                    self.stop(Some(AcpError::Deadline));
                }
            }
        }
    }
    pub fn issued_child_id(&self, id: &RpcId) -> bool {
        let RpcId::String(value) = id else {
            return false;
        };
        value.strip_prefix("child-history/").is_some_and(|serial| {
            !serial.starts_with('0')
                && serial.bytes().all(|b| b.is_ascii_digit())
                && serial
                    .parse::<u64>()
                    .is_ok_and(|n| n > 0 && n <= self.next_child_id)
        })
    }
    pub fn nearest_deadline(&self) -> Option<Instant> {
        self.pending
            .values()
            .filter(|p| !self.expiry_waiting.contains(&p.cell.key))
            .filter_map(|p| {
                if let ResponseDeadline::At(at) = p.deadline {
                    Some(at)
                } else {
                    None
                }
            })
            .min()
    }
    pub fn capture(&mut self, at: Instant, frames: usize) -> Result<ReceiptCut, AcpError> {
        self.receipt = self.receipt.checked_add(1).ok_or(AcpError::Capacity)?;
        let eligible = self
            .pending
            .iter()
            .filter(|(_, p)| p.accepted)
            .map(|(id, p)| (id.clone(), p.cell.key))
            .collect();
        let windows = self
            .pending
            .values()
            .filter(|p| p.accepted)
            .filter_map(|p| match &p.kind {
                RequestKind::Resume(s) => Some((s.clone(), p.cell.key, UpdateOrigin::DuringResume)),
                RequestKind::Replay(s) => Some((s.clone(), p.cell.key, UpdateOrigin::DuringReplay)),
                _ => None,
            })
            .collect();
        let cut = ReceiptCut {
            order: self.receipt,
            at,
            eligible,
            windows,
            remaining: frames,
        };
        if frames != 0 {
            self.held = Some(cut.clone());
        }
        Ok(cut)
    }
    pub fn frame_done(&mut self) {
        if let Some(h) = &mut self.held {
            h.remaining = h.remaining.saturating_sub(1);
            if h.remaining == 0 {
                self.held = None;
                self.expiry_waiting.clear();
            }
        }
        self.expire(Instant::now());
    }
    pub fn emit(
        &mut self,
        scope: ConnectionScope,
        cut: &ReceiptCut,
        incoming: Incoming,
    ) -> Result<(), AcpError> {
        self.sequence = self.sequence.checked_add(1).ok_or(AcpError::Capacity)?;
        self.events.push_back(Received {
            scope,
            sequence: self.sequence,
            receipt_order: cut.order,
            incoming,
            update_freshness: None,
            view_context: super::ViewReadContextKind::OutsideReadWindow,
            view_window: None,
        });
        Ok(())
    }
}
pub(super) const fn accepted_count(progress: WriteProgress) -> usize {
    match progress {
        WriteProgress::Writing { accepted, .. }
        | WriteProgress::Finished(WriteReport::Unknown { accepted, .. }) => accepted,
        WriteProgress::Flushing { total } => total,
        _ => 0,
    }
}
pub(super) const fn report_error(report: WriteReport) -> AcpError {
    match report {
        WriteReport::Zero {
            reason: ZeroReason::Deadline,
        }
        | WriteReport::Unknown {
            cause: WriteFailure::Deadline,
            ..
        } => AcpError::Deadline,
        WriteReport::Zero {
            reason: ZeroReason::Cancelled | ZeroReason::Abandoned,
        } => AcpError::Cancelled,
        _ => AcpError::Closed,
    }
}
#[allow(
    clippy::needless_pass_by_value,
    reason = "Consumes the public owned permission selection"
)]
pub(super) fn reserve_permission(
    core: &Arc<Core>,
    request: &PermissionRequest,
    choice: PermissionSelection,
) -> Result<Reservation, AcpError> {
    if request.scope() != Some(core.scope.clone()) {
        return Err(AcpError::InvalidRequestScope);
    }
    let strategy = core
        .lock()
        .strategy
        .clone()
        .ok_or(AcpError::NotInitialized)?;
    let fresh = strategy.permission(
        request.id.clone(),
        request.raw.clone(),
        request
            .raw
            .field("params")?
            .ok_or(AcpError::InvalidMessage)?,
    )?;
    let outcome = match choice {
        PermissionSelection::Cancelled => raw::literal(r#"{"outcome":"cancelled"}"#)?,
        PermissionSelection::Selected(id) => {
            if fresh.options.iter().filter(|o| o.id == id).count() != 1
                || fresh
                    .options
                    .iter()
                    .enumerate()
                    .any(|(i, o)| fresh.options[..i].iter().any(|p| p.id == o.id))
            {
                return Err(AcpError::InvalidOption);
            }
            raw::object(
                [
                    ("outcome", &raw::string("selected")?),
                    ("optionId", &raw::string(id.as_str())?),
                ],
                None,
            )?
        }
    };
    let result = raw::object([("outcome", &outcome)], None)?;
    reserve_reply(
        core,
        &request.id,
        &request.raw,
        OutboundKind::PermissionReply {
            session: fresh.session,
            id: request.id.clone(),
        },
        "result",
        &result,
    )
}
pub(super) fn reserve_unsupported(
    core: &Arc<Core>,
    request: &UnsupportedRequest,
) -> Result<Reservation, AcpError> {
    if request.scope() != core.scope {
        return Err(AcpError::InvalidRequestScope);
    }
    reserve_reply(
        core,
        &request.id,
        &request.raw,
        OutboundKind::UnsupportedReply(request.id.clone()),
        "error",
        &raw::literal(r#"{"code":-32601,"message":"Method not found"}"#)?,
    )
}

pub(super) fn inspect_permission_request(
    core: &Arc<Core>,
    request: &PermissionRequest,
) -> Result<super::PermissionPendingSnapshot, AcpError> {
    if request.scope().as_ref() != Some(&core.scope) {
        return Err(AcpError::InvalidRequestScope);
    }
    let digest: [u8; 32] = Sha256::digest(request.raw().bytes()).into();
    let state = core.lock();
    if state.stopped {
        return Ok(super::PermissionPendingSnapshot::SpentOrInvalid);
    }
    if state.strategy.is_none() {
        return Err(AcpError::NotInitialized);
    }
    let Some(original) = state.server.get(request.id()) else {
        return Ok(super::PermissionPendingSnapshot::SpentOrInvalid);
    };
    if original.digest != digest {
        return Err(AcpError::InvalidRequestScope);
    }
    Ok(match original.status {
        ServerStatus::Waiting => super::PermissionPendingSnapshot::Waiting,
        ServerStatus::Reserved(_) => super::PermissionPendingSnapshot::Reserved,
    })
}
fn reserve_reply(
    core: &Arc<Core>,
    id: &RpcId,
    message: &RawMessage,
    kind: OutboundKind,
    key: &str,
    value: &OwnedRawValue,
) -> Result<Reservation, AcpError> {
    let digest: [u8; 32] = Sha256::digest(message.bytes()).into();
    let mut encoded = Vec::new();
    id.write_json(&mut encoded)?;
    let id_raw = super::super::raw_json::RawDocument::parse(encoded)?.root_owned();
    let payload = raw::object(
        [
            ("jsonrpc", &raw::string("2.0")?),
            ("id", &id_raw),
            (key, value),
        ],
        None,
    )?;
    let mut state = core.lock();
    state.expire(Instant::now());
    if state.stopped {
        return Err(AcpError::Closed);
    }
    let p = state.server.get(id).ok_or(AcpError::UnmatchedResponse)?;
    if p.digest != digest || !matches!(p.status, ServerStatus::Waiting) {
        return Err(AcpError::InvalidRequestScope);
    }
    let reservation = state.reserve_control(core, kind, payload)?;
    if let Some(p) = state.server.get_mut(id) {
        p.status = ServerStatus::Reserved(reservation.cell.key);
    }
    Ok(reservation)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closing_drain_records_seen_without_minting_server_grants() {
        // Pure bookkeeping regression; actual reader/receipt/drain is independently exercised.
        let mut state = State::new(Arc::new(Notify::new()));
        state.stop(None);
        let id = RpcId::String("new-closed-id".into());
        assert!(state.note_server_request(id.clone(), [1; 32]).is_ok());
        assert!(state.seen.contains(&id));
        assert!(state.server.is_empty());
        assert_eq!(
            state.note_server_request(id, [1; 32]),
            Err(AcpError::InvalidMessage)
        );
        assert!(state.server.is_empty());
        assert!(
            state
                .note_server_request(RpcId::String("later-id".into()), [2; 32])
                .is_ok()
        );
        assert_eq!(state.seen.len(), 2);
        assert!(state.server.is_empty());
    }
}

//! The only byte reader. Read receipt is captured at the real successful poll boundary.
#![allow(
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    super::model::{field, object, text},
    AcpError, Arc, AsyncRead, Incoming, Instant, RawMessage, RequestKind, Response,
    ResponseDeadline, RpcId, SessionEvent, SessionId, Strategy, UnsupportedRequest,
    UpdateFreshness, UpdateOrigin, Version, raw,
    state::{Core, ReceiptCut},
};
use sha2::{Digest, Sha256};
use std::{
    future::{pending, poll_fn},
    pin::Pin,
    task::Poll,
};
use tokio::io::ReadBuf;

#[allow(
    clippy::too_many_lines,
    clippy::naive_bytecount,
    reason = "One receipt/framing transition; LF scan is only the bounded scratch and adds no dependency"
)]
pub(super) async fn run<R: AsyncRead + Unpin>(core: Arc<Core>, mut reader: R) {
    let mut input = Vec::new();
    let mut scratch = [0u8; 8192];
    loop {
        let notified = core.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let deadline = {
            let mut s = core.lock();
            s.expire(Instant::now());
            if s.stopped {
                break;
            }
            s.nearest_deadline()
        };
        let timer = async {
            if let Some(at) = deadline {
                tokio::time::sleep_until(at).await;
            } else {
                pending::<()>().await;
            }
        };
        tokio::pin!(timer);
        let read = poll_fn(|cx| {
            let mut s = core.lock();
            s.expire(Instant::now());
            if s.stopped {
                return Poll::Ready(Ok(None));
            }
            let mut buf = ReadBuf::new(&mut scratch);
            match Pin::new(&mut reader).poll_read(cx, &mut buf) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Err(_)) => {
                    s.stop(Some(AcpError::Io));
                    Poll::Ready(Err(AcpError::Io))
                }
                Poll::Ready(Ok(())) => {
                    let n = buf.filled().len();
                    if n == 0 {
                        let error = if input.is_empty() {
                            AcpError::DeliveryUnknown
                        } else {
                            AcpError::InvalidMessage
                        };
                        s.stop(Some(error));
                        return Poll::Ready(Err(error));
                    }
                    let frames = buf.filled().iter().filter(|b| **b == b'\n').count();
                    let cut = s.capture(Instant::now(), frames);
                    s.expire(Instant::now());
                    Poll::Ready(cut.map(|c| Some((n, c))))
                }
            }
        });
        tokio::pin!(read);
        let batch = tokio::select! {biased; ()=&mut notified=>continue,()=&mut timer=>{core.notify.notify_waiters();continue;},result=&mut read=>result};
        let (n, cut) = match batch {
            Ok(Some(v)) => v,
            Ok(None) => break,
            Err(e) => {
                core.stop(Some(e));
                break;
            }
        };
        #[cfg(test)]
        super::observed::test_pause(
            &core,
            super::ObservationHookPoint::AfterReadCapturedBeforeRoute,
        )
        .await;
        if input.try_reserve(n).is_err() {
            core.stop(Some(AcpError::Allocation));
            break;
        }
        let scan_start = input.len();
        input.extend_from_slice(&scratch[..n]);
        let mut start = 0;
        for end in input[scan_start..]
            .iter()
            .enumerate()
            .filter_map(|(i, b)| (*b == b'\n').then_some(scan_start + i))
        {
            let mut frame = &input[start..end];
            if frame.last() == Some(&b'\r') {
                frame = &frame[..frame.len() - 1];
            }
            match RawMessage::parse(frame.to_vec()) {
                Ok(raw) => {
                    if let Err(error) = route(&core, &cut, raw.clone()) {
                        let mut s = core.lock();
                        s.rejected.push_back(raw);
                        s.stop(Some(error));
                    }
                }
                Err(error) => core.stop(Some(error)),
            }
            core.lock().frame_done();
            core.notify.notify_waiters();
            core.events_notify.notify_waiters();
            start = end + 1;
        }
        input.drain(..start);
        if core.lock().stopped {
            break;
        }
    }
}
fn route(core: &Arc<Core>, cut: &ReceiptCut, raw: RawMessage) -> Result<(), AcpError> {
    let fields = raw.fields()?;
    if fields
        .get("jsonrpc")?
        .map(super::super::raw_json::RawValueRef::string)
        .transpose()?
        .as_deref()
        != Some("2.0")
    {
        return Err(AcpError::InvalidMessage);
    }
    if let Some(method) = fields.get("method")? {
        if fields.get("result")?.is_some() || fields.get("error")?.is_some() {
            return Err(AcpError::InvalidMessage);
        }
        let method = text(method)?;
        let absent = raw::literal("null")?;
        let params = fields.get("params")?.unwrap_or(absent.as_ref());
        if !matches!(
            params.kind(),
            super::super::raw_json::RawKind::Null
                | super::super::raw_json::RawKind::Object
                | super::super::raw_json::RawKind::Array
        ) {
            return Err(AcpError::InvalidMessage);
        }
        let strategy = core
            .lock()
            .strategy
            .clone()
            .ok_or(AcpError::NotInitialized)?;
        if let Some(id) = fields.get("id")? {
            let id = RpcId::parse(id)?;
            let digest: [u8; 32] = Sha256::digest(raw.bytes()).into();
            let incoming = if &*method == "session/request_permission" {
                let mut request = strategy.permission(id.clone(), raw.clone(), params)?;
                request.connection = Some(core.scope.clone());
                Incoming::Permission(request)
            } else {
                Incoming::UnsupportedRequest(UnsupportedRequest {
                    scope: core.scope.clone(),
                    id: id.clone(),
                    method,
                    raw: raw.clone(),
                })
            };
            let mut s = core.lock();
            s.note_server_request(id, digest)?;
            s.emit(core.scope.clone(), cut, incoming)?;
            return Ok(());
        }
        if &*method == "session/update" {
            let event = strategy.update(params)?;
            let session = event.session();
            let mut s = core.lock();
            let (view_context, view_window) = receipt_context(&s, cut, session, &core.scope);
            let freshness = update_freshness(
                &mut s,
                cut,
                &event,
                strategy.version(),
                view_context,
                view_window.as_ref(),
            );
            let window = cut.windows.iter().find(|(target, key, _)| {
                target == session && s.pending.values().any(|p| p.cell.key == *key && p.accepted)
            });
            let (origin, key) = window.map_or((UpdateOrigin::Live, None), |(_, key, origin)| {
                (*origin, Some(*key))
            });
            if matches!(event, SessionEvent::State { .. })
                && freshness == UpdateFreshness::Current
                && let Some(key) = key
            {
                for p in s.pending.values_mut() {
                    if p.cell.key == key {
                        p.snapshot = true;
                    }
                }
            }
            s.emit(
                core.scope.clone(),
                cut,
                Incoming::Update { event, origin, raw },
            )?;
            if let Some(received) = s.events.back_mut() {
                received.view_context = view_context;
                received.view_window = view_window;
                received.update_freshness = Some(freshness);
            }
            return Ok(());
        }
        return core.lock().emit(
            core.scope.clone(),
            cut,
            Incoming::UnknownNotification { raw },
        );
    }
    response(core, cut, raw)
}
fn update_freshness(
    state: &mut super::state::State,
    cut: &ReceiptCut,
    event: &SessionEvent,
    wire: Version,
    context: super::ViewReadContextKind,
    window: Option<&super::ReceivedReadWindow>,
) -> UpdateFreshness {
    let session = event.session();
    match context {
        super::ViewReadContextKind::AmbiguousReceiptWindow => UpdateFreshness::Unconfirmed,
        super::ViewReadContextKind::OutsideReadWindow => {
            // A replacement accepted after this receipt cannot acquire the old frame.
            // With no replacement, frames after the original response are live again.
            if state.pending.values().any(|pending| {
                pending.accepted
                    && matches!(&pending.kind, RequestKind::Resume(s) | RequestKind::Replay(s) if s == session)
            })
            {
                UpdateFreshness::Unconfirmed
            } else {
                UpdateFreshness::Current
            }
        }
        super::ViewReadContextKind::UniqueReceiptWindow => {
            let Some(window) = window else {
                return UpdateFreshness::Unconfirmed;
            };
            let Some(pending) = state.pending.get_mut(window.rpc()) else {
                return UpdateFreshness::Unconfirmed;
            };
            if pending.cell.key != window.cell_key()
                || !pending.accepted
                || !cut
                    .eligible
                    .iter()
                    .any(|(rpc, key)| rpc == window.rpc() && *key == pending.cell.key)
            {
                return UpdateFreshness::Unconfirmed;
            }
            match &pending.kind {
                RequestKind::Resume(target) if target == session => {
                    if wire == Version::V2 {
                        UpdateFreshness::Current
                    } else {
                        UpdateFreshness::History
                    }
                }
                RequestKind::Replay(target) if target == session => {
                    if wire != Version::V2 {
                        return UpdateFreshness::History;
                    }
                    if matches!(event, SessionEvent::Other { kind, .. } if kind.as_ref() == "replay_complete")
                    {
                        pending.replay_complete = true;
                        UpdateFreshness::History
                    } else if pending.replay_complete {
                        UpdateFreshness::Current
                    } else {
                        UpdateFreshness::History
                    }
                }
                _ => UpdateFreshness::Unconfirmed,
            }
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CellSelection {
    Outside,
    Unique(u64),
    Ambiguous,
}
fn select_unique_cell(keys: impl Iterator<Item = u64>) -> CellSelection {
    keys.fold(CellSelection::Outside, |selection, key| match selection {
        CellSelection::Outside => CellSelection::Unique(key),
        CellSelection::Unique(previous) if previous == key => selection,
        CellSelection::Unique(_) | CellSelection::Ambiguous => CellSelection::Ambiguous,
    })
}

fn response(core: &Arc<Core>, cut: &ReceiptCut, raw: RawMessage) -> Result<(), AcpError> {
    let fields = raw.fields()?;
    let id = RpcId::parse(fields.get("id")?.ok_or(AcpError::InvalidMessage)?)?;
    let (key, kind, snapshot, strategy, observed) = {
        let s = core.lock();
        let p = s.pending.get(&id).ok_or(AcpError::UnmatchedResponse)?;
        if !p.accepted
            || !cut
                .eligible
                .iter()
                .any(|(i, k)| i == &id && *k == p.cell.key)
        {
            return Err(AcpError::UnmatchedResponse);
        }
        if matches!(p.deadline,ResponseDeadline::At(at) if cut.at>=at) {
            return Err(AcpError::Deadline);
        }
        (
            p.cell.key,
            p.kind.clone(),
            p.snapshot,
            s.strategy.clone(),
            s.observed,
        )
    };
    if fields.get("result")?.is_some() == fields.get("error")?.is_some() {
        return Err(AcpError::InvalidMessage);
    }
    let response = if let Some(error) = fields.get("error")? {
        object(error)?;
        let error_code = field(error, "code")?
            .integer()
            .ok_or(AcpError::InvalidMessage)?;
        text(field(error, "message")?)?;
        Response::RemoteError { code: error_code }
    } else {
        let result = fields.get("result")?.ok_or(AcpError::InvalidMessage)?;
        object(result)?;
        match &kind {
            RequestKind::Initialize(offer) => Response::Initialized(Strategy::select(
                *offer,
                core.scope.clone(),
                raw.clone(),
                result,
            )?),
            RequestKind::NewSession => {
                Response::NewSession(SessionId::parse(field(result, "sessionId")?)?)
            }
            RequestKind::Prompt(_) => Response::Prompt(
                strategy
                    .as_ref()
                    .ok_or(AcpError::NotInitialized)?
                    .prompt_result(result)?,
            ),
            RequestKind::Resume(session) | RequestKind::Replay(session) => {
                if strategy
                    .as_ref()
                    .is_some_and(|s| s.version() == Version::V2)
                    && !snapshot
                {
                    return Err(AcpError::MissingSnapshot);
                }
                if matches!(kind, RequestKind::Replay(_)) {
                    Response::Replayed {
                        session: session.clone(),
                    }
                } else {
                    Response::Resumed {
                        session: session.clone(),
                    }
                }
            }
            RequestKind::ListSessions => {
                let strategy = strategy.as_ref().ok_or(AcpError::NotInitialized)?;
                if observed {
                    strategy.require_list()?;
                    Response::DeferredSessions(strategy.clone())
                } else {
                    Response::Sessions(strategy.session_listing(result)?)
                }
            }
            RequestKind::Close(session) => Response::Closed {
                session: session.clone(),
            },
        }
    };
    let mut s = core.lock();
    if s.pending.get(&id).is_none_or(|p| p.cell.key != key) {
        return Err(AcpError::UnmatchedResponse);
    }
    s.pending.remove(&id);
    s.expiry_waiting.remove(&key);
    if let Response::Initialized(strategy) = &response {
        s.strategy = Some(strategy.clone());
    }
    let failed_init = matches!(kind, RequestKind::Initialize(_))
        && matches!(response, Response::RemoteError { .. });
    s.emit(
        core.scope.clone(),
        cut,
        Incoming::Response { id, response, raw },
    )?;
    if failed_init {
        s.stop(Some(AcpError::DeliveryUnknown));
    }
    Ok(())
}

fn receipt_context(
    state: &super::state::State,
    cut: &ReceiptCut,
    session: &SessionId,
    scope: &super::ConnectionScope,
) -> (
    super::ViewReadContextKind,
    Option<super::ReceivedReadWindow>,
) {
    let selected = select_unique_cell(cut.windows.iter().filter_map(|(target, key, _)| {
                (target == session && state.pending.values().any(|pending| {
                    pending.cell.key == *key && pending.accepted
                    && matches!(&pending.kind, RequestKind::Resume(sid) | RequestKind::Replay(sid) if sid == session)
                })).then_some(*key)
            }));
    let view_window = if let CellSelection::Unique(key) = selected {
        state.pending.iter().find_map(|(rpc, pending)| {
            if pending.cell.key != key {
                return None;
            }
            let kind = match &pending.kind {
                RequestKind::Resume(_) => super::ViewReadWindowKind::Resume,
                RequestKind::Replay(_) => super::ViewReadWindowKind::Replay,
                _ => return None,
            };
            Some(super::ReceivedReadWindow {
                scope: scope.clone(),
                session: session.clone(),
                rpc: rpc.clone(),
                kind,
                cell_key: pending.cell.key,
            })
        })
    } else {
        None
    };
    let view_context = match selected {
        CellSelection::Outside => super::ViewReadContextKind::OutsideReadWindow,
        CellSelection::Unique(_) => super::ViewReadContextKind::UniqueReceiptWindow,
        CellSelection::Ambiguous => super::ViewReadContextKind::AmbiguousReceiptWindow,
    };

    (view_context, view_window)
}

#[cfg(test)]
mod freshness_development_tests;

#[cfg(test)]
mod view_context_selection_tests {
    use super::{CellSelection, select_unique_cell};
    #[test]
    fn shared_view_unique_receipt_selector_deduplicates_cells_and_preserves_ambiguity() {
        assert_eq!(select_unique_cell([].into_iter()), CellSelection::Outside);
        assert_eq!(
            select_unique_cell([7, 7, 7].into_iter()),
            CellSelection::Unique(7)
        );
        assert_eq!(
            select_unique_cell([7, 9, 7].into_iter()),
            CellSelection::Ambiguous
        );
        assert_eq!(
            select_unique_cell([9, 7, 9].into_iter()),
            CellSelection::Ambiguous
        );
    }
}

//! Child history shares one source-owned job. Each waiter retains its own authorization.
use super::{AcpNativeAdapter, AcpSourceViews, NativeSessionKey, Received, ViewError};
use crate::{
    adapters::acp::{
        Incoming, RawMessage, Response, ResponseDeadline, RpcId, WriteReport,
        child_history::{ChildHistoryAssembly, ChildHistoryProgress},
        shared_view_read::request_error,
    },
    application::{
        native_dispatch::{SourceReadCapability, read_access::OwnedReadTicketJob},
        shared_views::BoxReadFuture,
    },
    transcript_cache::TranscriptCacheCoordinator,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};
use tokio::sync::{oneshot, watch};

type Permit = tokio::sync::OwnedMutexGuard<()>;
pub type Check = dyn Fn() -> BoxReadFuture<'static, Result<Permit, ViewError>> + Send + Sync;
type Cap = SourceReadCapability<AcpNativeAdapter>;
type Reply = Result<(RawMessage, u64), ViewError>;

#[derive(Default)]
pub(super) struct Queries {
    flights: Mutex<HashMap<NativeSessionKey, Weak<Flight>>>,
    replies: Mutex<HashMap<RpcId, oneshot::Sender<Reply>>>,
}
struct Flight {
    observers: Mutex<Vec<Weak<Check>>>,
    done: watch::Sender<Option<Result<(), ViewError>>>,
}
impl Flight {
    async fn authorize(&self) -> Result<Permit, ViewError> {
        let mut checked = 0;
        let mut last_error = ViewError::Revoked;
        loop {
            let pending = {
                let observers = self.observers.lock().map_err(|_| ViewError::ReadFailure)?;
                if checked == observers.len() {
                    return Err(last_error);
                }
                let pending = observers[checked..].to_vec();
                checked = observers.len();
                pending
            };
            for observer in pending {
                if let Some(check) = observer.upgrade() {
                    match check().await {
                        Ok(permit) if observer.strong_count() > 1 => return Ok(permit),
                        Err(error) => last_error = error,
                        Ok(_) => {}
                    }
                }
            }
        }
    }
}

impl AcpSourceViews {
    #[cfg(test)]
    pub(crate) fn child_observers(&self, key: &NativeSessionKey) -> usize {
        self.children
            .flights
            .lock()
            .unwrap()
            .get(key)
            .and_then(Weak::upgrade)
            .map_or(0, |f| {
                f.observers
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|o| o.strong_count() > 0)
                    .count()
            })
    }
    pub(super) fn child_response(&self, received: &Received) -> Result<(), ViewError> {
        if let Incoming::Response { id, response, raw } = received.incoming()
            && let Some(tx) = self
                .children
                .replies
                .lock()
                .map_err(|_| ViewError::ReadFailure)?
                .remove(id)
        {
            let _ = tx.send(match response {
                Response::ChildHistory => Ok((raw.clone(), received.sequence())),
                Response::RemoteError { code: -32601 } => Err(ViewError::UnsupportedCapability),
                _ => Err(ViewError::ProtocolFailure),
            });
        }
        Ok(())
    }

    pub(crate) async fn child_history(
        self: &Arc<Self>,
        cap: Arc<Cap>,
        observer: Arc<Check>,
        tc: Arc<TranscriptCacheCoordinator>,
    ) -> Result<(), ViewError> {
        let connection = self.source.connection().map_err(ViewError::NativeFailure)?;
        let strategy = connection.strategy().ok_or(ViewError::SourceReleased)?;
        if connection.scope() != *cap.scope()
            || u64::try_from(cap.configuration_binding().1.number()).ok()
                != Some(strategy.version().number())
        {
            return Err(ViewError::InvalidConfiguration);
        }
        let root = super::replay::session(cap.root_native())?;
        let child = super::replay::session(cap.native())?;
        strategy
            .child_history(&root, &child, None)
            .map_err(request_error)?;
        drop(observer().await?);
        let mut done = {
            let mut flights = self
                .children
                .flights
                .lock()
                .map_err(|_| ViewError::ReadFailure)?;
            flights.retain(|_, flight| flight.strong_count() != 0);
            if let Some(flight) = flights.get(cap.native()).and_then(Weak::upgrade) {
                flight
                    .observers
                    .lock()
                    .map_err(|_| ViewError::ReadFailure)?
                    .push(Arc::downgrade(&observer));
                flight.done.subscribe()
            } else {
                let lease = cap.claim_child_history()?;
                let (tx, rx) = watch::channel(None);
                let flight = Arc::new(Flight {
                    observers: Mutex::new(vec![Arc::downgrade(&observer)]),
                    done: tx,
                });
                let views = self.clone();
                let task_cap = cap.clone();
                let work = flight.clone();
                let fail = flight.clone();
                let key = cap.native().clone();
                self.registrar
                    .adopt(OwnedReadTicketJob {
                        capability: cap,
                        view_generation: None,
                        work: Box::pin(async move {
                            let result = views.read_child(&task_cap, &work, &tc).await;
                            drop(lease);
                            work.done.send_replace(Some(result));
                        }),
                        on_failure: Box::new(move || {
                            fail.done.send_replace(Some(Err(ViewError::SourceReleased)));
                        }),
                    })
                    .map_err(|err| err.error)?;
                flights.insert(key, Arc::downgrade(&flight));
                rx
            }
        };
        loop {
            let result = *done.borrow_and_update();
            if let Some(result) = result {
                result?;
                break;
            }
            tokio::select! {
                result = done.changed() => result.map_err(|_|ViewError::SourceReleased)?,
                () = tokio::time::sleep(Duration::from_millis(100)) => drop(observer().await?),
            }
        }
        observer().await.map(drop)
    }

    async fn read_child(
        &self,
        cap: &Cap,
        flight: &Flight,
        tc: &TranscriptCacheCoordinator,
    ) -> Result<(), ViewError> {
        let connection = self.source.connection().map_err(ViewError::NativeFailure)?;
        let strategy = connection.strategy().ok_or(ViewError::SourceReleased)?;
        let root = super::replay::session(cap.root_native())?;
        let child = super::replay::session(cap.native())?;
        let mut assembly = ChildHistoryAssembly::new(strategy.clone(), root.clone(), child.clone());
        let expected = tc
            .store()
            .history_generation(cap.native())
            .await
            .map_err(|_| ViewError::ReadFailure)?;
        let display = self.views()?.core.target(cap.native())?.send.clone();
        let mut cursor = None;
        let mut receipts = Vec::new();
        loop {
            if !display.current()? {
                return Err(ViewError::StaleCursor);
            }
            cap.require_current()?;
            let request = strategy
                .child_history(&root, &child, cursor.as_deref())
                .map_err(request_error)?;
            let (message, sequence) = self
                .request_chunk(cap, flight, &connection, request)
                .await?;
            match assembly
                .accept(&message)
                .map_err(|_| ViewError::ProtocolFailure)?
            {
                ChildHistoryProgress::More {
                    cursor: next,
                    assembly: more,
                } => {
                    receipts.resize(more.frame_count(), sequence);
                    cursor = Some(next);
                    assembly = more;
                }
                ChildHistoryProgress::Complete(frames) => {
                    drop(flight.authorize().await?);
                    cap.require_current()?;
                    receipts.resize(frames.len(), sequence);
                    let frames = frames.into_iter().zip(receipts).collect();
                    tc.store()
                        .replace_checked(
                            cap.native(),
                            strategy.version(),
                            super::capability_profile_from_strategy(&strategy),
                            frames,
                            Some(expected),
                            async {
                                let permit = flight.authorize().await?;
                                cap.require_current()?;
                                if display.current()? {
                                    Ok(permit)
                                } else {
                                    Err(ViewError::StaleCursor)
                                }
                            },
                        )
                        .await
                        .map_err(|_| ViewError::ReadFailure)?;
                    return Ok(());
                }
            }
        }
    }
    async fn request_chunk(
        &self,
        cap: &Cap,
        flight: &Flight,
        connection: &crate::adapters::acp::Connection,
        request: crate::adapters::acp::Request,
    ) -> Reply {
        let until = tokio::time::Instant::now() + Duration::from_secs(30);
        let reservation = connection
            .reserve_request(request, ResponseDeadline::At(until))
            .map_err(request_error)?;
        let frame = reservation.encode().map_err(request_error)?;
        let rpc = reservation
            .id()
            .cloned()
            .ok_or(ViewError::ProtocolFailure)?;
        let (tx, rx) = oneshot::channel();
        self.children
            .replies
            .lock()
            .map_err(|_| ViewError::ReadFailure)?
            .insert(rpc.clone(), tx);
        let _pending = Pending {
            connection,
            queries: &self.children,
            rpc: rpc.clone(),
        };
        let fence = super::replay::ReplayFence::child_history(cap, rpc);
        let permit = flight.authorize().await?;
        let ticket = connection
            .enqueue(reservation, frame, Arc::new(fence), until)
            .map_err(|err| request_error(err.error()))?;
        drop(permit);
        let response = async {
            if ticket.wait().await != WriteReport::Written {
                return Err(ViewError::TransportFailure);
            }
            rx.await.map_err(|_| ViewError::SourceReleased)?
        };
        tokio::pin!(response);
        loop {
            tokio::select! {
                result = &mut response => return result,
                () = tokio::time::sleep_until(until) => return Err(ViewError::ReadFailure),
                () = tokio::time::sleep(Duration::from_millis(100)) => {cap.require_current()?; drop(flight.authorize().await?);}
            }
        }
    }
}
struct Pending<'a> {
    connection: &'a crate::adapters::acp::Connection,
    queries: &'a Queries,
    rpc: RpcId,
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        self.connection.retire_child_history(&self.rpc);
        if let Ok(mut entries) = self.queries.replies.lock() {
            entries.remove(&self.rpc);
        }
    }
}

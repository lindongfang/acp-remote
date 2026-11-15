//! Real owned duplex display attachment. No socket flush proof can be constructed here.
use super::{
    AcpViewModuleFixture, AuthenticatedLocalViewClient, ConnectionScope, FixtureTarget,
    LocalFixtureViewAuthority, NativeSessionKey, RetainedAcpRaw, ViewError, ViewReadService,
};
use crate::application::shared_views::{
    CurrentViewAuthority,
    budget::ReadReason,
    controller::{HookContext, ViewHookPoint, ViewTestController},
    local_transfer::{LocalChunkWriter, LocalCreditRead, LocalCreditResult, LocalViewStream},
    reader::{
        OpenedView, OriginalReadObserver, PrepareNext, ReadCloseReceipt, ReadRebind,
        ReadRebindMetadata, RecordAddress,
    },
    tasks::{ViewTaskKind, ViewTaskSupervisor},
    wire::{HEADER_BYTES, MAX_PAYLOAD_BYTES, PacketKind, RecordKind, TransferHeader},
};
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, DuplexStream},
    sync::{mpsc, oneshot},
};
type Reads = ViewReadService<
    ConnectionScope,
    RetainedAcpRaw,
    LocalFixtureViewAuthority,
    AuthenticatedLocalViewClient,
>;
mod original_read;
pub use original_read::ModuleOriginalReadProgress;
use original_read::{ContinuationLane, ContinueKind};
#[derive(Clone, Copy)]
enum Signal {
    Running,
    Stale,
    Failed(ViewError),
    Closed,
}
struct Bootstrap {
    peer: DuplexStream,
    stream: crate::claims::Id128,
    epoch: crate::domain::shared_view::ViewEpoch,
}
struct Continue {
    reply: oneshot::Sender<Result<Bootstrap, ViewError>>,
    kind: ContinueKind,
}
#[derive(Clone, Copy)]
struct CreditFact {
    revision: u64,
    received_revision: u64,
    packet: Option<u64>,
    committed_through: u64,
    stream: crate::claims::Id128,
    result: Result<LocalCreditResult, ViewError>,
}
struct Shared {
    original: Mutex<OriginalReadObserver>,
    current_stream: Mutex<LocalViewStream>,
    lane: Mutex<ContinuationLane>,
    controller: ViewTestController<ConnectionScope>,
    scope: ConnectionScope,
    signal: watch::Sender<Signal>,
    consumer_stop: watch::Sender<bool>,
    credit: watch::Sender<CreditFact>,
    closed: Mutex<Option<ReadCloseReceipt>>,
    addresses: Mutex<std::collections::HashMap<(crate::claims::Id128, u64), RecordAddress>>,
}
impl Shared {
    fn record_prepared(
        &self,
        stream: crate::claims::Id128,
        packet: u64,
        address: RecordAddress,
    ) -> Result<(), ViewError> {
        let mut addresses = self.addresses.lock().map_err(|_| ViewError::ReadFailure)?;
        addresses
            .try_reserve(1)
            .map_err(|_| ViewError::AllocationFailure)?;
        if addresses.insert((stream, packet), address).is_some() {
            return Err(ViewError::InvalidStage);
        }
        Ok(())
    }
    fn published_flush(
        &self,
        stream: crate::claims::Id128,
        observation: crate::application::shared_views::local_transfer::LocalFlushObservation,
    ) {
        if observation.committed {
            self.credit.send_modify(|fact| {
                if fact.stream != stream {
                    return;
                }
                if let Some(revision) = fact.revision.checked_add(1) {
                    fact.revision = revision;
                    fact.committed_through = fact.committed_through.max(observation.packet);
                } else {
                    fact.result = Err(ViewError::SequenceExhausted);
                }
            });
        }
    }
}
pub struct ModuleViewTransfer {
    #[cfg(test)]
    last_follow_rejection: Option<original_read::FollowRejection>,
    reads: Arc<Reads>,
    authority: Arc<LocalFixtureViewAuthority>,
    client: AuthenticatedLocalViewClient,
    peer: Option<DuplexStream>,
    stream: crate::claims::Id128,
    epoch: crate::domain::shared_view::ViewEpoch,
    signal: watch::Receiver<Signal>,
    credit: watch::Receiver<CreditFact>,
    shared: Arc<Shared>,
    continue_read: mpsc::Sender<Continue>,
    stale_reported: bool,
    frame: Vec<u8>,
    record: Vec<u8>,
    record_header: Option<TransferHeader>,
}
pub struct CollectedModuleRecord {
    address: RecordAddress,
    bytes: Vec<u8>,
}
impl CollectedModuleRecord {
    #[must_use]
    pub const fn address(&self) -> RecordAddress {
        self.address
    }
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
struct ReceiverOwner {
    stop: watch::Sender<bool>,
    done: watch::Receiver<bool>,
}
impl ReceiverOwner {
    async fn stop(&mut self) {
        self.stop.send_replace(true);
        while !*self.done.borrow_and_update() {
            if self.done.changed().await.is_err() {
                break;
            }
        }
    }
}
async fn qualified<T, F: std::future::Future<Output = Result<T, ViewError>>>(
    authority: &LocalFixtureViewAuthority,
    client: &AuthenticatedLocalViewClient,
    mut stop: watch::Receiver<bool>,
    work: F,
) -> Result<T, ViewError> {
    tokio::pin!(work);
    loop {
        if *stop.borrow() {
            return Err(ViewError::NodeStopping);
        }
        tokio::select! {
            result=&mut work=>return result,
            change=authority.wait_client_change(client)=>{change?;},
            _ = stop.changed()=>{},
        }
    }
}
fn attach_receiver(
    supervisor: &ViewTaskSupervisor,
    authority: Arc<LocalFixtureViewAuthority>,
    client: AuthenticatedLocalViewClient,
    target: NativeSessionKey,
    stream: &LocalViewStream,
    node: DuplexStream,
    shared: Arc<Shared>,
) -> Result<(LocalChunkWriter, ReceiverOwner), ViewError> {
    let binding = authority.client_binding(&client)?;
    let (writer, mut reader) = stream.bind_duplex(node, &binding)?;
    let stream = stream.clone();
    let (stop, stopped) = watch::channel(false);
    let (done, finished) = watch::channel(false);
    let mut owner_stop = supervisor.subscribe_stop();
    let mut publication_stop = stopped.clone();
    supervisor.spawn(ViewTaskKind::Reader, async move {
        loop {
            if *owner_stop.borrow() || *stopped.borrow() {
                break;
            }
            let reading = qualified(
                authority.as_ref(),
                &client,
                stopped.clone(),
                reader.read_credit_unbounded(authority.as_ref(), &client, &target),
            );
            let result = tokio::select! {
                result=reading=>result,
                _ = owner_stop.changed()=>Err(ViewError::NodeStopping),
            };
            if *stopped.borrow() || *owner_stop.borrow() {
                break;
            }
            let packet = result.as_ref().ok().map(|read| read.packet());
            let result = result.map(LocalCreditRead::result);
            let context = HookContext {
                scope: Some(shared.scope.clone()),
                target: Some(target.clone()),
                stream: Some(crate::domain::EntityId::from_bytes(*stream.id().as_bytes())),
                packet,
                ..Default::default()
            };
            tokio::select! {
                outcome=shared.controller.hit(ViewHookPoint::AfterModuleCreditReadBeforePublish,context)=>{outcome?;},
                _ = owner_stop.changed()=>break,
                _ = publication_stop.changed()=>break,
            }
            if *owner_stop.borrow() || *publication_stop.borrow() { break; }
            shared.credit.send_modify(|fact| {
                if fact.stream != stream.id() {
                    return;
                }
                if let (Some(revision), Some(received)) = (fact.revision.checked_add(1), fact.received_revision.checked_add(1)) {
                    fact.revision = revision;
                    fact.received_revision = received;
                    fact.packet = packet;
                    fact.result = result;
                    if matches!(result, Ok(LocalCreditResult::Committed)) && let Some(packet) = packet {
                        fact.committed_through = fact.committed_through.max(packet);
                    }
                } else {
                    fact.result = Err(ViewError::SequenceExhausted);
                }
            });
            if result.is_err()
                || matches!(result, Ok(LocalCreditResult::Sealed))
                || *stopped.borrow()
                || *owner_stop.borrow()
            {
                break;
            }
        }
        done.send_replace(true);
        Ok(())
    })?;
    Ok((
        writer,
        ReceiverOwner {
            stop,
            done: finished,
        },
    ))
}
struct Sender {
    reads: Arc<Reads>,
    authority: Arc<LocalFixtureViewAuthority>,
    client: AuthenticatedLocalViewClient,
    target: NativeSessionKey,
    read: Option<OpenedView>,
    stream: LocalViewStream,
    writer: LocalChunkWriter,
    receiver: ReceiverOwner,
    shared: Arc<Shared>,
    commands: mpsc::Receiver<Continue>,
    supervisor: ViewTaskSupervisor,
}
impl Sender {
    async fn run(mut self) -> Result<(), ViewError> {
        let mut consumer_stop = self.shared.consumer_stop.subscribe();
        let closing = *consumer_stop.borrow_and_update();
        let result = if closing {
            Ok(())
        } else {
            tokio::select! {
                result=self.drive()=>result,
                _=consumer_stop.changed()=>Ok(()),
            }
        };
        self.receiver.stop().await;
        *self
            .shared
            .lane
            .lock()
            .map_err(|_| ViewError::ReadFailure)? = ContinuationLane::Stopped;
        if let Some(read) = self.read.take() {
            let receipt = self.reads.release(read);
            *self
                .shared
                .closed
                .lock()
                .map_err(|_| ViewError::ReadFailure)? = Some(receipt);
        }
        self.shared.signal.send_replace(match result {
            Ok(()) | Err(ViewError::NodeStopping) => Signal::Closed,
            Err(error) => Signal::Failed(error),
        });
        // Consumer transport/protocol failure was delivered to that consumer above.
        // The registered task has still completed its real release/IO ownership cleanup.
        Ok(())
    }
    #[allow(
        clippy::too_many_lines,
        reason = "The local sender keeps each actual credit, source-change and original-read continuation transition explicit"
    )]
    async fn drive(&mut self) -> Result<(), ViewError> {
        let mut snapshot = true;
        let mut credit = self.shared.credit.subscribe();
        let mut stop = self.supervisor.subscribe_stop();
        loop {
            if *stop.borrow() {
                return Err(ViewError::NodeStopping);
            }
            if let Some(command) = self.take_inflight_command().await? {
                if self.accept_inflight(command).await? {
                    snapshot = true;
                }
                continue;
            }
            if !self.begin_prepare()? {
                continue;
            }
            let read = self.read.as_ref().ok_or(ViewError::StreamSealed)?;
            let prepared = if snapshot {
                self.reads.next_snapshot(&self.client, read).await
            } else {
                self.reads.next_delta(&self.client, read).await
            };
            match prepared {
                Ok(PrepareNext::Chunk(chunk)) => {
                    self.shared.record_prepared(
                        self.stream.id(),
                        chunk.packet_sequence(),
                        chunk.address(),
                    )?;
                    let writing = self.writer.write_chunk_unbounded(
                        chunk.into_local(),
                        self.authority.as_ref(),
                        &self.client,
                        &self.target,
                    );
                    let written = tokio::select! {result=writing=>result,_ = stop.changed()=>Err(ViewError::NodeStopping)};
                    match written {
                        Ok(observation) => {
                            self.shared.published_flush(self.stream.id(), observation);
                        }
                        Err(error) => {
                            if error == ViewError::StaleCursor
                                && !self.stream.explicitly_sealed()?
                            {
                                self.continue_same_read().await?;
                                snapshot = true;
                                continue;
                            }
                            return Err(error);
                        }
                    }
                }
                Ok(PrepareNext::SnapshotFinished) => {
                    snapshot = false;
                }
                Ok(PrepareNext::AwaitData) => {
                    self.set_idle_lane(ContinuationLane::AwaitData)?;
                    let mut command = None;
                    let result = tokio::select! {
                        result=self.reads.wait_change(&self.client, read)=>result,
                        incoming=self.commands.recv()=>{
                            command = Some(incoming.ok_or(ViewError::StreamSealed)?);
                            Ok(read.descriptor())
                        },
                        changed=credit.changed()=>{
                            changed.map_err(|_|ViewError::TransportFailure)?;
                            let result = credit.borrow().result;
                            match result {
                                Ok(LocalCreditResult::Sealed)=>return Ok(()),
                                Err(error)=>return Err(error),
                                _=>continue,
                            }
                        },
                        _ = stop.changed()=>Err(ViewError::NodeStopping)
                    };
                    if let Some(command) = command {
                        if self.accept_inflight(command).await? {
                            snapshot = true;
                        }
                        continue;
                    }
                    if let Err(error) = result {
                        if error == ViewError::StaleCursor && !self.stream.explicitly_sealed()? {
                            self.continue_same_read().await?;
                            snapshot = true;
                            continue;
                        }
                        return Err(error);
                    }
                }
                Ok(PrepareNext::AwaitCredit) => {
                    self.set_idle_lane(ContinuationLane::AwaitCredit)?;
                    // Subscribe before sampling. The credit reader is independent of slow writes.
                    let fact = *credit.borrow_and_update();
                    fact.result?;
                    if matches!(fact.result, Ok(LocalCreditResult::Sealed)) {
                        return Ok(());
                    }
                    match self.stream.ready() {
                        Ok(true) | Err(ViewError::StaleCursor) => continue,
                        Ok(false) => {}
                        Err(error) => return Err(error),
                    }
                    tokio::select! {
                        change=credit.changed()=>change.map_err(|_|ViewError::TransportFailure)?,
                        changed=self.authority.wait_client_change(&self.client)=>{changed?;},
                        _ = self.reads.wait_change(&self.client,read)=>{},
                        _ = stop.changed()=>return Err(ViewError::NodeStopping),
                    }
                }
                Ok(PrepareNext::Closed) => return Ok(()),
                Err(ViewError::StaleCursor) if !self.stream.explicitly_sealed()? => {
                    self.continue_same_read().await?;
                    snapshot = true;
                }
                Err(error) => {
                    if self.stream.explicitly_sealed()?
                        && matches!(
                            self.shared.credit.borrow().result,
                            Ok(LocalCreditResult::Sealed)
                        )
                    {
                        return Ok(());
                    }
                    return Err(error);
                }
            }
        }
    }
    async fn continue_same_read(&mut self) -> Result<(), ViewError> {
        self.shared.signal.send_replace(Signal::Stale);
        let mut stop = self.supervisor.subscribe_stop();
        let command = tokio::select! {
            command=self.commands.recv()=>command.ok_or(ViewError::StreamSealed)?,
            _ = stop.changed()=>return Err(ViewError::NodeStopping),
        };
        if matches!(&command.kind, ContinueKind::InFlight { .. }) {
            self.accept_inflight(command).await?;
            return Ok(());
        }
        let old = self.read.as_ref().ok_or(ViewError::StreamSealed)?;
        self.reads
            .wait_original_completion(&self.client, old)
            .await?;
        let rebound = self.reads.rebind_same_read(&self.client, old).await?;
        self.install_rebound(command, rebound).await
    }
    async fn install_rebound(
        &mut self,
        command: Continue,
        rebound: ReadRebind,
    ) -> Result<(), ViewError> {
        let (new, metadata) = rebound.into_parts();
        self.receiver.stop().await;
        let (node, peer) = tokio::io::duplex(MAX_PAYLOAD_BYTES + HEADER_BYTES);
        let stream = new.stream();
        let revision = self
            .shared
            .credit
            .borrow()
            .revision
            .checked_add(1)
            .ok_or(ViewError::SequenceExhausted)?;
        self.shared.credit.send_replace(CreditFact {
            revision,
            received_revision: 0,
            packet: None,
            committed_through: 0,
            stream: stream.id(),
            result: Ok(LocalCreditResult::Duplicate),
        });
        let (writer, receiver) = attach_receiver(
            &self.supervisor,
            Arc::clone(&self.authority),
            self.client.owned_task_ref(),
            self.target.clone(),
            &stream,
            node,
            Arc::clone(&self.shared),
        )?;
        self.writer = writer;
        self.receiver = receiver;
        self.stream = stream;
        *self
            .shared
            .current_stream
            .lock()
            .map_err(|_| ViewError::ReadFailure)? = self.stream.clone();
        *self
            .shared
            .original
            .lock()
            .map_err(|_| ViewError::ReadFailure)? = self.reads.original_observer(&new)?;
        let old = self.read.replace(new);
        drop(old);
        self.shared.signal.send_replace(Signal::Running);
        command
            .reply
            .send(Ok(Bootstrap {
                peer,
                stream: self.stream.id(),
                epoch: metadata.epoch,
            }))
            .map_err(|_| ViewError::StreamSealed)?;
        // The peer first receives the private bootstrap, writes real Credit(0), then reads
        // packet 1 Status. No metadata read itself counts as a delivered notification.
        let notice = notice_bytes(metadata)?;
        let read = self.read.as_ref().ok_or(ViewError::StreamSealed)?;
        let mut credits = self.shared.credit.subscribe();
        loop {
            if self.stream.ready()? {
                break;
            }
            credits
                .changed()
                .await
                .map_err(|_| ViewError::TransportFailure)?;
            credits.borrow().result?;
        }
        let chunk = self
            .stream
            .prepare_copy(
                self.authority.as_ref(),
                &self.client,
                &self.target,
                1,
                0,
                notice.len() as u64,
                RecordKind::Status,
                || Ok(notice),
            )
            .await?;
        self.shared.record_prepared(
            self.stream.id(),
            chunk.packet_sequence(),
            RecordAddress::from_header(metadata.epoch, chunk.header())?,
        )?;
        let flushed = self
            .writer
            .write_chunk_unbounded(chunk, self.authority.as_ref(), &self.client, &self.target)
            .await?;
        self.shared.published_flush(self.stream.id(), flushed);
        self.set_idle_lane(ContinuationLane::AwaitCredit)?;
        let _ = read;
        Ok(())
    }
}
fn notice_bytes(metadata: ReadRebindMetadata) -> Result<Vec<u8>, ViewError> {
    serde_json::to_vec(&serde_json::json!({
        "type":"stream_rebound","previousStreamId":metadata.previous_stream.encode(),
        "streamId":metadata.stream.encode(),"viewEpoch":metadata.epoch.as_entity_id().encode(),
        "readEpoch":metadata.read.as_entity_id().encode(),"snapshotRevision":metadata.revision.as_u64().to_string(),
        "sourceBindingId":metadata.source.binding_id().encode(),"sourceId":metadata.source.source().0.encode(),
    })).map_err(|_|ViewError::AllocationFailure)
}
impl AcpViewModuleFixture {
    /// # Errors
    /// Registers original consumer/sender/credit tasks before any display IO is polled.
    pub async fn open_transfer(
        &self,
        client: &AuthenticatedLocalViewClient,
        target: &FixtureTarget,
        reason: ReadReason,
    ) -> Result<ModuleViewTransfer, ViewError> {
        if target.scope != self.scope() {
            return Err(ViewError::InvalidScope);
        }
        let read = self.reads.open(client, &target.key, reason).await?;
        let stream = read.stream();
        let (node, peer) = tokio::io::duplex(MAX_PAYLOAD_BYTES + HEADER_BYTES);
        let (signal, signals) = watch::channel(Signal::Running);
        let (consumer_stop, _) = watch::channel(false);
        let (credit, credits) = watch::channel(CreditFact {
            revision: 0,
            received_revision: 0,
            packet: None,
            committed_through: 0,
            stream: stream.id(),
            result: Ok(LocalCreditResult::Duplicate),
        });
        let shared = Arc::new(Shared {
            original: Mutex::new(self.reads.original_observer(&read)?),
            current_stream: Mutex::new(stream.clone()),
            lane: Mutex::new(ContinuationLane::AwaitCredit),
            controller: self.view_owner.test_controller(),
            scope: self.scope(),
            signal,
            consumer_stop,
            credit,
            closed: Mutex::new(None),
            addresses: Mutex::new(std::collections::HashMap::new()),
        });
        let (send, commands) = mpsc::channel(1);
        let authority = self.auth.authority();
        let supervisor = self.view_owner.supervisor.clone();
        let (writer, receiver) = attach_receiver(
            &supervisor,
            Arc::clone(&authority),
            client.owned_task_ref(),
            target.key.clone(),
            &stream,
            node,
            Arc::clone(&shared),
        )?;
        let sender = Sender {
            reads: Arc::clone(&self.reads),
            authority: Arc::clone(&authority),
            client: client.owned_task_ref(),
            target: target.key.clone(),
            read: Some(read),
            stream: stream.clone(),
            writer,
            receiver,
            shared: Arc::clone(&shared),
            commands,
            supervisor: supervisor.clone(),
        };
        let epoch = sender
            .read
            .as_ref()
            .ok_or(ViewError::InvalidStage)?
            .descriptor()
            .view_epoch();
        supervisor.spawn(ViewTaskKind::Reader, sender.run())?;
        let mut transfer = ModuleViewTransfer {
            #[cfg(test)]
            last_follow_rejection: None,
            reads: Arc::clone(&self.reads),
            authority,
            client: client.owned_task_ref(),
            peer: Some(peer),
            stream: stream.id(),
            epoch,
            signal: signals,
            credit: credits,
            shared,
            continue_read: send,
            stale_reported: false,
            frame: Vec::new(),
            record: Vec::new(),
            record_header: None,
        };
        transfer.credit(0).await?;
        Ok(transfer)
    }
}
#[cfg(test)]
mod transfer001_tests;
impl ModuleViewTransfer {
    async fn next_packet(&mut self) -> Result<Vec<u8>, ViewError> {
        loop {
            let state = *self.signal.borrow_and_update();
            match state {
                Signal::Stale => return Err(ViewError::StaleCursor),
                Signal::Failed(error) => return Err(error),
                Signal::Closed => return Err(ViewError::StreamSealed),
                Signal::Running => {}
            }
            let reading = read_packet(
                self.peer.as_mut().ok_or(ViewError::StreamSealed)?,
                &mut self.frame,
            );
            tokio::select! {
                result=reading=>return result,
                change=self.signal.changed()=>change.map_err(|_|ViewError::StreamSealed)?,
            }
        }
    }
    /// # Errors
    /// Returns only bytes actually received by this client. Intermediate chunks receive real
    /// wire credit here; the last packet remains for the caller's explicit credit(packet).
    pub async fn next_record(&mut self) -> Result<CollectedModuleRecord, ViewError> {
        if self.stale_reported {
            let (reply, ready) = oneshot::channel();
            self.continue_read
                .send(Continue {
                    reply,
                    kind: ContinueKind::Completed,
                })
                .await
                .map_err(|_| ViewError::StreamSealed)?;
            let bootstrap = ready.await.map_err(|_| ViewError::StreamSealed)??;
            self.peer = Some(bootstrap.peer);
            self.stream = bootstrap.stream;
            self.epoch = bootstrap.epoch;
            self.frame.clear();
            self.record.clear();
            self.record_header = None;
            self.stale_reported = false;
            self.credit(0).await?;
        }
        loop {
            let packet = match self.next_packet().await {
                Err(ViewError::StaleCursor) => {
                    self.frame.clear();
                    self.record.clear();
                    self.record_header = None;
                    self.stale_reported = true;
                    return Err(ViewError::StaleCursor);
                }
                other => other?,
            };
            let (header, payload) =
                TransferHeader::decode(&packet).map_err(|_| ViewError::ProtocolFailure)?;
            if header.kind() != PacketKind::Data || header.stream() != self.stream {
                return Err(ViewError::ProtocolFailure);
            }
            let address = self
                .shared
                .addresses
                .lock()
                .map_err(|_| ViewError::ReadFailure)?
                .remove(&(self.stream, header.packet()))
                .ok_or(ViewError::ProtocolFailure)?;
            let header_address = RecordAddress::from_header(self.epoch, header)?
                .in_segment(address.segment_view_epoch());
            if address != header_address {
                return Err(ViewError::ProtocolFailure);
            }
            if let Some(first) = self.record_header {
                if first.record() != header.record()
                    || first.record_kind() != header.record_kind()
                    || first.total() != header.total()
                    || header.offset() != self.record.len() as u64
                {
                    return Err(ViewError::ProtocolFailure);
                }
            } else if header.offset() != 0 {
                return Err(ViewError::ProtocolFailure);
            } else {
                self.record_header = Some(header);
            }
            self.record
                .try_reserve(payload.len())
                .map_err(|_| ViewError::AllocationFailure)?;
            self.record.extend_from_slice(payload);
            if header.is_last() {
                self.record_header = None;
                return Ok(CollectedModuleRecord {
                    address,
                    bytes: std::mem::take(&mut self.record),
                });
            }
            self.credit(header.packet()).await?;
        }
    }
    /// # Errors
    /// Actual peer bytes must be parsed and qualified by the independent local credit owner.
    pub async fn credit(&mut self, packet: u64) -> Result<(), ViewError> {
        let signal = *self.signal.borrow();
        if let Signal::Failed(error) = signal {
            return Err(error);
        }
        if self.credit.borrow().stream != self.stream {
            return Err(ViewError::InvalidStage);
        }
        self.credit.borrow().result?;
        let before = self.credit.borrow_and_update().received_revision;
        let bytes = TransferHeader::credit(self.stream, packet)
            .encode(&[])
            .map_err(|_| ViewError::ProtocolFailure)?;
        self.peer
            .as_mut()
            .ok_or(ViewError::StreamSealed)?
            .write_all(&bytes)
            .await
            .map_err(|_| ViewError::TransportFailure)?;
        loop {
            let fact = *self.credit.borrow_and_update();
            fact.result?;
            if fact.stream != self.stream {
                return Err(ViewError::InvalidStage);
            }
            if fact.received_revision > before && fact.packet == Some(packet) {
                match fact.result {
                    Ok(LocalCreditResult::HeldEarly) if fact.committed_through < packet => {}
                    Ok(LocalCreditResult::Sealed) => return Err(ViewError::StreamSealed),
                    _ => return Ok(()),
                }
            }
            tokio::select! {
                changed=self.credit.changed()=>changed.map_err(|_|ViewError::StreamSealed)?,
                changed=self.signal.changed()=>{
                    changed.map_err(|_|ViewError::StreamSealed)?;
                    let signal = *self.signal.borrow();
                    match signal {Signal::Failed(error)=>return Err(error),Signal::Closed=>return Err(ViewError::StreamSealed),_=>{}}
                }
            }
        }
    }
    /// # Errors
    /// Sends actual Detach and waits for the registered sender to release its original read.
    pub async fn detach(&mut self) -> Result<ReadCloseReceipt, ViewError> {
        // Closing this unique consumer requests its owned sender to stop; only the
        // sender's real receiver join/read release below can produce the close receipt.
        self.shared.consumer_stop.send_replace(true);
        let bytes = TransferHeader::detach(self.stream)
            .encode(&[])
            .map_err(|_| ViewError::ProtocolFailure)?;
        if let Some(peer) = self.peer.as_mut() {
            let _ = peer.write_all(&bytes).await;
        }
        // Closing this consumer drops its actual read endpoint too, so an already-started
        // fixed chunk cannot strand its registered writer when this peer stops draining.
        self.peer.take();
        loop {
            let receipt = self
                .shared
                .closed
                .lock()
                .map_err(|_| ViewError::ReadFailure)?
                .clone();
            if let Some(receipt) = receipt {
                return Ok(receipt);
            }
            self.signal
                .changed()
                .await
                .map_err(|_| ViewError::ShutdownUnconfirmed)?;
        }
    }
}
impl Drop for ModuleViewTransfer {
    fn drop(&mut self) {
        self.shared.consumer_stop.send_replace(true);
    }
}
async fn read_packet(peer: &mut DuplexStream, frame: &mut Vec<u8>) -> Result<Vec<u8>, ViewError> {
    loop {
        let total = if frame.len() < HEADER_BYTES {
            HEADER_BYTES
        } else {
            let count = u32::from_be_bytes(
                frame[64..68]
                    .try_into()
                    .map_err(|_| ViewError::ProtocolFailure)?,
            ) as usize;
            if count > MAX_PAYLOAD_BYTES {
                return Err(ViewError::ProtocolFailure);
            }
            HEADER_BYTES + count
        };
        if frame.len() == total && total >= HEADER_BYTES {
            if frame.len() == HEADER_BYTES && frame[64..68] != [0; 4] {
                continue;
            }
            return Ok(std::mem::take(frame));
        }
        let mut bytes = [0; 4096];
        let count = (total - frame.len()).min(bytes.len());
        let received = peer
            .read(&mut bytes[..count])
            .await
            .map_err(|_| ViewError::TransportFailure)?;
        if received == 0 {
            return Err(ViewError::TransportFailure);
        }
        frame
            .try_reserve(received)
            .map_err(|_| ViewError::AllocationFailure)?;
        frame.extend_from_slice(&bytes[..received]);
    }
}

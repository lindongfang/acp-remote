//! Socket-owned buffers and custody over the original Node body preparation service.
//! No task, parser, request registry, native proof or dispatch owner is created here.
#![allow(
    clippy::match_same_arms,
    clippy::needless_pass_by_value,
    clippy::redundant_pub_crate,
    clippy::significant_drop_tightening,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    NodeRequestAuthority,
    prompt_preparation::{
        PreparationReceipt, PreparedBodyHandle, PreparedPrompt, PreparedPromptLease,
    },
};
use crate::application::catalog_preparation::{
    CatalogBufferError, CatalogBufferPhase, CatalogBufferSlot, CatalogBufferStatus,
    CatalogJobResult, CatalogPreparationFailure, CatalogPreparationOwner, CatalogWorkReceipt,
};
use crate::{
    application::{
        request_registry::RequestAuthority,
        shared_views::socket_authority::{
            LiveSocketContext, SocketLifetimeParticipant, SocketViewAuthority, SocketViewClient,
        },
    },
    claims::Id128,
    domain::{EntityId, request_registry::RegistryError, shared_view::ViewError},
    management_ws::input_upload::{InputAck, InputData, UploadError},
};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, Weak},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UploadPurpose {
    Submit,
    Edit,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UploadPhase {
    Receiving,
    Preparing,
    Ready,
    Failed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UploadRoutingError {
    Business(UploadError),
    Revoked,
    StorageUnavailable,
}
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UploadRouteFault {
    ActorRevoked,
    ActorStorageUnavailable,
    UploadStorageUnavailable,
}
#[cfg(test)]
#[derive(Default)]
struct UploadRouteFaults {
    state: Mutex<UploadRouteFaultState>,
}
#[cfg(test)]
#[derive(Default)]
struct UploadRouteFaultState {
    next_token: u64,
    armed: Option<ArmedUploadRouteFault>,
}
#[cfg(test)]
struct ArmedUploadRouteFault {
    token: u64,
    connection: Id128,
    fault: UploadRouteFault,
    hits: Arc<AtomicUsize>,
}
#[cfg(test)]
pub(crate) struct UploadRouteFaultReceipt {
    faults: Arc<UploadRouteFaults>,
    token: u64,
    connection: Id128,
    fault: UploadRouteFault,
    hits: Arc<AtomicUsize>,
}
#[cfg(test)]
impl UploadRouteFaults {
    fn arm(
        self: &Arc<Self>,
        connection: Id128,
        fault: UploadRouteFault,
    ) -> UploadRouteFaultReceipt {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.next_token = state.next_token.wrapping_add(1);
        let token = state.next_token;
        let hits = Arc::new(AtomicUsize::new(0));
        state.armed = Some(ArmedUploadRouteFault {
            token,
            connection,
            fault,
            hits: hits.clone(),
        });
        UploadRouteFaultReceipt {
            faults: self.clone(),
            token,
            connection,
            fault,
            hits,
        }
    }
    fn consume(&self, connection: Id128) -> Option<UploadRouteFault> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state
            .armed
            .as_ref()
            .is_some_and(|armed| armed.connection == connection)
        {
            return state.armed.take().map(|armed| {
                armed.hits.fetch_add(1, Ordering::Release);
                armed.fault
            });
        }
        None
    }
}
#[cfg(test)]
impl UploadRouteFaultReceipt {
    pub(crate) fn hits(&self) -> usize {
        self.hits.load(Ordering::Acquire)
    }
}
#[cfg(test)]
impl Drop for UploadRouteFaultReceipt {
    fn drop(&mut self) {
        let mut state = self
            .faults
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state
            .armed
            .as_ref()
            .is_some_and(|armed| armed.token == self.token)
        {
            state.armed = None;
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadStarted {
    upload: Id128,
}
impl UploadStarted {
    pub(crate) const fn upload(&self) -> Id128 {
        self.upload
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadPreparing {
    upload: Id128,
}
impl UploadPreparing {
    pub(crate) const fn upload(&self) -> Id128 {
        self.upload
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadStatus {
    phase: UploadPhase,
    next_offset: u64,
    body: Option<Id128>,
    error: Option<UploadError>,
}
impl UploadStatus {
    pub(crate) const fn phase(self) -> UploadPhase {
        self.phase
    }
    pub(crate) const fn next_offset(self) -> u64 {
        self.next_offset
    }
    pub(crate) const fn body(self) -> Option<Id128> {
        self.body
    }
    pub(crate) const fn error(self) -> Option<UploadError> {
        self.error
    }
}

struct Previous {
    offset: u64,
    length: usize,
}
enum EntryPhase {
    Receiving {
        bytes: Vec<u8>,
        previous: Option<Previous>,
    },
    Preparing(PreparationReceipt),
    Ready {
        body: Id128,
        prompt: PreparedPrompt,
    },
    Failed(UploadError),
    Released,
}
struct InputEntry {
    target: EntityId,
    purpose: UploadPurpose,
    next_offset: u64,
    phase: EntryPhase,
}
enum Entry {
    Input(InputEntry),
    Catalog(CatalogBufferSlot),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DataRoute {
    Input,
    Catalog,
}
impl Entry {
    const fn input(&self) -> Result<&InputEntry, UploadError> {
        match self {
            Self::Input(e) => Ok(e),
            Self::Catalog(_) => Err(UploadError::InvalidOwner),
        }
    }
    const fn input_mut(&mut self) -> Result<&mut InputEntry, UploadError> {
        match self {
            Self::Input(e) => Ok(e),
            Self::Catalog(_) => Err(UploadError::InvalidOwner),
        }
    }
    const fn catalog(&self) -> Result<&CatalogBufferSlot, UploadError> {
        match self {
            Self::Catalog(e) => Ok(e),
            Self::Input(_) => Err(UploadError::InvalidOwner),
        }
    }
}
struct Table {
    sealed: bool,
    entries: HashMap<Id128, Entry>,
}
struct Owner {
    context: Arc<LiveSocketContext>,
    body: PreparedBodyHandle,
    catalog: CatalogPreparationOwner,
    authority: NodeRequestAuthority,
    socket_authority: Arc<SocketViewAuthority>,
    table: Mutex<Table>,
    #[cfg(test)]
    route_faults: Arc<UploadRouteFaults>,
}
#[derive(Clone)]
pub struct UploadOwner {
    original: Arc<Owner>,
}
struct Registry {
    sealed: bool,
    owners: Vec<Weak<Owner>>,
}
pub(super) struct NodeInputUploads {
    body: PreparedBodyHandle,
    catalog: CatalogPreparationOwner,
    authority: NodeRequestAuthority,
    socket_authority: Arc<SocketViewAuthority>,
    registry: Mutex<Registry>,
    #[cfg(test)]
    route_faults: Arc<UploadRouteFaults>,
}
fn lock<T>(value: &Mutex<T>) -> Result<MutexGuard<'_, T>, UploadError> {
    value.lock().map_err(|_| UploadError::StorageUnavailable)
}
const fn registry_error(error: RegistryError) -> UploadError {
    match error {
        RegistryError::InvalidActor => UploadError::InvalidOwner,
        RegistryError::InvalidTarget => UploadError::InvalidTarget,
        RegistryError::NodeStopping | RegistryError::NodeStarting => UploadError::NodeStopping,
        _ => UploadError::StorageUnavailable,
    }
}
const fn view_error(error: ViewError) -> UploadError {
    match error {
        ViewError::AllocationFailure => UploadError::Allocation,
        ViewError::NodeStopping => UploadError::NodeStopping,
        ViewError::InvalidTarget => UploadError::InvalidTarget,
        _ => UploadError::InvalidOwner,
    }
}
const fn registry_routing_error(error: RegistryError) -> UploadRoutingError {
    match error {
        RegistryError::InvalidActor => UploadRoutingError::Revoked,
        RegistryError::StorageUnavailable => UploadRoutingError::StorageUnavailable,
        _ => UploadRoutingError::Business(registry_error(error)),
    }
}
const fn view_routing_error(error: ViewError) -> UploadRoutingError {
    match error {
        ViewError::Revoked => UploadRoutingError::Revoked,
        ViewError::ReadFailure => UploadRoutingError::StorageUnavailable,
        _ => UploadRoutingError::Business(view_error(error)),
    }
}
const fn preparation_error(
    error: crate::application::prompt_preparation::PreparationFailure,
) -> UploadError {
    use crate::application::prompt_preparation::PreparationFailure as P;
    match error {
        P::Allocation => UploadError::Allocation,
        P::MalformedInput => UploadError::InvalidBody,
        P::Stopping => UploadError::NodeStopping,
        _ => UploadError::BodyUnavailable,
    }
}
fn random_id() -> Result<Id128, UploadError> {
    crate::random::bytes()
        .map(Id128::new)
        .map_err(|_| UploadError::StorageUnavailable)
}

impl NodeInputUploads {
    #[cfg_attr(not(test), allow(clippy::missing_const_for_fn))]
    pub(super) fn new(
        body: PreparedBodyHandle,
        catalog: CatalogPreparationOwner,
        authority: NodeRequestAuthority,
        socket_authority: Arc<SocketViewAuthority>,
    ) -> Self {
        Self {
            body,
            catalog,
            authority,
            socket_authority,
            registry: Mutex::new(Registry {
                sealed: false,
                owners: Vec::new(),
            }),
            #[cfg(test)]
            route_faults: Arc::new(UploadRouteFaults::default()),
        }
    }
    pub(super) fn open(&self, client: &SocketViewClient) -> Result<UploadOwner, UploadError> {
        if !Arc::ptr_eq(&self.socket_authority, client.context.authority()) {
            return Err(UploadError::InvalidOwner);
        }
        let mut registry = lock(&self.registry)?;
        if registry.sealed {
            return Err(UploadError::NodeStopping);
        }
        registry.owners.retain(|owner| owner.strong_count() > 0);
        for existing in registry.owners.iter().filter_map(Weak::upgrade) {
            if Arc::ptr_eq(&existing.context, &client.context) {
                client
                    .context
                    .lifetime
                    .with_live(|| Ok(()))
                    .map_err(view_error)?;
                return Ok(UploadOwner { original: existing });
            }
        }
        registry
            .owners
            .try_reserve(1)
            .map_err(|_| UploadError::Allocation)?;
        let owner = Arc::new(Owner {
            context: client.context.clone(),
            body: self.body.clone(),
            catalog: self.catalog.clone(),
            authority: self.authority.clone(),
            socket_authority: self.socket_authority.clone(),
            table: Mutex::new(Table {
                sealed: false,
                entries: HashMap::new(),
            }),
            #[cfg(test)]
            route_faults: self.route_faults.clone(),
        });
        let participant: Arc<dyn SocketLifetimeParticipant> = owner.clone();
        client
            .context
            .lifetime
            .register_participant(Arc::downgrade(&participant))
            .map_err(view_error)?;
        registry.owners.push(Arc::downgrade(&owner));
        Ok(UploadOwner { original: owner })
    }
    pub(super) fn seal(&self) {
        let mut registry = self
            .registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        registry.sealed = true;
        for owner in registry.owners.iter().filter_map(Weak::upgrade) {
            owner.seal();
        }
    }
    #[cfg(test)]
    pub(crate) fn arm_route_fault_for_test(
        &self,
        client: &SocketViewClient,
        fault: UploadRouteFault,
    ) -> Result<UploadRouteFaultReceipt, UploadError> {
        if !Arc::ptr_eq(&self.socket_authority, client.context.authority()) {
            return Err(UploadError::InvalidOwner);
        }
        client
            .context
            .lifetime
            .with_live(|| Ok(()))
            .map_err(view_error)?;
        let table = lock(&self.registry)?;
        if table.sealed {
            return Err(UploadError::NodeStopping);
        }
        drop(table);
        Ok(self.route_faults.arm(client.context.connection_id(), fault))
    }
}
impl SocketLifetimeParticipant for Owner {
    fn seal(&self) {
        let mut table = self
            .table
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        table.sealed = true;
        table.entries.clear();
    }
}
impl Owner {
    fn check(&self, client: &SocketViewClient) -> Result<(), UploadError> {
        if !Arc::ptr_eq(&self.context, &client.context) {
            return Err(UploadError::InvalidOwner);
        }
        self.context
            .lifetime
            .with_live(|| Ok(()))
            .map_err(view_error)
    }
    async fn current(&self, client: &SocketViewClient) -> Result<(), UploadError> {
        self.check(client)?;
        self.authority
            .current(self.context.actor())
            .await
            .map_err(registry_error)?;
        Ok(())
    }
    async fn current_typed(&self, client: &SocketViewClient) -> Result<(), UploadRoutingError> {
        self.check(client).map_err(UploadRoutingError::Business)?;
        self.authority
            .current(self.context.actor())
            .await
            .map(|_| ())
            .map_err(registry_routing_error)
    }
    async fn target_guard(
        &self,
        client: &SocketViewClient,
        target: EntityId,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, UploadError> {
        self.current(client).await?;
        self.authority
            .resolve_target(target)
            .await
            .map_err(registry_error)?;
        let (_, guard) = self
            .socket_authority
            .current_actor_guard(client)
            .await
            .map_err(view_error)?;
        Ok(guard)
    }
    async fn target_guard_typed(
        &self,
        client: &SocketViewClient,
        target: EntityId,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, UploadRoutingError> {
        self.current_typed(client).await?;
        self.authority
            .resolve_target(target)
            .await
            .map_err(registry_routing_error)?;
        let (_, guard) = self
            .socket_authority
            .current_actor_guard(client)
            .await
            .map_err(view_routing_error)?;
        Ok(guard)
    }
    async fn entry_guard(
        &self,
        client: &SocketViewClient,
        upload: Id128,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, UploadError> {
        self.current(client).await?;
        let target = {
            let table = lock(&self.table)?;
            if table.sealed {
                return Err(UploadError::NodeStopping);
            }
            table
                .entries
                .get(&upload)
                .ok_or(UploadError::BodyUnavailable)?
                .input()?
                .target
        };
        self.target_guard(client, target).await
    }
    async fn typed_entry_guard(
        &self,
        client: &SocketViewClient,
        upload: Id128,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, UploadRoutingError> {
        self.check(client).map_err(UploadRoutingError::Business)?;
        #[cfg(test)]
        if let Some(fault) = self.route_faults.consume(client.context.connection_id()) {
            return Err(match fault {
                UploadRouteFault::ActorRevoked => UploadRoutingError::Revoked,
                UploadRouteFault::ActorStorageUnavailable => UploadRoutingError::StorageUnavailable,
                UploadRouteFault::UploadStorageUnavailable => {
                    UploadRoutingError::Business(UploadError::StorageUnavailable)
                }
            });
        }
        let target = {
            let table = lock(&self.table).map_err(UploadRoutingError::Business)?;
            if table.sealed {
                return Err(UploadRoutingError::Business(UploadError::NodeStopping));
            }
            table
                .entries
                .get(&upload)
                .ok_or(UploadRoutingError::Business(UploadError::BodyUnavailable))?
                .input()
                .map_err(UploadRoutingError::Business)?
                .target
        };
        self.target_guard_typed(client, target).await
    }
    fn mutate<T>(
        &self,
        f: impl FnOnce(&mut Table) -> Result<T, UploadError>,
    ) -> Result<T, UploadError> {
        self.context
            .lifetime
            .with_live(|| {
                let mut table = self.table.lock().map_err(|_| ViewError::ReadFailure)?;
                Ok(if table.sealed {
                    Err(UploadError::NodeStopping)
                } else {
                    f(&mut table)
                })
            })
            .map_err(view_error)?
    }
}
impl UploadOwner {
    pub(crate) async fn begin(
        &self,
        client: &SocketViewClient,
        target: EntityId,
        purpose: UploadPurpose,
    ) -> Result<UploadStarted, UploadError> {
        let _guard = self.original.target_guard(client, target).await?;
        self.begin_after_guard(target, purpose)
    }
    fn begin_after_guard(
        &self,
        target: EntityId,
        purpose: UploadPurpose,
    ) -> Result<UploadStarted, UploadError> {
        self.original.mutate(|table| {
            table
                .entries
                .try_reserve(1)
                .map_err(|_| UploadError::Allocation)?;
            let upload = loop {
                let id = random_id()?;
                if !table.entries.contains_key(&id) {
                    break id;
                }
            };
            table.entries.insert(
                upload,
                Entry::Input(InputEntry {
                    target,
                    purpose,
                    next_offset: 0,
                    phase: EntryPhase::Receiving {
                        bytes: Vec::new(),
                        previous: None,
                    },
                }),
            );
            Ok(UploadStarted { upload })
        })
    }
    pub(crate) async fn begin_typed(
        &self,
        client: &SocketViewClient,
        target: EntityId,
        purpose: UploadPurpose,
    ) -> Result<UploadStarted, UploadRoutingError> {
        let _guard = self.original.target_guard_typed(client, target).await?;
        self.begin_after_guard(target, purpose)
            .map_err(UploadRoutingError::Business)
    }
    pub(crate) async fn accept_data(
        &self,
        client: &SocketViewClient,
        data: InputData<'_>,
    ) -> Result<InputAck, UploadError> {
        let _guard = self.original.entry_guard(client, data.upload()).await?;
        self.accept_data_after_guard(data)
    }
    fn accept_data_after_guard(&self, data: InputData<'_>) -> Result<InputAck, UploadError> {
        self.original.mutate(|table| {
            let entry = table
                .entries
                .get_mut(&data.upload())
                .ok_or(UploadError::BodyUnavailable)?
                .input_mut()?;
            let EntryPhase::Receiving { bytes, previous } = &mut entry.phase else {
                return Err(UploadError::InvalidState);
            };
            if let Some(prior) = previous
                .as_ref()
                .filter(|prior| prior.offset == data.offset())
            {
                let start = usize::try_from(prior.offset).map_err(|_| UploadError::Conflict)?;
                let end = start
                    .checked_add(prior.length)
                    .ok_or(UploadError::Conflict)?;
                if prior.length != data.payload().len()
                    || bytes.get(start..end) != Some(data.payload())
                {
                    return Err(UploadError::Conflict);
                }
                return Ok(InputAck::received(data.upload(), entry.next_offset));
            }
            if data.offset() != entry.next_offset {
                return Err(UploadError::Conflict);
            }
            let next = data
                .offset()
                .checked_add(data.payload().len() as u64)
                .ok_or(UploadError::Conflict)?;
            usize::try_from(next).map_err(|_| UploadError::Allocation)?;
            bytes
                .try_reserve(data.payload().len())
                .map_err(|_| UploadError::Allocation)?;
            bytes.extend_from_slice(data.payload());
            *previous = Some(Previous {
                offset: data.offset(),
                length: data.payload().len(),
            });
            entry.next_offset = next;
            Ok(InputAck::received(data.upload(), next))
        })
    }
    pub(crate) async fn accept_data_typed(
        &self,
        client: &SocketViewClient,
        data: InputData<'_>,
    ) -> Result<InputAck, UploadRoutingError> {
        let _guard = self
            .original
            .typed_entry_guard(client, data.upload())
            .await?;
        self.accept_data_after_guard(data)
            .map_err(UploadRoutingError::Business)
    }
    pub(crate) async fn accept_data_routed_typed(
        &self,
        client: &SocketViewClient,
        data: InputData<'_>,
    ) -> Result<InputAck, UploadRoutingError> {
        let route = {
            self.original.current_typed(client).await?;
            let table = lock(&self.original.table).map_err(UploadRoutingError::Business)?;
            if table.sealed {
                return Err(UploadRoutingError::Business(UploadError::NodeStopping));
            }
            match table
                .entries
                .get(&data.upload())
                .ok_or(UploadRoutingError::Business(UploadError::BodyUnavailable))?
            {
                Entry::Input(_) => DataRoute::Input,
                Entry::Catalog(_) => DataRoute::Catalog,
            }
        };
        match route {
            DataRoute::Input => self.accept_data_typed(client, data).await,
            DataRoute::Catalog => self.catalog_data_typed(client, data).await,
        }
    }
    pub(crate) async fn finish(
        &self,
        client: &SocketViewClient,
        upload: Id128,
        total: u64,
    ) -> Result<UploadPreparing, UploadError> {
        let _guard = self.original.entry_guard(client, upload).await?;
        self.finish_after_guard(upload, total)
    }
    fn finish_after_guard(
        &self,
        upload: Id128,
        total: u64,
    ) -> Result<UploadPreparing, UploadError> {
        self.original.mutate(|table| {
            let entry = table
                .entries
                .get_mut(&upload)
                .ok_or(UploadError::BodyUnavailable)?
                .input_mut()?;
            if total != entry.next_offset {
                return Err(UploadError::InvalidState);
            }
            let EntryPhase::Receiving { bytes, .. } = &mut entry.phase else {
                return Err(UploadError::InvalidState);
            };
            let bytes = std::mem::take(bytes);
            match self.original.body.begin(bytes) {
                Ok(receipt) => {
                    entry.phase = EntryPhase::Preparing(receipt);
                    Ok(UploadPreparing { upload })
                }
                Err(rejected) => {
                    let error = preparation_error(rejected.kind());
                    if let EntryPhase::Receiving { bytes, .. } = &mut entry.phase {
                        *bytes = rejected.into_upload();
                    }
                    Err(error)
                }
            }
        })
    }
    pub(crate) async fn finish_typed(
        &self,
        client: &SocketViewClient,
        upload: Id128,
        total: u64,
    ) -> Result<UploadPreparing, UploadRoutingError> {
        let _guard = self.original.typed_entry_guard(client, upload).await?;
        self.finish_after_guard(upload, total)
            .map_err(UploadRoutingError::Business)
    }
    pub(crate) async fn status(
        &self,
        client: &SocketViewClient,
        upload: Id128,
    ) -> Result<UploadStatus, UploadError> {
        let _guard = self.original.entry_guard(client, upload).await?;
        self.status_after_guard(upload)
    }
    fn status_after_guard(&self, upload: Id128) -> Result<UploadStatus, UploadError> {
        self.original.mutate(|table| {
            let entry = table
                .entries
                .get_mut(&upload)
                .ok_or(UploadError::BodyUnavailable)?
                .input_mut()?;
            if let EntryPhase::Preparing(receipt) = &mut entry.phase {
                match receipt.try_ready() {
                    Ok(Some(prompt)) => match random_id() {
                        Ok(body) => entry.phase = EntryPhase::Ready { body, prompt },
                        Err(error) => entry.phase = EntryPhase::Failed(error),
                    },
                    Ok(None) => {}
                    Err(error) => entry.phase = EntryPhase::Failed(preparation_error(error)),
                }
            }
            let (phase, body, error) = match &entry.phase {
                EntryPhase::Receiving { .. } => (UploadPhase::Receiving, None, None),
                EntryPhase::Preparing(_) => (UploadPhase::Preparing, None, None),
                EntryPhase::Ready { body, .. } => (UploadPhase::Ready, Some(*body), None),
                EntryPhase::Failed(error) => (UploadPhase::Failed, None, Some(*error)),
                EntryPhase::Released => return Err(UploadError::BodyUnavailable),
            };
            Ok(UploadStatus {
                phase,
                next_offset: entry.next_offset,
                body,
                error,
            })
        })
    }
    pub(crate) async fn status_typed(
        &self,
        client: &SocketViewClient,
        upload: Id128,
    ) -> Result<UploadStatus, UploadRoutingError> {
        let _guard = self.original.typed_entry_guard(client, upload).await?;
        self.status_after_guard(upload)
            .map_err(UploadRoutingError::Business)
    }
    pub(crate) async fn release(
        &self,
        client: &SocketViewClient,
        upload: Id128,
    ) -> Result<(), UploadError> {
        let _guard = self.original.entry_guard(client, upload).await?;
        self.release_after_guard(upload)
    }
    fn release_after_guard(&self, upload: Id128) -> Result<(), UploadError> {
        self.original.mutate(|table| {
            table
                .entries
                .get_mut(&upload)
                .ok_or(UploadError::BodyUnavailable)?
                .input_mut()?
                .phase = EntryPhase::Released;
            Ok(())
        })
    }
    pub(crate) async fn release_typed(
        &self,
        client: &SocketViewClient,
        upload: Id128,
    ) -> Result<(), UploadRoutingError> {
        let _guard = self.original.typed_entry_guard(client, upload).await?;
        self.release_after_guard(upload)
            .map_err(UploadRoutingError::Business)
    }
    pub(super) async fn lease_body(
        &self,
        client: &SocketViewClient,
        body: Id128,
        target: EntityId,
        purpose: UploadPurpose,
    ) -> Result<NodePreparedBodyLease, UploadError> {
        let _guard = self.original.target_guard(client, target).await?;
        self.original.mutate(|table| {
            let entry = table.entries.values().filter_map(|e|e.input().ok()).find(|entry| matches!(&entry.phase, EntryPhase::Ready { body: actual, .. } if *actual == body)).ok_or(UploadError::BodyUnavailable)?;
            if entry.target != target || entry.purpose != purpose { return Err(UploadError::InvalidOwner); }
            let EntryPhase::Ready { prompt, .. } = &entry.phase else { return Err(UploadError::BodyUnavailable); };
            let lease = prompt.acquire_use().map_err(preparation_error)?;
            Ok(NodePreparedBodyLease { lease })
        })
    }
    pub(crate) fn seal(&self) {
        self.original.seal();
    }
    #[cfg(test)]
    pub(crate) fn observation(&self) -> UploadObservation {
        let table = self
            .original
            .table
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        UploadObservation {
            sealed: table.sealed,
            entries: table.entries.len(),
            receiving: table
                .entries
                .values()
                .filter(|e| {
                    e.input()
                        .is_ok_and(|e| matches!(e.phase, EntryPhase::Receiving { .. }))
                })
                .count(),
            preparing: table
                .entries
                .values()
                .filter(|e| {
                    e.input()
                        .is_ok_and(|e| matches!(e.phase, EntryPhase::Preparing(_)))
                })
                .count(),
            ready: table
                .entries
                .values()
                .filter(|e| {
                    e.input()
                        .is_ok_and(|e| matches!(e.phase, EntryPhase::Ready { .. }))
                })
                .count(),
        }
    }
}
pub(super) struct NodePreparedBodyLease {
    lease: PreparedPromptLease,
}
impl NodePreparedBodyLease {
    pub(super) fn shared_validated(
        &self,
    ) -> crate::application::queue_owner::ValidatedBody<crate::adapters::acp::PromptDocument> {
        let original = self.validated();
        crate::application::queue_owner::ValidatedBody::new(
            original.body().clone(),
            *original.digest(),
        )
    }
    pub(super) fn validated(
        &self,
    ) -> &crate::application::queue_owner::ValidatedBody<crate::adapters::acp::PromptDocument> {
        self.lease.validated()
    }
}
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub struct UploadObservation {
    pub(crate) sealed: bool,
    pub(crate) entries: usize,
    pub(crate) receiving: usize,
    pub(crate) preparing: usize,
    pub(crate) ready: usize,
}

const fn catalog_error(e: CatalogBufferError) -> UploadError {
    match e {
        CatalogBufferError::InvalidState => UploadError::InvalidState,
        CatalogBufferError::Conflict => UploadError::Conflict,
        CatalogBufferError::BodyUnavailable => UploadError::BodyUnavailable,
        CatalogBufferError::Allocation => UploadError::Allocation,
        CatalogBufferError::NodeStopping => UploadError::NodeStopping,
        CatalogBufferError::StorageUnavailable => UploadError::StorageUnavailable,
        CatalogBufferError::InvalidDocument => UploadError::InvalidBody,
    }
}
impl UploadOwner {
    async fn catalog_guard(
        &self,
        c: &SocketViewClient,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, UploadError> {
        self.original.current(c).await?;
        let (_, g) = self
            .original
            .socket_authority
            .current_actor_guard(c)
            .await
            .map_err(view_error)?;
        Ok(g)
    }
    async fn catalog_guard_typed(
        &self,
        c: &SocketViewClient,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, UploadRoutingError> {
        self.original.current_typed(c).await?;
        let (_, g) = self
            .original
            .socket_authority
            .current_actor_guard(c)
            .await
            .map_err(view_routing_error)?;
        Ok(g)
    }
    pub(super) async fn catalog_begin(
        &self,
        c: &SocketViewClient,
    ) -> Result<UploadStarted, UploadError> {
        let _guard = self.catalog_guard(c).await?;
        self.original.mutate(|table| {
            table
                .entries
                .try_reserve(1)
                .map_err(|_| UploadError::Allocation)?;
            let id = loop {
                let id = random_id()?;
                if !table.entries.contains_key(&id) {
                    break id;
                }
            };
            let root = self
                .original
                .catalog
                .reserve_buffer()
                .map_err(catalog_error)?;
            table.entries.insert(id, Entry::Catalog(root));
            Ok(UploadStarted { upload: id })
        })
    }
    pub(super) async fn catalog_begin_typed(
        &self,
        c: &SocketViewClient,
    ) -> Result<UploadStarted, UploadRoutingError> {
        let _guard = self.catalog_guard_typed(c).await?;
        self.original
            .mutate(|table| {
                table
                    .entries
                    .try_reserve(1)
                    .map_err(|_| UploadError::Allocation)?;
                let id = loop {
                    let id = random_id()?;
                    if !table.entries.contains_key(&id) {
                        break id;
                    }
                };
                let root = self
                    .original
                    .catalog
                    .reserve_buffer()
                    .map_err(catalog_error)?;
                table.entries.insert(id, Entry::Catalog(root));
                Ok(UploadStarted { upload: id })
            })
            .map_err(UploadRoutingError::Business)
    }
    pub(super) async fn catalog_data(
        &self,
        c: &SocketViewClient,
        data: InputData<'_>,
    ) -> Result<InputAck, UploadError> {
        let guard = self.catalog_guard(c).await?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(data.payload().len())
            .map_err(|_| UploadError::Allocation)?;
        bytes.extend_from_slice(data.payload());
        let (slot, receipt, launch) = self.original.mutate(|table| {
            let slot = table
                .entries
                .get(&data.upload())
                .ok_or(UploadError::BodyUnavailable)?
                .catalog()?
                .clone();
            let (receipt, work) = slot.append(data.offset(), bytes).map_err(catalog_error)?;
            Ok((slot, receipt, work))
        })?;
        drop(guard);
        if let Some(work) = launch {
            work.start();
        }
        let result = receipt.wait().await;
        let _guard = self.catalog_guard(c).await?;
        self.original.mutate(|table| {
            let original = table
                .entries
                .get(&data.upload())
                .ok_or(UploadError::BodyUnavailable)?
                .catalog()?;
            if !slot.same(original)
                || matches!(
                    original.status().phase,
                    CatalogBufferPhase::Released | CatalogBufferPhase::Claimed
                )
            {
                return Err(UploadError::BodyUnavailable);
            }
            match result {
                CatalogJobResult::Append(result) => result
                    .map(|next| InputAck::received(data.upload(), next))
                    .map_err(catalog_error),
                CatalogJobResult::WorkerUnconfirmed(_) => Err(UploadError::StorageUnavailable),
                _ => Err(UploadError::StorageUnavailable),
            }
        })
    }
    pub(super) async fn catalog_data_typed(
        &self,
        c: &SocketViewClient,
        data: InputData<'_>,
    ) -> Result<InputAck, UploadRoutingError> {
        let guard = self.catalog_guard_typed(c).await?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(data.payload().len())
            .map_err(|_| UploadRoutingError::Business(UploadError::Allocation))?;
        bytes.extend_from_slice(data.payload());
        let (slot, receipt, launch) = self
            .original
            .mutate(|table| {
                let slot = table
                    .entries
                    .get(&data.upload())
                    .ok_or(UploadError::BodyUnavailable)?
                    .catalog()?
                    .clone();
                let (receipt, work) = slot.append(data.offset(), bytes).map_err(catalog_error)?;
                Ok((slot, receipt, work))
            })
            .map_err(UploadRoutingError::Business)?;
        drop(guard);
        if let Some(work) = launch {
            work.start();
        }
        let result = receipt.wait().await;
        let _guard = self.catalog_guard_typed(c).await?;
        self.original
            .mutate(|table| {
                let original = table
                    .entries
                    .get(&data.upload())
                    .ok_or(UploadError::BodyUnavailable)?
                    .catalog()?;
                if !slot.same(original)
                    || matches!(
                        original.status().phase,
                        CatalogBufferPhase::Released | CatalogBufferPhase::Claimed
                    )
                {
                    return Err(UploadError::BodyUnavailable);
                }
                match result {
                    CatalogJobResult::Append(result) => result
                        .map(|next| InputAck::received(data.upload(), next))
                        .map_err(catalog_error),
                    CatalogJobResult::WorkerUnconfirmed(_) => Err(UploadError::StorageUnavailable),
                    _ => Err(UploadError::StorageUnavailable),
                }
            })
            .map_err(UploadRoutingError::Business)
    }
    pub(super) async fn catalog_finish(
        &self,
        c: &SocketViewClient,
        u: Id128,
        total: u64,
        prepare: impl FnOnce(
            Vec<u8>,
            tokio::runtime::Handle,
        ) -> Result<
            crate::domain::native_catalog::CatalogDraft,
            CatalogPreparationFailure,
        > + Send
        + 'static,
    ) -> Result<UploadPreparing, UploadError> {
        let guard = self.catalog_guard(c).await?;
        let work = self.original.mutate(|table| {
            table
                .entries
                .get(&u)
                .ok_or(UploadError::BodyUnavailable)?
                .catalog()?
                .prepare(total, prepare)
                .map_err(catalog_error)
        })?;
        drop(guard);
        work.start();
        Ok(UploadPreparing { upload: u })
    }
    pub(super) async fn catalog_finish_typed(
        &self,
        c: &SocketViewClient,
        u: Id128,
        total: u64,
        prepare: impl FnOnce(
            Vec<u8>,
            tokio::runtime::Handle,
        ) -> Result<
            crate::domain::native_catalog::CatalogDraft,
            CatalogPreparationFailure,
        > + Send
        + 'static,
    ) -> Result<UploadPreparing, UploadRoutingError> {
        let guard = self.catalog_guard_typed(c).await?;
        let work = self
            .original
            .mutate(|table| {
                table
                    .entries
                    .get(&u)
                    .ok_or(UploadError::BodyUnavailable)?
                    .catalog()?
                    .prepare(total, prepare)
                    .map_err(catalog_error)
            })
            .map_err(UploadRoutingError::Business)?;
        drop(guard);
        work.start();
        Ok(UploadPreparing { upload: u })
    }
    pub(super) async fn catalog_status(
        &self,
        c: &SocketViewClient,
        u: Id128,
    ) -> Result<CatalogBufferStatus, UploadError> {
        let _guard = self.catalog_guard(c).await?;
        self.original.mutate(|table| {
            let status = table
                .entries
                .get(&u)
                .ok_or(UploadError::BodyUnavailable)?
                .catalog()?
                .status();
            if matches!(
                status.phase,
                CatalogBufferPhase::Claimed | CatalogBufferPhase::Released
            ) {
                return Err(UploadError::BodyUnavailable);
            }
            Ok(status)
        })
    }
    pub(super) async fn catalog_status_typed(
        &self,
        c: &SocketViewClient,
        u: Id128,
    ) -> Result<CatalogBufferStatus, UploadRoutingError> {
        let _guard = self.catalog_guard_typed(c).await?;
        self.original
            .mutate(|table| {
                let status = table
                    .entries
                    .get(&u)
                    .ok_or(UploadError::BodyUnavailable)?
                    .catalog()?
                    .status();
                if matches!(
                    status.phase,
                    CatalogBufferPhase::Claimed | CatalogBufferPhase::Released
                ) {
                    return Err(UploadError::BodyUnavailable);
                }
                Ok(status)
            })
            .map_err(UploadRoutingError::Business)
    }
    pub(super) async fn catalog_release(
        &self,
        c: &SocketViewClient,
        u: Id128,
    ) -> Result<(), UploadError> {
        let _guard = self.catalog_guard(c).await?;
        self.original.mutate(|table| {
            table
                .entries
                .get(&u)
                .ok_or(UploadError::BodyUnavailable)?
                .catalog()?
                .retire();
            Ok(())
        })
    }
    pub(super) async fn catalog_release_typed(
        &self,
        c: &SocketViewClient,
        u: Id128,
    ) -> Result<(), UploadRoutingError> {
        let _guard = self.catalog_guard_typed(c).await?;
        self.original
            .mutate(|table| {
                table
                    .entries
                    .get(&u)
                    .ok_or(UploadError::BodyUnavailable)?
                    .catalog()?
                    .retire();
                Ok(())
            })
            .map_err(UploadRoutingError::Business)
    }
    pub(super) async fn catalog_register(
        &self,
        c: &SocketViewClient,
        u: Id128,
        consume: impl FnOnce(
            crate::domain::native_catalog::CatalogDraft,
            tokio::runtime::Handle,
        ) -> Result<
            crate::domain::native_catalog::CatalogRequestId,
            crate::domain::native_catalog::CatalogFailure,
        > + Send
        + 'static,
    ) -> Result<CatalogWorkReceipt, UploadError> {
        let guard = self.catalog_guard(c).await?;
        let (receipt, work) = self.original.mutate(|table| {
            let slot = table
                .entries
                .get(&u)
                .ok_or(UploadError::BodyUnavailable)?
                .catalog()?;
            if let Some(receipt) = slot.registered() {
                return Ok((receipt, None));
            }
            let work = slot.register(consume).map_err(catalog_error)?;
            Ok((work.receipt.clone(), Some(work)))
        })?;
        drop(guard);
        if let Some(work) = work {
            work.start();
        }
        Ok(receipt)
    }
}

//! Internal dispatch retains distinct linear Local and Socket tokens.
use super::{
    CurrentViewAuthority, ReadClientBinding,
    local_transfer::{LocalTransferObservation, LocalViewStream, PreparedLocalChunk},
    send_epoch::OwnedDisplaySendView,
    socket_transfer::{PreparedSocketChunk, SocketViewStream},
    wire::{RecordKind, TransferHeader},
};
use crate::{
    claims::Id128,
    domain::{native::NativeSessionKey, shared_view::ViewError},
};
#[derive(Clone)]
pub enum ReadTransferStream {
    Local(LocalViewStream),
    Socket(SocketViewStream),
}
pub enum PreparedTransferChunk {
    Local(PreparedLocalChunk),
    Socket(PreparedSocketChunk),
}
impl PreparedTransferChunk {
    pub(crate) const fn header(&self) -> TransferHeader {
        match self {
            Self::Local(p) => p.header(),
            Self::Socket(p) => p.header(),
        }
    }
    pub(crate) const fn packet_sequence(&self) -> u64 {
        self.header().packet()
    }
    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
    pub(crate) fn into_local(self) -> PreparedLocalChunk {
        match self {
            Self::Local(p) => p,
            Self::Socket(_) => panic!("socket packet requires its original socket writer"),
        }
    }
    pub(crate) fn into_socket(self) -> Result<PreparedSocketChunk, ViewError> {
        match self {
            Self::Socket(p) => Ok(p),
            Self::Local(_) => Err(ViewError::InvalidClient),
        }
    }
}
macro_rules! dispatch {($self:ident,$method:ident $(,$arg:expr)*)=>{match $self{Self::Local(s)=>s.$method($($arg),*),Self::Socket(s)=>s.$method($($arg),*)}}}
impl ReadTransferStream {
    pub(crate) fn open<A: CurrentViewAuthority<C>, C>(
        view: &OwnedDisplaySendView,
        binding: ReadClientBinding,
        authority: &A,
        client: &C,
        target: &NativeSessionKey,
    ) -> Result<Self, ViewError> {
        if let Some(context) = authority.socket_context(client)? {
            if !binding.same_client(&context.binding()) {
                return Err(ViewError::InvalidClient);
            }
            Ok(Self::Socket(SocketViewStream::open(
                view.clone(),
                context,
                target.clone(),
            )?))
        } else {
            view.open(binding).map(Self::Local)
        }
    }
    pub(crate) async fn wait_transport_closed(&self) {
        match self {
            Self::Local(_) => std::future::pending().await,
            Self::Socket(stream) => stream.wait_closed().await,
        }
    }
    pub(crate) fn id(&self) -> Id128 {
        dispatch!(self, id)
    }
    pub(crate) fn observation(&self) -> Result<LocalTransferObservation, ViewError> {
        dispatch!(self, observation)
    }
    pub(crate) fn seal(&self) -> Result<(), ViewError> {
        dispatch!(self, seal)
    }
    pub(crate) fn ready(&self) -> Result<bool, ViewError> {
        dispatch!(self, ready)
    }
    pub(crate) fn explicitly_sealed(&self) -> Result<bool, ViewError> {
        dispatch!(self, explicitly_sealed)
    }
    pub(crate) fn terminally_sealed(&self) -> Result<bool, ViewError> {
        match self {
            Self::Local(s) => s.explicitly_sealed(),
            Self::Socket(s) => s.terminally_sealed(),
        }
    }
    pub(crate) fn view_current(&self) -> Result<bool, ViewError> {
        dispatch!(self, view_current)
    }
    pub(super) fn target_generation_current_in(&self, group: &super::send_epoch::Group) -> bool {
        dispatch!(self, target_generation_current_in, group)
    }
    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
    pub(crate) fn install_hooks(
        &self,
        h: std::sync::Arc<dyn super::local_transfer::LocalIoHooks>,
    ) -> Result<(), ViewError> {
        dispatch!(self, install_hooks, h)
    }
    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
    pub(crate) fn local(&self) -> LocalViewStream {
        match self {
            Self::Local(s) => s.clone(),
            Self::Socket(_) => panic!("socket read is not a local duplex endpoint"),
        }
    }
    pub(crate) fn socket(&self) -> Result<SocketViewStream, ViewError> {
        match self {
            Self::Socket(s) => Ok(s.clone()),
            Self::Local(_) => Err(ViewError::InvalidClient),
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn prepare_copy<A, C, F>(
        &self,
        a: &A,
        c: &C,
        t: &NativeSessionKey,
        record: u64,
        offset: u64,
        total: u64,
        kind: RecordKind,
        copy: F,
    ) -> Result<PreparedTransferChunk, ViewError>
    where
        A: CurrentViewAuthority<C>,
        C: Send + Sync,
        F: FnOnce() -> Result<Vec<u8>, ViewError> + Send,
    {
        self.prepare_using(a, c, t, record, offset, total, kind, |_| copy())
            .await
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn prepare_using<A, C, F>(
        &self,
        a: &A,
        c: &C,
        t: &NativeSessionKey,
        record: u64,
        offset: u64,
        total: u64,
        kind: RecordKind,
        copy: F,
    ) -> Result<PreparedTransferChunk, ViewError>
    where
        A: CurrentViewAuthority<C>,
        C: Send + Sync,
        F: FnOnce(u64) -> Result<Vec<u8>, ViewError> + Send,
    {
        match self {
            Self::Local(s) => s
                .prepare_using(a, c, t, record, offset, total, kind, copy)
                .await
                .map(PreparedTransferChunk::Local),
            Self::Socket(s) => s
                .prepare_using(a, c, t, record, offset, total, kind, copy)
                .await
                .map(PreparedTransferChunk::Socket),
        }
    }
}

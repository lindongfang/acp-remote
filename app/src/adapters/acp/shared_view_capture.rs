//! A view tap borrows the original receiver's actual event; it never owns or creates a reader.
use super::{Connection, ConnectionScope, RawMessage, Received};
use crate::{
    application::shared_views::{
        RetainedRaw,
        ingress::{IngressProbe, SourceConsumer, SourceProducer, source_ingress},
    },
    domain::shared_view::ViewError,
};

#[derive(Clone)]
pub struct RetainedAcpRaw {
    message: RawMessage,
    observation: crate::application::shared_views::read_observation::ReadObservation,
}
impl RetainedAcpRaw {
    pub(super) const fn message(&self) -> &RawMessage {
        &self.message
    }
    pub(super) fn from_received(received: &Received) -> Self {
        use super::transport::{ViewReadContextKind, ViewReadWindowKind};
        use crate::application::shared_views::{
            read_control::RpcScalar,
            read_observation::{ReadContextKind, ReadObservation, ReadWindow, ReadWindowKind},
        };
        let kind = match received.view_read_context() {
            ViewReadContextKind::OutsideReadWindow => ReadContextKind::OutsideReadWindow,
            ViewReadContextKind::UniqueReceiptWindow => ReadContextKind::UniqueReceiptWindow,
            ViewReadContextKind::AmbiguousReceiptWindow => ReadContextKind::AmbiguousReceiptWindow,
        };
        let window = received
            .view_read_window()
            .filter(|window| window.scope() == received.scope())
            .map(|window| ReadWindow {
                receipt_cell: window.cell_key(),
                session: window.session().shared_text(),
                rpc: match window.rpc() {
                    super::RpcId::Integer(id) => RpcScalar::Integer(*id),
                    super::RpcId::String(id) => RpcScalar::String(std::sync::Arc::clone(id)),
                },
                kind: match window.kind() {
                    ViewReadWindowKind::Resume => ReadWindowKind::Resume,
                    ViewReadWindowKind::Replay => ReadWindowKind::Replay,
                },
            });
        Self {
            message: received.raw().clone(),
            observation: ReadObservation {
                kind,
                window,
                session: match received.incoming() {
                    super::Incoming::Update { event, .. } => Some(event.session().shared_text()),
                    _ => None,
                },
                response: match received.incoming() {
                    super::Incoming::Response { id, .. } => Some(match id {
                        super::RpcId::Integer(id) => RpcScalar::Integer(*id),
                        super::RpcId::String(id) => RpcScalar::String(std::sync::Arc::clone(id)),
                    }),
                    _ => None,
                },
            },
        }
    }
    pub(crate) fn observation(
        &self,
    ) -> crate::application::shared_views::read_observation::ReadObservation {
        self.observation.clone()
    }
}
impl RetainedRaw for RetainedAcpRaw {
    fn len(&self) -> usize {
        self.message.bytes().len()
    }
    fn bytes(&self) -> &[u8] {
        self.message.bytes()
    }
}
pub struct AcpViewCapture {
    producer: SourceProducer<ConnectionScope, RetainedAcpRaw>,
    connection: Connection,
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    creates: Vec<(super::RpcId, crate::domain::native::NativeSourceId)>,
    seal: Option<crate::application::shared_views::source::SourceViewSeal>,
}
impl AcpViewCapture {
    pub(crate) fn ensure_external_route(
        &mut self,
        key: crate::domain::native::NativeSessionKey,
    ) -> Result<(u64, u64), ViewError> {
        self.producer.bind_read_route(key, None, None)
    }
    pub(crate) fn restore_external_route(
        &mut self,
        key: crate::domain::native::NativeSessionKey,
        restored: crate::application::shared_views::send_epoch::RestoredDisplayTarget,
    ) -> Result<(u64, u64), ViewError> {
        self.producer.restore_route(key, restored)
    }
    pub(crate) fn fail_module_read(
        &mut self,
        key: &crate::domain::native::NativeSessionKey,
        read: &super::shared_view_read::AcpReplayRead,
        error: ViewError,
        evidence: crate::application::shared_views::ingress::ReadFailureEvidence<RetainedAcpRaw>,
    ) -> Result<u64, ViewError> {
        if read.original_scope() != *self.producer.scope()
            || read.original_session().as_str() != key.native_id()
        {
            return Err(ViewError::InvalidScope);
        }
        let rpc = match read.original_rpc() {
            super::RpcId::Integer(id) => {
                crate::application::shared_views::read_control::RpcScalar::Integer(*id)
            }
            super::RpcId::String(id) => {
                crate::application::shared_views::read_control::RpcScalar::String(
                    std::sync::Arc::clone(id),
                )
            }
        };
        self.producer
            .fail_read(key, read.original_epoch(), &rpc, error, evidence)
    }
    pub(crate) fn install_source_seal(
        &mut self,
        views: &crate::application::shared_views::SharedViews<ConnectionScope, RetainedAcpRaw>,
    ) -> Result<(), ViewError> {
        if &views.core.scope != self.producer.scope() {
            return Err(ViewError::InvalidScope);
        }
        if self.seal.is_some() {
            return Err(ViewError::AlreadyAttached);
        }
        self.seal = Some(views.core.seal_handle());
        Ok(())
    }
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    pub(crate) fn install_controller(
        &mut self,
        controller: crate::application::shared_views::controller::ViewTestController<
            ConnectionScope,
        >,
    ) -> Result<(), ViewError> {
        self.producer.install_controller(controller)
    }
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    pub(crate) fn register_module_create(
        &mut self,
        reservation: &super::Reservation,
        source: crate::domain::native::NativeSourceId,
    ) -> Result<(), ViewError> {
        if reservation.scope() != *self.producer.scope() {
            return Err(ViewError::InvalidScope);
        }
        if !matches!(
            reservation.kind(),
            super::OutboundKind::Request(super::RequestKind::NewSession)
        ) {
            return Err(ViewError::InvalidStage);
        }
        let rpc = reservation.id().ok_or(ViewError::InvalidStage)?.clone();
        if self.creates.iter().any(|(id, _)| *id == rpc) {
            return Err(ViewError::AlreadyAttached);
        }
        self.creates
            .try_reserve(1)
            .map_err(|_| ViewError::AllocationFailure)?;
        self.creates.push((rpc, source));
        Ok(())
    }
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    pub(crate) fn resolve_module_created(
        &mut self,
        received: &Received,
    ) -> Result<Option<(super::RpcId, crate::domain::native::NativeSessionKey)>, ViewError> {
        if received.scope() != *self.producer.scope() {
            return Err(ViewError::InvalidScope);
        }
        let super::Incoming::Response {
            id,
            response: super::Response::NewSession(session),
            ..
        } = received.incoming()
        else {
            return Ok(None);
        };
        let Some(index) = self.creates.iter().position(|(rpc, _)| rpc == id) else {
            return Ok(None);
        };
        let source = self.creates[index].1;
        let key =
            crate::domain::native::NativeSessionKey::from_config(Some(source.0), session.as_str())
                .map_err(|_| ViewError::InvalidTarget)?;
        self.producer.bind_route(key.clone(), None)?;
        self.creates.remove(index);
        Ok(Some((id.clone(), key)))
    }
    pub(crate) fn bind_module_reservation(
        &mut self,
        reservation: &super::Reservation,
        key: crate::domain::native::NativeSessionKey,
        read: Option<crate::domain::shared_view::ReadEpoch>,
    ) -> Result<(u64, u64), ViewError> {
        if reservation.scope() != *self.producer.scope() {
            return Err(ViewError::InvalidScope);
        }
        match (reservation.kind(), read) {
            (super::OutboundKind::Request(super::RequestKind::Resume(session)), None)
            | (super::OutboundKind::Request(super::RequestKind::Replay(session)), Some(_))
                if session.as_str() == key.native_id() =>
            {
                let rpc = reservation.id().ok_or(ViewError::InvalidStage)?;
                let rpc = match rpc {
                    super::RpcId::Integer(id) => {
                        crate::application::shared_views::read_control::RpcScalar::Integer(*id)
                    }
                    super::RpcId::String(id) => {
                        crate::application::shared_views::read_control::RpcScalar::String(
                            std::sync::Arc::clone(id),
                        )
                    }
                };
                self.producer.bind_read_route(key, read, Some(rpc))
            }
            _ => Err(ViewError::InvalidTarget),
        }
    }
    pub(crate) fn complete_module_read(
        &mut self,
        key: crate::domain::native::NativeSessionKey,
        read: &super::shared_view_read::AcpReplayRead,
    ) -> Result<u64, ViewError> {
        let boundary = read.response_boundary().ok_or(ViewError::ReadFailure)?;
        if boundary.scope != *self.producer.scope() || boundary.session.as_str() != key.native_id()
        {
            return Err(ViewError::InvalidScope);
        }
        self.producer.complete_read(
            key,
            boundary.epoch,
            boundary.sequence,
            boundary.receipt_order,
            read.capture_prefix(),
        )
    }
    /// Installs a component tap for this actual connection, not a native resource grant.
    #[must_use]
    pub fn for_connection(
        connection: &Connection,
    ) -> (
        Self,
        SourceConsumer<RetainedAcpRaw>,
        IngressProbe<RetainedAcpRaw>,
    ) {
        let (producer, consumer, probe) = source_ingress(connection.scope());
        (
            Self {
                producer,
                connection: connection.clone(),
                #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
                creates: Vec::new(),
                seal: None,
            },
            consumer,
            probe,
        )
    }
    /// # Errors
    /// Returns only a display failure. The caller must not translate this into control failure.
    pub fn capture_received(&mut self, received: &Received) -> Result<(), ViewError> {
        let retained = RetainedAcpRaw::from_received(received);
        let context = retained.observation();
        self.producer.retain(
            &received.scope(),
            received.sequence(),
            received.receipt_order(),
            retained,
            context,
        )
    }
    pub fn seal(&self) {
        self.producer.seal();
        if let Some(seal) = &self.seal {
            let _ = seal.seal();
        }
    }
    /// Tracks only a real Replay reservation on this actual captured connection.
    /// This is a read fact observer, not a Native resource or dispatch grant.
    /// # Errors
    /// Rejects wrong scope, a non-Replay reservation, or a missing real typed RPC.
    pub fn track_replay(
        &self,
        reservation: &super::Reservation,
        epoch: crate::domain::shared_view::ReadEpoch,
    ) -> Result<super::shared_view_read::AcpReplayRead, ViewError> {
        super::shared_view_read::AcpReplayRead::from_reservation(
            reservation,
            self.producer.scope(),
            self.producer.probe(),
            self.connection.clone(),
            epoch,
        )
    }
}
impl Drop for AcpViewCapture {
    fn drop(&mut self) {
        self.seal();
    }
}

#[cfg(test)]
mod tests;

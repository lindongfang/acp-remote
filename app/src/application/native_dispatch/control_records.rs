//! Immutable control phases are necessary metadata, never received observations or write grants.
use super::{
    Arc, BoxFuture, DispatchError, EntityId, ResourceMetadata, RpcScalar, SourceMetadata,
    TransportFact,
};
use crate::application::request_registry::{OperationContext, RegistryDeadline};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlKind {
    NativeCancel,
    PermissionCancelled,
    Unsupported,
}
impl ControlKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NativeCancel => "native_cancel",
            Self::PermissionCancelled => "permission_cancelled",
            Self::Unsupported => "unsupported",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlIntentKind {
    Priority,
    Shutdown,
    Unsupported,
}
impl ControlIntentKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Priority => "priority",
            Self::Shutdown => "shutdown",
            Self::Unsupported => "unsupported",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlPhase {
    Reserved,
    Terminal(TransportFact),
}
impl ControlPhase {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Reserved => "reserved",
            Self::Terminal(_) => "terminal",
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlMetadata {
    pub control_id: EntityId,
    pub obligation_id: EntityId,
    pub attempt_serial: u64,
    pub requirement: EntityId,
    pub source: SourceMetadata,
    pub scope_binding: EntityId,
    pub resource: Option<ResourceMetadata>,
    pub work_serial: Option<u64>,
    pub intent: ControlIntentKind,
    pub intent_reference: EntityId,
    pub kind: ControlKind,
    pub rpc: Option<RpcScalar>,
    pub frame_length: Option<u64>,
    pub phase: ControlPhase,
    pub fingerprint: [u8; 32],
    pub at: i64,
}
pub struct ControlRecordProof {
    pub(super) metadata: ControlMetadata,
    pub(super) nonce: Arc<()>,
}
impl ControlRecordProof {
    pub(super) fn probe(&self) -> ControlRecordProbe {
        ControlRecordProbe {
            metadata: self.metadata.clone(),
            nonce: self.nonce.clone(),
        }
    }
    pub(super) fn matches(&self, probe: &ControlRecordProbe) -> bool {
        Arc::ptr_eq(&self.nonce, &probe.nonce) && self.metadata == probe.metadata
    }
    #[must_use]
    pub const fn metadata(&self) -> &ControlMetadata {
        &self.metadata
    }
}
pub struct ControlRecordProbe {
    pub(super) metadata: ControlMetadata,
    pub(super) nonce: Arc<()>,
}
impl ControlRecordProbe {
    #[must_use]
    pub const fn metadata(&self) -> &ControlMetadata {
        &self.metadata
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredControl {
    pub metadata: ControlMetadata,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlInspection {
    pub reserved: Option<StoredControl>,
    pub terminal: Option<StoredControl>,
}
pub trait NativeControlRepository: Send + Sync + 'static {
    fn record_control<'a>(
        &'a self,
        p: &'a ControlRecordProof,
        c: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredControl, DispatchError>>;
    fn inspect_control<'a>(
        &'a self,
        p: &'a ControlRecordProbe,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<ControlInspection, DispatchError>>;
}
/// Canonical metadata encoding excludes raw request, configuration and all content.
/// # Errors
/// Rejects unrepresentable field lengths and impossible control shapes.
pub fn control_fingerprint(m: &ControlMetadata) -> Result<[u8; 32], DispatchError> {
    use sha2::{Digest, Sha256};
    fn text(h: &mut Sha256, v: &str) -> Result<(), DispatchError> {
        h.update(
            u64::try_from(v.len())
                .map_err(|_| DispatchError::LengthUnrepresentable)?
                .to_be_bytes(),
        );
        h.update(v.as_bytes());
        Ok(())
    }
    let mut h = Sha256::new();
    h.update(b"native-control-v1");
    for id in [
        m.control_id,
        m.obligation_id,
        m.requirement,
        m.source.source_id,
        m.source.runtime.0,
        m.source.instance_id,
        m.source.connection_epoch,
        m.scope_binding,
        m.intent_reference,
    ] {
        text(&mut h, &id.encode())?;
    }
    h.update(m.attempt_serial.to_be_bytes());
    text(&mut h, m.intent.label())?;
    text(&mut h, m.kind.label())?;
    text(&mut h, m.phase.label())?;
    h.update(m.at.to_be_bytes());
    h.update([u8::from(m.resource.is_some())]);
    if let Some(r) = &m.resource {
        for id in [r.target, r.owner, r.load_epoch] {
            text(&mut h, &id.encode())?;
        }
    }
    for value in [m.work_serial, m.frame_length] {
        h.update([u8::from(value.is_some())]);
        if let Some(value) = value {
            h.update(value.to_be_bytes());
        }
    }
    match &m.rpc {
        None => h.update([0]),
        Some(RpcScalar::Integer(v)) => {
            h.update([1]);
            h.update(v.to_be_bytes());
        }
        Some(RpcScalar::String(v)) => {
            h.update([2]);
            text(&mut h, v)?;
        }
    }
    match m.phase {
        ControlPhase::Reserved => h.update([0]),
        ControlPhase::Terminal(TransportFact::Zero { reason }) => {
            h.update([1]);
            text(&mut h, reason.label())?;
        }
        ControlPhase::Terminal(TransportFact::Unknown { accepted, cause }) => {
            h.update([2]);
            h.update(accepted.to_be_bytes());
            text(&mut h, cause.label())?;
        }
        ControlPhase::Terminal(TransportFact::Written) => h.update([3]),
    }
    Ok(h.finalize().into())
}

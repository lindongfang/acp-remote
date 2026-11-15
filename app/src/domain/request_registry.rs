//! Body-free request identities and stable management outcomes. None are authentication grants.
use super::{EntityId, native::RuntimeEpoch};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RegistrationId(EntityId);
impl RegistrationId {
    #[must_use]
    pub const fn new(id: EntityId) -> Self {
        Self(id)
    }
    #[must_use]
    pub const fn entity(self) -> EntityId {
        self.0
    }
    #[must_use]
    pub fn encode(self) -> String {
        self.0.encode()
    }
    /// # Errors
    /// Rejects noncanonical identifiers.
    pub fn decode(value: &str) -> Result<Self, RegistryError> {
        EntityId::decode(value)
            .map(Self)
            .map_err(|_| RegistryError::InvalidIntent)
    }
}

macro_rules! labels {
    ($name:ident { $($variant:ident => $label:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum $name { $($variant),+ }
        impl $name {
            #[must_use]
            pub const fn label(self) -> &'static str { match self { $(Self::$variant => $label),+ } }
            #[must_use]
            pub fn from_label(value: &str) -> Option<Self> {
                match value { $($label => Some(Self::$variant),)+ _ => None }
            }
        }
    };
}
labels!(QueueAction {
    Submit => "input.submit", Edit => "input.edit", Cancel => "input.cancel",
    Prioritize => "input.prioritize", Resume => "input.resume",
});
labels!(QueueRejection {
    Full => "full", DuplicateId => "duplicate_id", NotFound => "not_found",
    RevisionChanged => "revision_changed", RevisionExhausted => "revision_exhausted",
    DispatchStarted => "dispatch_started", Suspended => "suspended", Busy => "busy",
    SequenceExhausted => "sequence_exhausted",
});

#[derive(Clone, Eq, PartialEq)]
pub struct RegisteredIntent {
    actor: EntityId,
    action: QueueAction,
    target: EntityId,
    item: Option<EntityId>,
    expected_revision: Option<u64>,
    body_digest: Option<[u8; 32]>,
}
impl fmt::Debug for RegisteredIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegisteredIntent")
            .field("actor", &self.actor)
            .field("action", &self.action)
            .field("target", &self.target)
            .field("item", &self.item)
            .field("expected_revision", &self.expected_revision)
            .field("has_body_binding", &self.body_digest.is_some())
            .finish()
    }
}
impl RegisteredIntent {
    /// # Errors
    /// Rejects fields outside the closed action shape.
    pub const fn new(
        actor: EntityId,
        action: QueueAction,
        target: EntityId,
        item: Option<EntityId>,
        expected_revision: Option<u64>,
        body_digest: Option<[u8; 32]>,
    ) -> Result<Self, RegistryError> {
        let valid = match action {
            QueueAction::Submit => {
                item.is_none() && expected_revision.is_none() && body_digest.is_some()
            }
            QueueAction::Edit => {
                item.is_some() && expected_revision.is_some() && body_digest.is_some()
            }
            QueueAction::Cancel | QueueAction::Prioritize => {
                item.is_some() && expected_revision.is_some() && body_digest.is_none()
            }
            QueueAction::Resume => {
                item.is_none() && expected_revision.is_none() && body_digest.is_none()
            }
        };
        if !valid {
            return Err(RegistryError::InvalidIntent);
        }
        Ok(Self {
            actor,
            action,
            target,
            item,
            expected_revision,
            body_digest,
        })
    }
    #[must_use]
    pub const fn actor(&self) -> EntityId {
        self.actor
    }
    #[must_use]
    pub const fn action(&self) -> QueueAction {
        self.action
    }
    #[must_use]
    pub const fn target(&self) -> EntityId {
        self.target
    }
    #[must_use]
    pub const fn item(&self) -> Option<EntityId> {
        self.item
    }
    #[must_use]
    pub const fn expected_revision(&self) -> Option<u64> {
        self.expected_revision
    }
    #[must_use]
    pub const fn body_digest(&self) -> Option<&[u8; 32]> {
        self.body_digest.as_ref()
    }
    /// # Errors
    /// Rejects outcomes belonging to another action, item or content revision.
    pub fn validate_outcome(
        &self,
        id: RegistrationId,
        outcome: QueueOutcome,
    ) -> Result<(), RegistryError> {
        use QueueAction as A;
        use QueueOutcome as O;
        use QueueRejection as R;
        let valid = match (self.action, outcome) {
            (A::Submit, O::Queued { item, revision } | O::DirectSelected { item, revision }) => {
                item == id.entity() && revision == 1
            }
            (A::Edit, O::Edited { item, revision }) => {
                Some(item) == self.item
                    && self.expected_revision.and_then(|n| n.checked_add(1)) == Some(revision)
            }
            (A::Cancel, O::Cancelled { item, revision })
            | (A::Prioritize, O::Prioritized { item, revision }) => {
                Some(item) == self.item && Some(revision) == self.expected_revision
            }
            (A::Resume, O::Resumed { restored }) => restored <= 3,
            (A::Submit, O::Rejected(R::Full | R::DuplicateId | R::SequenceExhausted))
            | (
                A::Edit,
                O::Rejected(
                    R::DispatchStarted | R::NotFound | R::RevisionChanged | R::RevisionExhausted,
                ),
            )
            | (A::Cancel, O::Rejected(R::DispatchStarted | R::NotFound | R::RevisionChanged))
            | (
                A::Prioritize,
                O::Rejected(R::DispatchStarted | R::NotFound | R::RevisionChanged | R::Suspended),
            ) => true,
            _ => false,
        };
        if valid {
            Ok(())
        } else {
            Err(RegistryError::InvalidIntent)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistrationFact {
    id: RegistrationId,
    intent: RegisteredIntent,
    register_auth_generation: i64,
    registered_at_ms: i64,
}
impl RegistrationFact {
    /// # Errors
    /// Rejects invalid authentication metadata. This constructor does not authorize consumption.
    pub const fn new(
        id: RegistrationId,
        intent: RegisteredIntent,
        register_auth_generation: i64,
        registered_at_ms: i64,
    ) -> Result<Self, RegistryError> {
        if register_auth_generation < 0 {
            return Err(RegistryError::InvalidIntent);
        }
        Ok(Self {
            id,
            intent,
            register_auth_generation,
            registered_at_ms,
        })
    }
    #[must_use]
    pub const fn id(&self) -> RegistrationId {
        self.id
    }
    #[must_use]
    pub const fn intent(&self) -> &RegisteredIntent {
        &self.intent
    }
    #[must_use]
    pub const fn register_auth_generation(&self) -> i64 {
        self.register_auth_generation
    }
    #[must_use]
    pub const fn registered_at_ms(&self) -> i64 {
        self.registered_at_ms
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredApplying {
    pub registration: RegistrationFact,
    pub operation: EntityId,
    pub runtime: RuntimeEpoch,
    pub admission_auth_generation: i64,
    pub begun_at_ms: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestFact {
    Registered(RegistrationFact),
    Applying(StoredApplying),
    Settled {
        registration: RegistrationFact,
        outcome: QueueOutcome,
    },
    Interrupted {
        registration: RegistrationFact,
    },
    Retired,
}
impl RequestFact {
    #[must_use]
    pub const fn registration(&self) -> Option<&RegistrationFact> {
        match self {
            Self::Registered(r)
            | Self::Settled {
                registration: r, ..
            }
            | Self::Interrupted { registration: r } => Some(r),
            Self::Applying(a) => Some(&a.registration),
            Self::Retired => None,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueOutcome {
    Queued { item: EntityId, revision: u64 },
    DirectSelected { item: EntityId, revision: u64 },
    Edited { item: EntityId, revision: u64 },
    Cancelled { item: EntityId, revision: u64 },
    Prioritized { item: EntityId, revision: u64 },
    Resumed { restored: u8 },
    Rejected(QueueRejection),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RegistryError {
    #[error("request intent is invalid")]
    InvalidIntent,
    #[error("request actor is not current")]
    InvalidActor,
    #[error("request target is not current")]
    InvalidTarget,
    #[error("request identity is not registered")]
    UnknownRegistration,
    #[error("request conflicts with its stable fact")]
    Conflict,
    #[error("request identity is retired")]
    Retired,
    #[error("node random source is unavailable")]
    RandomUnavailable,
    #[error("necessary request storage is unavailable")]
    StorageUnavailable,
    #[error("request commit could not be confirmed")]
    CommitUnconfirmed,
    #[error("stored request state is invalid")]
    CorruptState,
    #[error("request wait was cancelled")]
    Cancelled,
    #[error("this owner has no such pending operation")]
    NoOwnedOperation,
    #[error("observation belongs to an old owner or work")]
    StaleObservation,
    #[error("queue runtime is stopping")]
    NodeStopping,
    #[error("queue runtime is not ready")]
    NodeStarting,
    #[error("queue runtime already has an owner")]
    RuntimeAlreadyStarted,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(n: u8) -> EntityId {
        EntityId::from_bytes([n; 16])
    }
    #[test]
    fn intent_shape_and_result_binding_are_closed() {
        assert_eq!(
            RegisteredIntent::new(id(1), QueueAction::Resume, id(2), Some(id(3)), None, None),
            Err(RegistryError::InvalidIntent)
        );
        let submit =
            RegisteredIntent::new(id(1), QueueAction::Submit, id(2), None, None, Some([3; 32]))
                .unwrap();
        let request = RegistrationId::new(id(4));
        assert!(
            submit
                .validate_outcome(
                    request,
                    QueueOutcome::Queued {
                        item: id(4),
                        revision: 1
                    }
                )
                .is_ok()
        );
        assert!(
            submit
                .validate_outcome(
                    request,
                    QueueOutcome::Queued {
                        item: id(5),
                        revision: 1
                    }
                )
                .is_err()
        );
        assert!(
            submit
                .validate_outcome(request, QueueOutcome::Rejected(QueueRejection::Busy))
                .is_err()
        );
        let edit = RegisteredIntent::new(
            id(1),
            QueueAction::Edit,
            id(2),
            Some(id(4)),
            Some(u64::MAX),
            Some([3; 32]),
        )
        .unwrap();
        assert!(
            edit.validate_outcome(
                request,
                QueueOutcome::Edited {
                    item: id(4),
                    revision: 0
                }
            )
            .is_err()
        );
        assert!(
            edit.validate_outcome(
                request,
                QueueOutcome::Rejected(QueueRejection::RevisionExhausted)
            )
            .is_ok()
        );
        assert!(!format!("{submit:?}").contains("3, 3, 3"));
    }
}

//! Product request facts and conservative time observations, independent of signatures and SQL.
use super::{EntityId, native::RequestEpoch};
use std::time::Duration;

pub const REQUEST_VALIDITY_MS: u64 = 86_400_000;
pub const MAX_REQUEST_TIME: u64 = 9_007_199_254_740_991;

macro_rules! labels {
    ($name:ident { $($variant:ident => $label:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum $name { $($variant),+ }
        impl $name {
            #[must_use]
            pub const fn label(self) -> &'static str { match self { $(Self::$variant => $label),+ } }
            #[must_use]
            pub fn from_label(value: &str) -> Option<Self> { match value { $($label => Some(Self::$variant),)+ _ => None } }
        }
    };
}
labels!(ProductAction {
    HarnessRegister => "harness.register", WorkspaceRegister => "workspace.register",
    SessionCreate => "session.create", SessionLoad => "session.load", SessionInput => "session.input",
    SessionCancel => "session.cancel", SessionRelease => "session.release", SessionResume => "session.resume",
    ApprovalDecide => "approval.decide",
});
labels!(ProductTargetKind {
    Harness => "harness", Workspace => "workspace", Source => "source", Session => "session", Approval => "approval",
});

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProductTarget {
    pub kind: ProductTargetKind,
    pub id: EntityId,
}

/// The digest is computed from the frozen action DTO before constructing this value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestIntent {
    pub actor: EntityId,
    pub action: ProductAction,
    pub target: ProductTarget,
    pub payload_digest: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRequestIdentity {
    pub epoch: RequestEpoch,
    pub request: EntityId,
    pub intent: RequestIntent,
    pub issued_at: u64,
    pub expires_at: u64,
}

/// A signature/intent proof, never authentication or permission to dispatch.
pub struct VerifiedProductRequest(pub(crate) ProductRequestIdentity);
impl VerifiedProductRequest {
    #[must_use]
    pub const fn identity(&self) -> &ProductRequestIdentity {
        &self.0
    }
}

/// A local observation, NOT a durable clock checkpoint, authentication grant, or dispatch permit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestTime {
    now: u64,
    imported_floor: u64,
}
impl RequestTime {
    #[must_use]
    pub const fn millis(self) -> u64 {
        self.now
    }
    #[must_use]
    pub const fn candidate_watermark(self) -> u64 {
        self.now
    }
    #[must_use]
    pub const fn imported_watermark(self) -> u64 {
        self.imported_floor
    }
}

/// Caller samples afresh at each issue/admission/dispatch boundary. Only SQL may confirm a floor.
pub struct RequestClock {
    anchor_wall: u64,
    last_wall: u64,
    last_elapsed: Duration,
    candidate: u64,
    imported_floor: u64,
    unreliable: bool,
}
impl RequestClock {
    /// # Errors
    /// A restarted clock below its imported durable floor cannot issue dangerous qualifications.
    pub const fn new(imported_floor: u64, startup_wall: u64) -> Result<Self, RequestIdentityError> {
        if startup_wall > MAX_REQUEST_TIME || imported_floor > startup_wall {
            return Err(RequestIdentityError::ClockUnreliable);
        }
        Ok(Self {
            anchor_wall: startup_wall,
            last_wall: startup_wall,
            last_elapsed: Duration::ZERO,
            candidate: startup_wall,
            imported_floor,
            unreliable: false,
        })
    }

    /// # Errors
    /// Missing, regressing or overflowing time permanently faults this running clock instance.
    pub fn observe(
        &mut self,
        wall: Option<u64>,
        elapsed: Option<Duration>,
    ) -> Result<RequestTime, RequestIdentityError> {
        if self.unreliable {
            return Err(RequestIdentityError::ClockUnreliable);
        }
        let result = self.observe_checked(wall, elapsed);
        if result.is_err() {
            self.unreliable = true;
        }
        result
    }

    fn observe_checked(
        &mut self,
        wall: Option<u64>,
        elapsed: Option<Duration>,
    ) -> Result<RequestTime, RequestIdentityError> {
        let wall = wall.ok_or(RequestIdentityError::ClockUnreliable)?;
        let elapsed = elapsed.ok_or(RequestIdentityError::ClockUnreliable)?;
        if wall < self.last_wall || wall > MAX_REQUEST_TIME || elapsed < self.last_elapsed {
            return Err(RequestIdentityError::ClockUnreliable);
        }
        let elapsed_ms = u64::try_from(elapsed.as_millis())
            .map_err(|_| RequestIdentityError::ClockUnreliable)?;
        let monotonic_wall = self
            .anchor_wall
            .checked_add(elapsed_ms)
            .filter(|value| *value <= MAX_REQUEST_TIME)
            .ok_or(RequestIdentityError::ClockUnreliable)?;
        self.last_wall = wall;
        self.last_elapsed = elapsed;
        self.candidate = self.candidate.max(wall).max(monotonic_wall);
        Ok(RequestTime {
            now: self.candidate,
            imported_floor: self.imported_floor,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RequestIdentityError {
    #[error("product request signature or purpose is invalid")]
    InvalidSignature,
    #[error("product request claims are invalid")]
    InvalidClaims,
    #[error("product request does not match this operation or actor")]
    IntentMismatch,
    #[error("product request belongs to another node request epoch")]
    EpochMismatch,
    #[error("product request has expired")]
    Expired,
    #[error("product request time cannot be reliably verified")]
    ClockUnreliable,
}

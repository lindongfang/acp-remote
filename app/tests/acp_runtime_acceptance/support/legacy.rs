//! Test-only call-site bridge. Every incoming is a real reader-created Received.
//! No fake brand, product compatibility overload, private-field mutation or Value DOM in product.
#![allow(dead_code)]
#[path = "mod.rs"]
mod fresh;
use acp_remote::adapters::acp::*;
pub use fresh::{raw_value, setup};
use serde_json::Value;
use std::sync::Arc;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    time::Instant,
};
pub struct LegacyId(pub RpcId);
impl LegacyId {
    pub fn json(&self) -> Value {
        fresh::rpc_value(&self.0)
    }
}
pub struct TestClient {
    pub connection: Connection,
    pub receiver: EventReceiver,
    pub owner: IoOwner,
    selected: Option<Strategy>,
}
impl TestClient {
    pub fn new<R, W>(r: R, w: W) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (connection, receiver, owner) = Connection::spawn(r, w).unwrap();
        Self {
            connection,
            receiver,
            owner,
            selected: None,
        }
    }
    pub const fn strategy(&self) -> Option<&Strategy> {
        self.selected.as_ref()
    }
    pub fn is_closed(&self) -> bool {
        self.connection.is_closed()
    }
    pub fn pending_count(&self) -> usize {
        self.connection.pending_count()
    }
    pub fn take_rejected_message(&self) -> Option<RawMessage> {
        self.connection.take_rejected_message()
    }
    async fn submit(&self, r: Reservation, deadline: Instant) -> Result<LegacyId, AcpError> {
        let id = r.id().cloned().ok_or(AcpError::InvalidMessage)?;
        let f = r.encode()?;
        let t = self
            .connection
            .enqueue(r, f, Arc::new(fresh::Fence), deadline)
            .map_err(|e| e.error())?;
        match t.wait().await {
            WriteReport::Written => Ok(LegacyId(id)),
            _ => Err(AcpError::Closed),
        }
    }
    pub async fn initialize(&self, v: Version, deadline: Instant) -> Result<LegacyId, AcpError> {
        self.submit(self.connection.reserve_initialize(v, deadline)?, deadline)
            .await
    }
    pub async fn initialize_preferred(&self, deadline: Instant) -> Result<LegacyId, AcpError> {
        self.submit(
            self.connection.reserve_preferred_initialize(deadline)?,
            deadline,
        )
        .await
    }
    pub async fn request(&self, r: Request, deadline: Instant) -> Result<LegacyId, AcpError> {
        self.submit(
            self.connection
                .reserve_request(r, ResponseDeadline::At(deadline))?,
            deadline,
        )
        .await
    }
    pub async fn next(&mut self, _deadline: Instant) -> Result<Received, AcpError> {
        let result = self.receiver.next().await?.ok_or(AcpError::Closed)?;
        self.selected = self.connection.strategy();
        Ok(result)
    }
    pub async fn permission_response(
        &self,
        p: &PermissionRequest,
        choice: Option<&str>,
        deadline: Instant,
    ) -> Result<(), AcpError> {
        let selection = match choice {
            Some(s) => PermissionSelection::Selected(OptionId::parse(
                raw_value(&serde_json::json!(s)).as_ref(),
            )?),
            None => PermissionSelection::Cancelled,
        };
        let r = self.connection.reserve_permission_response(p, selection)?;
        let f = r.encode()?;
        let t = self
            .connection
            .enqueue(r, f, Arc::new(fresh::Fence), deadline)
            .map_err(|e| e.error())?;
        match t.wait().await {
            WriteReport::Written => Ok(()),
            _ => Err(AcpError::Closed),
        }
    }
}
pub trait LegacySetup {
    fn legacy_replay(&self, s: SessionId, cwd: &str) -> Result<Request, AcpError>;
    fn legacy_resume(&self, s: SessionId, cwd: &str) -> Result<Request, AcpError>;
}
impl LegacySetup for Strategy {
    fn legacy_replay(&self, s: SessionId, cwd: &str) -> Result<Request, AcpError> {
        self.replay(s, &setup(self.version(), cwd))
    }
    fn legacy_resume(&self, s: SessionId, cwd: &str) -> Result<Request, AcpError> {
        self.resume(s, &setup(self.version(), cwd))
    }
}

//! Protocol-neutral receipt context. This data grants neither source nor RPC authority.
use super::read_control::RpcScalar;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadContextKind {
    Unavailable,
    OutsideReadWindow,
    UniqueReceiptWindow,
    AmbiguousReceiptWindow,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadWindowKind {
    Resume,
    Replay,
}
#[derive(Clone)]
pub struct ReadWindow {
    pub(crate) rpc: RpcScalar,
    pub(crate) session: Arc<str>,
    pub(crate) kind: ReadWindowKind,
    pub(crate) receipt_cell: u64,
}
#[derive(Clone)]
pub struct ReadObservation {
    pub(crate) kind: ReadContextKind,
    pub(crate) window: Option<ReadWindow>,
    pub(crate) session: Option<Arc<str>>,
    pub(crate) response: Option<RpcScalar>,
}
impl ReadObservation {
    pub(crate) const fn unavailable() -> Self {
        Self {
            kind: ReadContextKind::Unavailable,
            window: None,
            session: None,
            response: None,
        }
    }
}

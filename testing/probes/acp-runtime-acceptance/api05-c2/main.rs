use acp_remote::adapters::acp::tool_projection::ToolProjectionRead;
use acp_remote::adapters::acp::view_observations::ObservationRead;
use acp_remote::adapters::acp::{Connection, EventReceiver, IoOwner, WriteTicket};
use acp_remote::adapters::acp::{Incoming, Received, SessionEvent};
use acp_remote::domain::views::ViewWindows;
use std::sync::Arc;
#[allow(dead_code)]
fn all_public_futures_send(
    c: &Connection,
    e: &mut EventReceiver,
    t: &WriteTicket,
    o: &IoOwner,
    at: tokio::time::Instant,
) {
    fn send<T: Send>(_: T) {}
    let _ = c.scope();
    send(e.next());
    send(t.wait());
    send(t.cancel_and_wait());
    send(o.shutdown(at));
}
#[allow(dead_code)]
fn witness(
    received: &Received,
    windows: &mut ViewWindows,
    read: &mut ObservationRead,
    tools: &mut ToolProjectionRead,
) {
    let _: &Incoming = received.incoming();
    let _ = received.scope();
    let _ = received.sequence();
    let _ = received.receipt_order();
    let _ = received.raw();
    let mut altered = received.incoming().clone();
    if let Incoming::Update {
        event: SessionEvent::Other { kind, .. },
        ..
    } = &mut altered
    {
        *kind = Arc::from("synthetic-forged-kind");
    }
    let _: &Incoming = &altered; // Ordinary data can be edited; it never becomes a source grant.
    let _ = read.observe(windows, received);
    let _ = tools.apply(windows, received);
    // SINGLE NEGATIVE OPERATION: API05-C2
    let _forged: Received = received.incoming().clone().into();

}
fn main() {}

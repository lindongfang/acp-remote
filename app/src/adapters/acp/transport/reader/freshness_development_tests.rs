//! Developer regression: real observed transport and controlled duplex peer, not a Harness.
mod context_tests;
use crate::adapters::acp::{
    AcpError, Connection, Incoming, IoOwner, ObservationAcknowledger, ObservationLease,
    ObservationReceiver, ResponseDeadline, RpcId, Version, WriteReport,
    test_support::{Allow, RpcJson, parse_session, setup},
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream},
    time::{Instant, timeout_at},
};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

struct Fixture {
    connection: Connection,
    receiver: ObservationReceiver,
    ack: ObservationAcknowledger,
    owner: IoOwner,
    peer: DuplexStream,
}
impl Fixture {
    async fn new(wire: Version) -> Self {
        let (local, peer) = tokio::io::duplex(65536);
        let (read, write) = tokio::io::split(local);
        let mut owner = None;
        let (start, receiver, ack, _) = Connection::prepare_observed(read, write, |io| {
            owner = Some(io);
            Ok(())
        })
        .expect("prepare observed");
        let mut fixture = Self {
            connection: start.start(),
            receiver,
            ack,
            owner: owner.expect("adopted owner"),
            peer,
        };
        let reservation = fixture
            .connection
            .reserve_initialize(wire, deadline())
            .expect("reserve initialize");
        let id = fixture.send(reservation).await;
        let result = match wire {
            Version::V1 => {
                json!({"protocolVersion":1,"agentInfo":{"name":"fixture","version":"fixed"},"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"resume":{}}}})
            }
            Version::V2 => {
                json!({"protocolVersion":2,"info":{"name":"fixture","version":"fixed"},"capabilities":{"session":{}}})
            }
        };
        fixture
            .feed(&[json!({"jsonrpc":"2.0","id":id.json(),"result":result})])
            .await;
        let lease = fixture.next().await.expect("initialize lease");
        fixture.ack.acknowledge(lease).expect("ack initialize");
        fixture
    }

    async fn send(&mut self, reservation: crate::adapters::acp::Reservation) -> RpcId {
        let id = reservation.id().expect("real RPC").clone();
        let frame = reservation.encode().expect("encode");
        let ticket = self
            .connection
            .enqueue(reservation, frame, Arc::new(Allow), deadline())
            .expect("enqueue");
        assert_eq!(
            timeout_at(deadline(), ticket.wait())
                .await
                .expect("written"),
            WriteReport::Written
        );
        let mut line = String::new();
        timeout_at(
            deadline(),
            BufReader::new(&mut self.peer).read_line(&mut line),
        )
        .await
        .expect("peer timeout")
        .expect("peer request");
        assert!(!line.is_empty());
        id
    }

    async fn read_request(&mut self, session: &str, replay: bool) -> RpcId {
        let strategy = self.connection.strategy().expect("strategy");
        let session = parse_session(&json!(session)).expect("session");
        let setup = setup(strategy.version(), "C:/fixture");
        let request = if replay {
            strategy.replay(session, &setup)
        } else {
            strategy.resume(session, &setup)
        }
        .expect("standard read");
        let reservation = self
            .connection
            .reserve_request(request, ResponseDeadline::Unbounded)
            .expect("reserve read");
        self.send(reservation).await
    }

    async fn feed(&mut self, frames: &[Value]) {
        let mut bytes = Vec::new();
        for frame in frames {
            serde_json::to_writer(&mut bytes, frame).expect("fixture JSON");
            bytes.push(b'\n');
        }
        timeout_at(deadline(), self.peer.write_all(&bytes))
            .await
            .expect("feed timeout")
            .expect("feed");
    }

    async fn next(&mut self) -> Result<ObservationLease, AcpError> {
        timeout_at(deadline(), self.receiver.next())
            .await
            .expect("bounded receive")?
            .ok_or(AcpError::Closed)
    }

    async fn stop(self) {
        assert!(self.owner.shutdown(deadline()).await.joined);
    }
}

fn update(session: &str, kind: &str) -> Value {
    json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":session,"update":{"sessionUpdate":kind,"state":"idle","background":[]}}})
}

#[tokio::test]
async fn freshness_replay_history_without_complete_cannot_satisfy_current_snapshot() {
    assert_missing_current(false).await;
}

#[tokio::test]
async fn freshness_replay_history_then_complete_requires_a_new_current_snapshot() {
    assert_missing_current(true).await;
}

async fn assert_missing_current(marker: bool) {
    let mut fixture = Fixture::new(Version::V2).await;
    let id = fixture.read_request("s", true).await;
    let mut tool = update("s", "tool_call_update");
    tool["params"]["update"]["toolCallId"] = json!("tool");
    tool["params"]["update"]["status"] = json!("in_progress");
    let mut frames = if marker {
        vec![update("s", "state_update"), update("s", "replay_complete")]
    } else {
        vec![tool]
    };
    frames.push(json!({"jsonrpc":"2.0","id":id.json(),"result":{}}));
    fixture.feed(&frames).await;
    for _ in 0..=usize::from(marker) {
        let lease = fixture.next().await.expect("update lease");
        assert!(matches!(
            lease.received().incoming(),
            Incoming::Update { .. }
        ));
        fixture.ack.acknowledge(lease).expect("ack update");
    }
    let result = fixture.next().await.map(|lease| {
        fixture
            .ack
            .acknowledge(lease)
            .expect("ack unexpected response");
    });
    fixture.stop().await;
    assert_eq!(result, Err(AcpError::MissingSnapshot));
}

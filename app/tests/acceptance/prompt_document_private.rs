//! Independent PA06/07. Existing crate visibility, real SQLite and R3; no Node authority claim.
use crate::{
    adapters::acp::{PromptDocument, native_dispatch::AcpNativeAdapter, raw_json::OwnedRawValue},
    application::{
        queue_owner::{
            ConsumeReply, NativeQueueDriver, QueueEnvironmentPort, QueueOwnerEvent, QueueRead,
            QueueService, RetainPrompt,
        },
        request_registry::{
            AuditFailure, AuditMetadata, AuditPort, QueueAction, QueueOutcome, RegistrationId,
            RegistryDeadline, RegistryError, RequestAuthority, RequestFact, SystemNodeClock,
            ValidatedBody, ValidatedMutation,
        },
    },
    domain::{
        ActorContext, EntityId,
        input_queue::DispatchReadiness,
        native::{
            HarnessConnectionEpoch, LoadEpoch, NativeSessionKey, ResourceEpoch, RuntimeEpoch,
        },
    },
    session_storage::request_registry::SqliteRequestRegistry,
    storage::{NodeMeta, SCHEMA_VERSION, Storage},
};
use futures_util::FutureExt;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::{
    future::Future,
    panic::AssertUnwindSafe,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time::Instant;

const TARGET: EntityId = EntityId::from_bytes([40; 16]);
const ACTOR: EntityId = EntityId::from_bytes([10; 16]);

struct TestAuthority(SqlitePool);
impl RequestAuthority for TestAuthority {
    type Actor = EntityId;
    async fn authorize(
        &self,
        actor: &EntityId,
        _: EntityId,
        _: QueueAction,
    ) -> Result<ActorContext, RegistryError> {
        let trusted: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM clients WHERE client_id=? AND status='trusted'",
        )
        .bind(actor.encode())
        .fetch_one(&self.0)
        .await?;
        if trusted != 1 {
            return Err(RegistryError::InvalidActor);
        }
        Ok(ActorContext {
            client_id: *actor,
            auth_generation: 1,
        })
    }
    async fn authorize_read(
        &self,
        actor: &EntityId,
        target: EntityId,
    ) -> Result<ActorContext, RegistryError> {
        self.authorize(actor, target, QueueAction::Submit).await
    }
    async fn resolve_target(&self, target: EntityId) -> Result<NativeSessionKey, RegistryError> {
        let pair: Option<(String, String)> =
            sqlx::query_as("SELECT source_id,native_id FROM product_targets WHERE target_id=?")
                .bind(target.encode())
                .fetch_optional(&self.0)
                .await?;
        let (source, native) = pair.ok_or(RegistryError::InvalidTarget)?;
        NativeSessionKey::from_config(
            Some(EntityId::decode(&source).map_err(|_| RegistryError::InvalidTarget)?),
            &native,
        )
        .map_err(|_| RegistryError::InvalidTarget)
    }
}
struct Environment(Mutex<DispatchReadiness>);
fn epoch() -> ResourceEpoch {
    ResourceEpoch {
        runtime: RuntimeEpoch(EntityId::from_bytes([101; 16])),
        connection: HarnessConnectionEpoch(EntityId::from_bytes([102; 16])),
        load: LoadEpoch(EntityId::from_bytes([103; 16])),
    }
}
impl QueueEnvironmentPort for Environment {
    fn readiness(&self, _: &NativeSessionKey) -> DispatchReadiness {
        *self.0.lock().unwrap()
    }
    fn resource_epoch(&self, _: &NativeSessionKey) -> Option<ResourceEpoch> {
        Some(epoch())
    }
    fn poll_event(&self, _: &NativeSessionKey) -> Option<QueueOwnerEvent> {
        None
    }
}
struct Audit;
impl AuditPort for Audit {
    fn append<'a>(
        &'a self,
        _: &'a AuditMetadata,
    ) -> Pin<Box<dyn Future<Output = Result<(), AuditFailure>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}
type Service =
    QueueService<TestAuthority, Arc<AcpNativeAdapter>, SqliteRequestRegistry, SystemNodeClock>;
struct Area {
    path: PathBuf,
    storage: Storage,
    service: Service,
    driver: NativeQueueDriver<PromptDocument>,
    adapter: Arc<AcpNativeAdapter>,
    environment: Arc<Environment>,
}
impl Area {
    async fn new() -> Self {
        // The runner creates a new owned root. Never default to a user's node data path.
        let parent = std::env::var_os("PA_DATA_ROOT")
            .filter(|v| !v.is_empty())
            .map_or_else(
                || {
                    let dir =
                        std::env::temp_dir().join(format!("acpr-pa-data-{}", std::process::id()));
                    std::fs::create_dir_all(&dir).unwrap();
                    dir
                },
                PathBuf::from,
            );
        assert!(parent.is_absolute() && parent.is_dir());
        let path = parent.join(format!(
            "pa-{}-{}",
            std::process::id(),
            crate::random::hex(10).unwrap()
        ));
        std::fs::create_dir(&path).unwrap();
        let storage = Storage::open(&path.join("node.sqlite3")).await.unwrap();
        storage
            .write_node_meta(&NodeMeta {
                schema_version: SCHEMA_VERSION,
                node_id: "independent-PA-synthetic".into(),
                public_key: vec![1, 2, 3],
                key_file_ref: "synthetic-unused.key".into(),
            })
            .await
            .unwrap();
        sqlx::query("INSERT INTO clients VALUES(?,?,?,'trusted',1,0,NULL)")
            .bind(ACTOR.encode())
            .bind(vec![10_u8])
            .bind("synthetic-PA-actor")
            .execute(storage.pool_for_test())
            .await
            .unwrap();
        sqlx::query("INSERT INTO product_sources VALUES(?,'independent synthetic')")
            .bind(EntityId::from_bytes([1; 16]).encode())
            .execute(storage.pool_for_test())
            .await
            .unwrap();
        sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
            .bind(TARGET.encode())
            .bind(EntityId::from_bytes([1; 16]).encode())
            .bind("independent-native")
            .execute(storage.pool_for_test())
            .await
            .unwrap();
        let adapter = Arc::new(AcpNativeAdapter::default());
        let environment = Arc::new(Environment(Mutex::new(DispatchReadiness::Blocked)));
        let (service, driver) = storage
            .start_queue_service_with_audit_for_fixture(
                TestAuthority(storage.pool_for_test().clone()),
                adapter.clone(),
                SystemNodeClock,
                environment.clone(),
                Arc::new(Audit),
            )
            .await
            .unwrap();
        Self {
            path,
            storage,
            service,
            driver,
            adapter,
            environment,
        }
    }
    fn document(bytes: &[u8]) -> PromptDocument {
        PromptDocument::parse(bytes.to_vec()).expect("complete synthetic document")
    }
    fn body(&self, bytes: &[u8]) -> ValidatedBody<PromptDocument> {
        self.service
            .validate_body(&Self::document(bytes))
            .expect("actual adapter validator")
    }
    async fn act(
        &self,
        mutation: ValidatedMutation<PromptDocument>,
    ) -> (RegistrationId, QueueOutcome) {
        let registration = self
            .service
            .register(&ACTOR, TARGET, &mutation)
            .await
            .unwrap();
        let reply = self
            .service
            .consume(&ACTOR, TARGET, registration.id(), mutation)
            .await;
        match reply {
            ConsumeReply::Resolved {
                fact: RequestFact::Settled { outcome, .. },
                returned: None,
            } => (registration.id(), outcome),
            other => panic!("expected real settled mutation, got {other:?}"),
        }
    }
    async fn waiting(&self) -> Vec<(EntityId, u64, Vec<u8>, [u8; 32])> {
        self.service
            .inspect_queue(&ACTOR, TARGET, |view| {
                let q = match view {
                    QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => queue,
                };
                q.waiting()
                    .map(|entry| {
                        (
                            entry.id(),
                            entry.revision(),
                            entry.input().body.body().uploaded_bytes().to_vec(),
                            *entry.input().body.digest(),
                        )
                    })
                    .collect()
            })
            .await
            .unwrap()
    }
    fn ready(&self) {
        *self.environment.0.lock().unwrap() = DispatchReadiness::Ready(epoch());
    }
    async fn close(self) {
        let report = self
            .service
            .shutdown(RegistryDeadline::new(
                Instant::now() + Duration::from_secs(5),
            ))
            .await;
        self.storage.close().await;
        assert!(report.joined, "real queue owner must join: {report:?}");
        assert!(self.path.join("node.sqlite3").is_file());
        // Preserve synthetic DB for the runner's ownership/cleanup evidence until it exits.
    }
}

#[tokio::test]
async fn pa06_real_r3_same_id_full_bytes_dedup_and_whitespace_conflict() {
    let f = Area::new().await;
    let result = AssertUnwindSafe(async {
        let bytes = b" \n[{\"type\":\"text\",\"text\":\"a\"}]\t";
        let original = f.body(bytes);
        assert_eq!(*original.digest(), <[u8; 32]>::from(Sha256::digest(bytes)));
        let (id, outcome) = f.act(ValidatedMutation::Submit(original)).await;
        assert_eq!(
            outcome,
            QueueOutcome::Queued {
                item: id.entity(),
                revision: 1
            }
        );
        let snapshot = f.waiting().await;
        let fact = f.service.query_request(&ACTOR, TARGET, id).await.unwrap();
        let again = f
            .service
            .consume(&ACTOR, TARGET, id, ValidatedMutation::Submit(f.body(bytes)))
            .await;
        match again {
            ConsumeReply::Resolved {
                fact: repeated,
                returned: Some(_),
            } => assert_eq!(repeated, fact),
            other => panic!("retry must return original fact and unconsumed body: {other:?}"),
        }
        for changed in [
            br#"[{"type":"text","text":"a"}]"#.as_slice(),
            br#" [{"type":"text","text":"\u0061"}] "#.as_slice(),
            br#" [{"type":"text","text":"changed"}] "#.as_slice(),
        ] {
            let reply = f
                .service
                .consume(
                    &ACTOR,
                    TARGET,
                    id,
                    ValidatedMutation::Submit(f.body(changed)),
                )
                .await;
            assert!(matches!(
                reply,
                ConsumeReply::Refused {
                    error: RegistryError::Conflict,
                    ..
                }
            ));
            assert_eq!(f.waiting().await, snapshot);
            assert_eq!(
                f.service.query_request(&ACTOR, TARGET, id).await.unwrap(),
                fact
            );
        }
        let (new_id, new_outcome) = f.act(ValidatedMutation::Submit(f.body(bytes))).await;
        assert_ne!(id, new_id);
        assert_eq!(
            new_outcome,
            QueueOutcome::Queued {
                item: new_id.entity(),
                revision: 1
            }
        );
        assert_eq!(f.waiting().await.len(), 2);
        let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM product_runs")
            .fetch_one(f.storage.pool_for_test())
            .await
            .unwrap();
        assert_eq!(runs, 0);
    })
    .catch_unwind()
    .await;
    f.close().await;
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

#[tokio::test]
async fn pa06_parse_failure_precedes_all_r3_calls_and_retainer_is_same_arc() {
    let f = Area::new().await;
    let result = AssertUnwindSafe(async {
        let mut calls_to_r3 = 0;
        let bad = PromptDocument::parse(br#"[{"type":"text","text":false}]"#.to_vec());
        if let Ok(doc) = bad {
            calls_to_r3 += 1;
            let body = f.service.validate_body(&doc).unwrap();
            let _ = f
                .service
                .register(&ACTOR, TARGET, &ValidatedMutation::Submit(body))
                .await;
        }
        assert_eq!(calls_to_r3, 0, "parse error must not enter R3");
        assert!(f.waiting().await.is_empty());
        let registered: i64 = sqlx::query_scalar("SELECT count(*) FROM node_request_registrations")
            .fetch_one(f.storage.pool_for_test())
            .await
            .unwrap();
        assert_eq!(registered, 0);
        let doc = Area::document(b" \n[{\"type\":\"text\",\"text\":\"owned\"}]\t");
        let address = doc.prompt_fragment().as_ptr();
        let body = f.service.validate_body(&doc).unwrap();
        let retained = f.adapter.retain(&body);
        assert_eq!(retained.as_ref().fragment().as_ptr(), address);
        drop(doc);
        drop(body);
        assert_eq!(
            retained.as_ref().fragment(),
            r#"[{"type":"text","text":"owned"}]"#
        );
    })
    .catch_unwind()
    .await;
    f.close().await;
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

#[tokio::test]
async fn pa07_prepared_capture_drop_edit_stale_selection_and_new_revision_bytes() {
    let f = Area::new().await;
    let result = AssertUnwindSafe(async {
        let first = b" [{\"type\":\"text\",\"text\":\"first\"}] ";
        let (request, _) = f.act(ValidatedMutation::Submit(f.body(first))).await;
        f.ready();
        let prepared = f
            .driver
            .prepare_capture(TARGET, f.adapter.as_ref())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            prepared.body().as_ref().fragment(),
            r#"[{"type":"text","text":"first"}]"#
        );
        drop(prepared);
        let old = f
            .driver
            .prepare_capture(TARGET, f.adapter.as_ref())
            .await
            .unwrap()
            .unwrap();
        let changed = b" [{\"type\":\"text\",\"text\":\"new version\"}] ";
        let (_, outcome) = f
            .act(ValidatedMutation::Edit {
                item: request.entity(),
                revision: 1,
                body: f.body(changed),
            })
            .await;
        assert_eq!(
            outcome,
            QueueOutcome::Edited {
                item: request.entity(),
                revision: 2
            }
        );
        let Err((_, old)) = f.driver.select_capture(old).await else {
            panic!("edited preparation must never select old content");
        };
        assert_eq!(
            old.body().as_ref().fragment(),
            r#"[{"type":"text","text":"first"}]"#
        );
        drop(old);
        let new = f
            .driver
            .prepare_capture(TARGET, f.adapter.as_ref())
            .await
            .unwrap()
            .unwrap();
        let pointer = new.body().as_ref().fragment().as_ptr();
        let Ok(selected) = f.driver.select_capture(new).await else {
            panic!("current real preparation must select");
        };
        assert_eq!(selected.retained.as_ref().fragment().as_ptr(), pointer);
        assert_eq!(
            selected.retained.as_ref().fragment(),
            r#"[{"type":"text","text":"new version"}]"#
        );
        assert_eq!(selected.origin.revision, 2);
        let (_, cancel) = f
            .act(ValidatedMutation::Cancel {
                item: request.entity(),
                revision: 2,
            })
            .await;
        assert!(matches!(cancel, QueueOutcome::Rejected(_)));
        assert_eq!(
            selected.retained.as_ref().fragment(),
            r#"[{"type":"text","text":"new version"}]"#
        );
    })
    .catch_unwind()
    .await;
    f.close().await;
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

#[tokio::test]
async fn pa07_direct_capture_is_from_real_consumption_and_keeps_full_byte_binding() {
    let f = Area::new().await;
    let result = AssertUnwindSafe(async {
        f.ready();
        let work = f.driver.open_external_work(TARGET, epoch()).await.unwrap();
        f.driver
            .observe(TARGET, QueueOwnerEvent::ExternalForegroundFailure { work })
            .await
            .unwrap();
        let bytes = b" \n[{\"type\":\"future\",\"n\":9007199254740993}]\t";
        let body = f.body(bytes);
        let root_pointer = body.body().prompt_fragment().as_ptr();
        let hash = *body.digest();
        let (id, outcome) = f.act(ValidatedMutation::Submit(body)).await;
        assert_eq!(
            outcome,
            QueueOutcome::DirectSelected {
                item: id.entity(),
                revision: 1
            }
        );
        let selected = f
            .driver
            .capture_direct(TARGET, f.adapter.as_ref())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(selected.origin.digest, hash);
        assert_eq!(selected.origin.submit_request, id);
        assert_eq!(selected.retained.as_ref().fragment().as_ptr(), root_pointer);
        assert_eq!(
            selected.retained.as_ref().fragment(),
            r#"[{"type":"future","n":9007199254740993}]"#
        );
    })
    .catch_unwind()
    .await;
    f.close().await;
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

mod actual_wire {
    use super::*;
    use crate::adapters::acp::{
        Connection, EventReceiver, ExtensionFields, FenceRejection, Incoming, IoOwner, PollIo,
        Response, ResponseDeadline, RpcId, SessionId, Version, WriteFence, WriteIdentity,
        WriteReport, WriteStep,
    };
    use serde_json::{Value, json};
    use std::task::Context;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf},
        time::timeout,
    };

    struct ControlledLocalFence;
    impl WriteFence for ControlledLocalFence {
        fn check_and_poll(
            &self,
            _: &WriteIdentity,
            _: WriteStep,
            cx: &mut Context<'_>,
            io: &mut dyn PollIo,
        ) -> Result<(), FenceRejection> {
            let _ = io.poll(cx);
            Ok(())
        }
    }
    type Peer = (BufReader<ReadHalf<DuplexStream>>, WriteHalf<DuplexStream>);
    fn limit() -> Instant {
        Instant::now() + Duration::from_secs(120)
    }
    async fn read(peer: &mut Peer) -> Vec<u8> {
        let mut bytes = Vec::new();
        let count = timeout(
            Duration::from_secs(120),
            peer.0.read_until(b'\n', &mut bytes),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(count > 0 && bytes.last() == Some(&b'\n'));
        bytes
    }
    async fn feed(peer: &mut Peer, value: &Value) {
        let mut bytes = serde_json::to_vec(value).unwrap();
        bytes.push(b'\n');
        peer.1.write_all(&bytes).await.unwrap();
    }
    async fn start(selected: Version) -> (Connection, EventReceiver, IoOwner, Peer) {
        let (a, b) = tokio::io::duplex(128 * 1024);
        let (ar, aw) = tokio::io::split(a);
        let (br, bw) = tokio::io::split(b);
        let (connection, mut events, owner) = Connection::spawn(ar, aw).unwrap();
        let mut peer = (BufReader::new(br), bw);
        let reservation = connection.reserve_initialize(Version::V2, limit()).unwrap();
        let frame = reservation.encode().unwrap();
        let ticket = connection
            .enqueue(reservation, frame, Arc::new(ControlledLocalFence), limit())
            .unwrap_or_else(|e| panic!("initialize enqueue: {}", e.error()));
        let init: Value = serde_json::from_slice(&read(&mut peer).await).unwrap();
        assert_eq!(init["params"]["protocolVersion"], 2);
        assert_eq!(ticket.wait().await, WriteReport::Written);
        let result = if selected == Version::V1 {
            json!({"protocolVersion":1,"agentCapabilities":{"promptCapabilities":{"image":true,"audio":true,"embeddedContext":true}}})
        } else {
            json!({"protocolVersion":2,"info":{"name":"controlled-peer","version":"fixed"},"capabilities":{"session":{"prompt":{"image":{},"audio":{},"embeddedContext":{}}}}})
        };
        feed(
            &mut peer,
            &json!({"jsonrpc":"2.0","id":init["id"],"result":result}),
        )
        .await;
        let received = timeout(Duration::from_secs(10), events.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(matches!(
            received.incoming(),
            Incoming::Response {
                response: Response::Initialized(_),
                ..
            }
        ));
        assert_eq!(connection.strategy().unwrap().version(), selected);
        (connection, events, owner, peer)
    }
    // Independent span scan: copy complete JSON string tokens verbatim; remove only outside WS.
    // This is not a call to the product's compact(), and does not parse/re-emit any values.
    fn expected_wire_root(bytes: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'"' {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                result.extend_from_slice(&bytes[start..i]);
            } else {
                if !b" \t\r\n".contains(&bytes[i]) {
                    result.push(bytes[i]);
                }
                i += 1;
            }
        }
        result
    }

    #[tokio::test]
    #[allow(
        clippy::too_many_lines,
        reason = "Keep real IO ownership, cleanup and each version's assertions in one acceptance scenario"
    )]
    async fn pa05_retained_actual_prompt_reaches_wire_above_old_frame_limit_and_v1_fallback() {
        let f = Area::new().await;
        let result = AssertUnwindSafe(async {
            for selected in [Version::V1, Version::V2] {
                let mut uploaded =
                    b" \n[ { \"type\" : \"text\", \"text\" : \"  escaped \\u0061 \\n ".to_vec();
                let text_length = if selected == Version::V2 {
                    33 * 1024 * 1024
                } else {
                    33
                };
                uploaded.extend(std::iter::repeat_n(b'x', text_length));
                uploaded
                    .extend_from_slice(b"\", \"unknown\" : 9007199254740993, \"n\": 1e+009 } ]\t");
                let original_hash: [u8; 32] = Sha256::digest(&uploaded).into();
                let doc =
                    PromptDocument::parse(uploaded).expect("complete upload above obsolete cap");
                let body = f.service.validate_body(&doc).unwrap();
                assert_eq!(*body.digest(), original_hash);
                let retained: OwnedRawValue = f.adapter.retain(&body);
                assert_eq!(
                    retained.as_ref().fragment().as_ptr(),
                    doc.prompt_fragment().as_ptr()
                );
                let (connection, mut events, owner, mut peer) = start(selected).await;
                let wire_result = AssertUnwindSafe(async {
                    let session_doc =
                        crate::adapters::acp::raw_json::RawDocument::parse(br#""s""#.to_vec())
                            .unwrap();
                    let session = SessionId::parse(session_doc.root()).unwrap();
                    let strategy = connection.strategy().unwrap();
                    let request = strategy
                        .prompt(session, retained, ExtensionFields::empty().unwrap())
                        .unwrap();
                    assert_eq!(
                        request
                            .params()
                            .object()
                            .unwrap()
                            .get("prompt")
                            .unwrap()
                            .unwrap()
                            .fragment()
                            .as_bytes(),
                        expected_wire_root(doc.prompt_fragment().as_bytes())
                    );
                    let reservation = connection
                        .reserve_request(request, ResponseDeadline::At(limit()))
                        .unwrap();
                    let id = reservation.id().unwrap().clone();
                    let frame = reservation.encode().unwrap();
                    let ticket = connection
                        .enqueue(reservation, frame, Arc::new(ControlledLocalFence), limit())
                        .unwrap_or_else(|e| panic!("prompt enqueue: {}", e.error()));
                    let bytes = read(&mut peer).await;
                    assert_eq!(ticket.wait().await, WriteReport::Written);
                    if selected == Version::V2 {
                        assert!(bytes.len() > 32 * 1024 * 1024);
                    }
                    let raw = crate::adapters::acp::raw_json::RawDocument::parse(bytes).unwrap();
                    let root = raw.root().object().unwrap();
                    assert_eq!(
                        root.get("method").unwrap().unwrap().string().unwrap(),
                        "session/prompt"
                    );
                    assert_eq!(RpcId::parse(root.get("id").unwrap().unwrap()).unwrap(), id);
                    let observed = root
                        .get("params")
                        .unwrap()
                        .unwrap()
                        .object()
                        .unwrap()
                        .get("prompt")
                        .unwrap()
                        .unwrap();
                    let expected = expected_wire_root(doc.prompt_fragment().as_bytes());
                    assert_eq!(observed.fragment().as_bytes(), expected);
                    let mut id_bytes = Vec::new();
                    id.write_json(&mut id_bytes).unwrap();
                    let id_value: Value = serde_json::from_slice(&id_bytes).unwrap();
                    let response = if selected == Version::V1 {
                        json!({"stopReason":"end_turn"})
                    } else {
                        json!({"messageId":"actual-message"})
                    };
                    feed(
                        &mut peer,
                        &json!({"jsonrpc":"2.0","id":id_value,"result":response}),
                    )
                    .await;
                    let response = timeout(Duration::from_secs(10), events.next())
                        .await
                        .unwrap()
                        .unwrap()
                        .unwrap();
                    assert!(matches!(
                        response.incoming(),
                        Incoming::Response {
                            response: Response::Prompt(_),
                            ..
                        }
                    ));
                    assert_eq!(connection.pending_count(), 0);
                })
                .catch_unwind()
                .await;
                let closed = owner
                    .shutdown(Instant::now() + Duration::from_secs(5))
                    .await;
                assert!(closed.joined, "owned IO must join");
                if let Err(panic) = wire_result {
                    std::panic::resume_unwind(panic);
                }
            }
        })
        .catch_unwind()
        .await;
        f.close().await;
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }
}

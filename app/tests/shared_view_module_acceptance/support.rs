use acp_remote::{
    adapters::acp::{
        ConfigSourceStamp, ExtensionFields, Known, SessionId, SessionSetup, SetupSnapshot, Version,
        raw_json::RawDocument,
    },
    application::shared_views::{
        budget::ReadReason,
        controller::{HookSelector, ViewHookPoint},
    },
    domain::{EntityId, native::NativeSourceId, shared_view::ViewError},
    shared_view_test_support::{
        AcpViewModuleFixture, CollectedModuleRecord, FixturePeer, FixturePeerWire,
        FixtureSigningClient, FixtureSourceConfig, FixtureTarget, FixtureTaskOwner,
        LocalViewAuthFixture, ModuleViewTransfer, OwnedTestArea,
    },
};
use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::time::{Instant, timeout_at};

pub fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

pub fn stage(label: &str, phase: &str) {
    eprintln!("[SHARED-PUBLIC] case={label} phase={phase}");
}

pub fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn setup(version: Version) -> Result<SessionSetup> {
    Ok(SessionSetup::try_from_snapshot(SetupSnapshot {
        wire: version,
        source: ConfigSourceStamp {
            reference: Arc::from("independent-module-known-config"),
            revision: 1,
        },
        cwd: Known::Known(Arc::from("D:/independent-synthetic-cwd")),
        mcp_servers: Known::Known(Vec::new()),
        additional_directories: Known::Known(Vec::new()),
        extra: ExtensionFields::empty()?,
    })?)
}

pub fn session(text: &str) -> Result<SessionId> {
    let raw = RawDocument::parse(serde_json::to_vec(text)?)?;
    Ok(SessionId::parse(raw.root())?)
}

pub fn update(session: &str, text: &str) -> Result<Vec<u8>> {
    // Only self-owned small string fields are JSON-encoded here. Unknown numeric vectors
    // are built independently as raw bytes in the actual test, never Value round-tripped.
    Ok(format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{{\"sessionId\":{},\"update\":{{\"sessionUpdate\":\"independent_unknown\",\"text\":{}}}}}}}",
        serde_json::to_string(session)?,
        serde_json::to_string(text)?
    ).into_bytes())
}

pub async fn next_rpc(peer: &mut FixturePeer) -> Result<serde_json::Value> {
    let bytes = peer
        .next_node_frame(deadline())
        .await?
        .context("original peer EOF")?;
    let rpc: serde_json::Value = serde_json::from_slice(&bytes)?;
    ensure!(rpc.get("id").is_some(), "peer frame must be a real request");
    Ok(rpc)
}

pub async fn send_raw(peer: &mut FixturePeer, bytes: &[u8]) -> Result<()> {
    let mut frame = bytes.to_vec();
    frame.push(b'\n');
    peer.write_bytes(&frame, deadline()).await?;
    Ok(())
}

pub async fn respond(peer: &mut FixturePeer, rpc: &serde_json::Value, result: &str) -> Result<()> {
    let id = rpc.get("id").context("actual scalar request id")?;
    send_raw(
        peer,
        format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{result}}}").as_bytes(),
    )
    .await
}

pub struct Rig {
    pub module: AcpViewModuleFixture,
    pub peer: FixturePeer,
    pub auth: Arc<LocalViewAuthFixture>,
    pub signer: FixtureSigningClient,
    pub client:
        acp_remote::application::shared_views::local_authority::AuthenticatedLocalViewClient,
    owner: FixtureTaskOwner,
    io_area: OwnedTestArea,
    _parent: PathBuf,
    label: &'static str,
}

impl Rig {
    pub async fn start(version: Version, replay: bool, label: &'static str) -> Result<Self> {
        let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.tmp/shared-module-independent-fixtures");
        std::fs::create_dir_all(&parent)?;
        let parent = parent.canonicalize()?;
        let auth_area = OwnedTestArea::create(&parent, "independent-auth")?;
        eprintln!(
            "[SHARED-OWN-AREA] case={label} auth_child={}",
            auth_area
                .path()
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        );
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["shared-view.fixture.test".to_owned()])?;
        auth_area.install_tls(
            cert.pem().as_bytes(),
            signing_key.serialize_pem().as_bytes(),
        )?;
        stage(label, "before-real-auth-node-start");
        let auth = Arc::new(
            LocalViewAuthFixture::start(auth_area)
                .await
                .context("real temporary Node auth start; no returned Rig yet")?,
        );
        stage(label, "after-real-auth-node-start");
        let signer = FixtureSigningClient::generate()?;
        let client = auth
            .authenticate(&signer)
            .await
            .context("actual fixture pairing/authentication; no returned Rig yet")?;
        stage(label, "after-real-client-authenticate");
        let io_area = OwnedTestArea::create(&parent, "independent-io")?;
        eprintln!(
            "[SHARED-OWN-AREA] case={label} io_child={}",
            io_area
                .path()
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        );
        let owner = FixtureTaskOwner::new(&io_area, tokio::runtime::Handle::current())?;
        let (wire, mut peer) =
            FixturePeerWire::from_duplex(&owner.handle(), tokio::io::duplex(64 * 1024))
                .map_err(|error| error.error())?;
        let source = NativeSourceId(EntityId::from_bytes(acp_remote::random::bytes()?));
        let start = AcpViewModuleFixture::start(
            FixtureSourceConfig {
                source,
                entry_alias: Arc::from("independent-entry"),
            },
            wire,
            Arc::clone(&auth),
        );
        let init = async {
            let request = next_rpc(&mut peer).await?;
            ensure!(
                request["method"] == "initialize",
                "actual first RPC must initialize"
            );
            ensure!(
                request["params"]["protocolVersion"] == 2,
                "preferred implemented version offered"
            );
            let response = match version {
                Version::V1 => format!("{{\"protocolVersion\":1,\"agentCapabilities\":{{\"loadSession\":{replay},\"sessionCapabilities\":{{\"resume\":{{}}}}}}}}"),
                Version::V2 => "{\"protocolVersion\":2,\"info\":{\"name\":\"independent\",\"version\":\"synthetic\"},\"capabilities\":{\"session\":{}}}".to_owned(),
            };
            respond(&mut peer, &request, &response).await
        };
        stage(label, "before-module-initialize");
        let (module, initialized) = timeout_at(deadline(), async { tokio::join!(start, init) })
            .await
            .context("module start/real Initialize outer deadline; no returned Rig yet")?;
        initialized?;
        let module = module?;
        stage(label, "module-start-returned");
        Ok(Self {
            module,
            peer,
            auth,
            signer,
            client,
            owner,
            io_area,
            _parent: parent,
            label,
        })
    }

    pub async fn create(
        &mut self,
        version: Version,
        sid: &str,
        early: Option<&[u8]>,
    ) -> Result<FixtureTarget> {
        let creating = self.module.create(setup(version)?);
        let response = async {
            let rpc = next_rpc(&mut self.peer).await?;
            ensure!(rpc["method"] == "session/new", "actual create method");
            if let Some(raw) = early {
                send_raw(&mut self.peer, raw).await?;
            }
            respond(
                &mut self.peer,
                &rpc,
                &format!("{{\"sessionId\":{}}}", serde_json::to_string(sid)?),
            )
            .await
        };
        let (target, response) =
            timeout_at(deadline(), async { tokio::join!(creating, response) }).await?;
        response?;
        Ok(target?)
    }

    pub async fn resume(&mut self, version: Version, sid: &str) -> Result<FixtureTarget> {
        let resuming = self.module.resume_existing(session(sid)?, setup(version)?);
        let response = async {
            let rpc = next_rpc(&mut self.peer).await?;
            ensure!(rpc["method"] == "session/resume", "actual resume method");
            ensure!(
                rpc["params"]["sessionId"] == sid,
                "original session identity"
            );
            ensure!(
                rpc["params"].get("replayFrom").is_none(),
                "ordinary resume is not replay"
            );
            if version == Version::V2 {
                // Actual protocol snapshot event in the accepted Resume window, not a flag.
                let snapshot = format!(
                    "{{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{{\"sessionId\":{},\"update\":{{\"sessionUpdate\":\"state_update\",\"state\":\"idle\"}}}}}}",
                    serde_json::to_string(sid)?
                );
                send_raw(&mut self.peer, snapshot.as_bytes()).await?;
            }
            respond(&mut self.peer, &rpc, "{}").await
        };
        let (target, response) =
            timeout_at(deadline(), async { tokio::join!(resuming, response) }).await?;
        response?;
        Ok(target?)
    }

    pub async fn open_transfer(
        &self,
        target: &FixtureTarget,
    ) -> Result<(ModuleViewTransfer, EntityId)> {
        self.open_transfer_for(target, &self.client).await
    }

    pub async fn send_captured(&mut self, bytes: &[u8]) -> Result<()> {
        let scope = self.module.scope();
        let before = self.module.probe().source(&scope)?;
        // Drain only already-captured entries before arming the next actual Raw point.
        // This polls observable monotonically processed facts, not a time/sleep guess.
        timeout_at(deadline(), async {
            loop {
                let now = self.module.probe().source(&scope)?;
                ensure!(
                    now.first_missing_sequence().is_none(),
                    "fixture source has no capture gap"
                );
                if now.processed_frontier() >= before.retained_frontier()
                    && now.processed_route_version() >= before.route_version()
                {
                    return Ok::<(), anyhow::Error>(());
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .context("wait for already-captured fixture prefix")??;
        let expected = before
            .retained_frontier()
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("fixture expected sequence overflow"))?;
        let mut captured = self.module.test_controller().observe_once(
            ViewHookPoint::AfterCaptureEnqueue,
            HookSelector::Source(scope.clone()),
        )?;
        send_raw(&mut self.peer, bytes).await?;
        let observation = timeout_at(deadline(), captured.arrived()).await??;
        ensure!(
            observation.scope() == Some(&scope),
            "actual capture witness belongs to original source"
        );
        ensure!(
            observation.sequence() == Some(expected),
            "next actual Received was captured before opening"
        );
        stage(self.label, "actual-capture-witness-before-open");
        Ok(())
    }

    pub async fn open_transfer_for(
        &self,
        target: &FixtureTarget,
        client: &acp_remote::application::shared_views::local_authority::AuthenticatedLocalViewClient,
    ) -> Result<(ModuleViewTransfer, EntityId)> {
        let mut started = self.module.test_controller().observe_once(
            ViewHookPoint::AfterStartSendBeforeFlush,
            HookSelector::Target(target.key().clone()),
        )?;
        let transfer = timeout_at(
            deadline(),
            self.module
                .open_transfer(client, target, ReadReason::FirstOpen),
        )
        .await??;
        let observed = timeout_at(deadline(), started.arrived()).await??;
        ensure!(
            observed.scope() == Some(&self.module.scope()),
            "actual started source scope"
        );
        let stream = observed.stream_id().context("actual Started stream data")?;
        ensure!(
            self.module.probe().stream(stream).is_some(),
            "actual owned stream exists"
        );
        Ok((transfer, stream))
    }

    pub async fn collect(
        &self,
        transfer: &mut ModuleViewTransfer,
        stream: EntityId,
    ) -> Result<CollectedModuleRecord> {
        let collected = timeout_at(deadline(), transfer.next_record()).await;
        if collected.is_err() {
            if let Ok(source) = self.module.probe().source(&self.module.scope()) {
                eprintln!(
                    "[SHARED-WAIT] case={} retained={} processed={} route={} processed_route={}",
                    self.label,
                    source.retained_frontier(),
                    source.processed_frontier(),
                    source.route_version(),
                    source.processed_route_version()
                );
            }
            if let Some(observed) = self.module.probe().stream(stream) {
                eprintln!(
                    "[SHARED-WAIT] case={} started={} flushed={} committed={} early_credit={} snapshot_exhausted={}",
                    self.label,
                    observed.highest_started(),
                    observed.highest_flushed(),
                    observed.highest_committed(),
                    observed.pending_early_credit(),
                    observed.snapshot_cursor().exhausted()
                );
            }
        }
        let record = collected.context("collect actual peer next_record deadline")??;
        eprintln!(
            "[SHARED-RECORD] case={} kind={:?} length={}",
            self.label,
            record.address().kind(),
            record.bytes().len()
        );
        let inspection = self
            .module
            .probe()
            .stream(stream)
            .context("original stream inspection")?;
        ensure!(
            inspection.highest_started() > inspection.highest_committed(),
            "last actual peer-received packet awaits credit"
        );
        ensure!(
            record.address().is_last(),
            "peer collection returns a complete record"
        );
        ensure!(
            record.address().total_length() == u64::try_from(record.bytes().len())?,
            "exact total bytes"
        );
        timeout_at(deadline(), transfer.credit(inspection.highest_started())).await??;
        let committed = self
            .module
            .probe()
            .stream(stream)
            .context("same stream after credit")?;
        ensure!(
            committed.highest_committed() == inspection.highest_started(),
            "actual flush and credit commit same packet"
        );
        Ok(record)
    }

    pub async fn wait_for_raw(
        &self,
        transfer: &mut ModuleViewTransfer,
        stream: EntityId,
        expected: &[u8],
    ) -> Result<()> {
        use acp_remote::application::shared_views::wire::RecordKind;
        for _ in 0..32 {
            let record = self.collect(transfer, stream).await?;
            if record.address().kind() == RecordKind::OriginalRaw
                && digest(record.bytes()) == digest(expected)
            {
                ensure!(
                    record.bytes().len() == expected.len(),
                    "original raw exact length"
                );
                return Ok(());
            }
        }
        bail!("expected original raw not present within independent bounded fixture records")
    }

    pub async fn close(mut self) -> Result<()> {
        stage(self.label, "cleanup-module-start");
        let report = self.module.shutdown(deadline()).await;
        eprintln!(
            "[SHARED-CLEANUP] case={} module_io={} module_pump={} module_views={} failure={:?}",
            self.label,
            report.io_joined(),
            report.pump_joined(),
            report.views_joined(),
            report.failure()
        );
        let io = self.owner.shutdown(deadline()).await;
        eprintln!(
            "[SHARED-CLEANUP] case={} fixture_joined={} tasks={} resources={} failure={:?}",
            self.label,
            io.all_joined(),
            io.pending_tasks(),
            io.pending_resources(),
            io.failure()
        );
        let auth = self.auth.close().await;
        if auth.is_err() {
            eprintln!(
                "[SHARED-AUTH-CLOSE-FIRST] {:?}",
                self.auth.first_close_observation()
            );
            eprintln!(
                "[SHARED-NODE-SHUTDOWN-CURRENT] {:?}",
                self.auth.node_shutdown_observation()
            );
        }
        eprintln!(
            "[SHARED-CLEANUP] case={} auth_close_result={:?}",
            self.label, auth
        );
        let cleanup = self.io_area.cleanup().await;
        match &cleanup {
            Ok(cleanup) => eprintln!(
                "[SHARED-CLEANUP] case={} io_area_removed={} io_area_pending={}",
                self.label,
                cleanup.removed(),
                cleanup.pending_resources()
            ),
            Err(error) => eprintln!(
                "[SHARED-CLEANUP] case={} io_area_cleanup_error={error:?}",
                self.label
            ),
        }
        ensure!(
            report.io_joined() && report.pump_joined() && report.views_joined(),
            "original Module owners actually joined"
        );
        ensure!(
            report.failure().is_none(),
            "Module shutdown report has no failure"
        );
        ensure!(
            io.all_joined() && io.pending_tasks() == 0 && io.pending_resources() == 0,
            "original fixture owner drained tasks/resources"
        );
        auth?;
        ensure!(
            cleanup?.removed(),
            "first cleanup removes only this owned area"
        );
        Ok(())
    }
}

pub async fn assert_no_more_rpc(peer: &mut FixturePeer) -> Result<()> {
    match peer
        .next_node_frame(Instant::now() + Duration::from_millis(100))
        .await
    {
        Err(acp_remote::shared_view_test_support::FixtureError::Timeout) => Ok(()),
        _ => Err(anyhow::anyhow!(
            "unexpected additional RPC, EOF or IO failure"
        )),
    }
}

pub fn expected_unsupported() -> String {
    ViewError::Unsupported.to_string()
}

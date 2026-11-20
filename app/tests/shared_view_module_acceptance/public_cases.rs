use super::support::{self, Rig, deadline, digest, send_raw, update};
use acp_remote::{
    adapters::acp::Version,
    application::shared_views::{budget::ReadReason, wire::RecordKind},
    domain::shared_view::{ReadMode, ViewError},
    shared_view_test_support::{FixtureSigningClient, FixtureTargetOrigin},
};
use anyhow::{Context, Result, bail, ensure};
use futures_util::FutureExt;
use std::panic::AssertUnwindSafe;
use tokio::time::timeout_at;

// Every body result, including a caught panic, closes the actual owners before returning.
// These are independent Module data/IO/auth assertions, not whole SV IDs or Node/WSS.

#[allow(
    clippy::too_many_lines,
    reason = "Keep the original transfer, first snapshot before replay response, explicit rebind and same-H tail assertions in one ordered independent ownership flow"
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn module_p01_v1_real_create_early_raw_first_snapshot_then_tail() -> Result<()> {
    let mut rig = Rig::start(Version::V1, true, "P01")
        .await
        .context("P01 fixture start")?;
    let result = AssertUnwindSafe(async {
        let early = update("created-session", "already-received-before-session-created")?;
        support::stage("P01", "before-create-with-early-raw");
        let target = rig
            .create(Version::V1, "created-session", Some(&early))
            .await
            .context("P01 real create and early raw")?;
        support::stage("P01", "after-create-before-open");
        ensure!(
            target.origin() == FixtureTargetOrigin::ModuleFixture,
            "origin never Native Loaded"
        );
        ensure!(
            target.scope() == rig.module.scope(),
            "actual source scope returned"
        );
        ensure!(
            target.key().native_id() == "created-session",
            "real NewSession SID binding"
        );
        let (mut transfer, mut stream) = rig
            .open_transfer(&target)
            .await
            .context("P01 first transfer/Started observation")?;
        support::stage("P01", "after-open-before-snapshot");
        let load = support::next_rpc(&mut rig.peer)
            .await
            .context("P01 original load request")?;
        ensure!(
            load["method"] == "session/load",
            "actual supported v1 standard replay requested"
        );
        support::stage("P01", "actual-load-read-without-reply");
        let mut seen_early = false;
        let mut snapshot_end = false;
        for _ in 0..32 {
            let record = rig.collect(&mut transfer, stream).await?;
            match record.address().kind() {
                RecordKind::OriginalRaw => {
                    ensure!(
                        digest(record.bytes()) == digest(&early),
                        "only exact assigned old raw in initial snapshot"
                    );
                    seen_early = true;
                }
                RecordKind::SnapshotEnd => {
                    snapshot_end = true;
                    break;
                }
                RecordKind::TurnStart | RecordKind::FieldBytes => {
                    bail!("unexpected history record in the ordinary first snapshot")
                }
                RecordKind::ItemProjection | RecordKind::Status => {}
            }
        }
        ensure!(
            seen_early && snapshot_end,
            "first snapshot contains existing raw without any new output"
        );
        support::stage("P01", "snapshot-existing-raw-and-end-verified");
        let inspection = rig
            .module
            .probe()
            .stream(stream)
            .ok_or_else(|| anyhow::anyhow!("original stream"))?;
        ensure!(
            inspection.snapshot_cursor().exhausted(),
            "actual SnapshotEnd flush plus credit enters tail"
        );
        // The original read is still in flight until here: the existing first snapshot
        // above is never replaced by an already-completed Replay just to make it pass.
        let previous_stream = stream;
        let previous_epoch = inspection.view_epoch();
        support::respond(&mut rig.peer, &load, "{}").await?;
        match timeout_at(deadline(), transfer.next_record()).await? {
            Err(ViewError::StaleCursor) => {}
            _ => bail!("real new Replay publication must explicitly stale the original stream"),
        }
        let notice = timeout_at(deadline(), transfer.next_record()).await??;
        ensure!(
            notice.address().kind() == RecordKind::Status && notice.address().record_id() == 1,
            "same-read private rebind first delivers real Status record1"
        );
        let value: serde_json::Value = serde_json::from_slice(notice.bytes())?;
        ensure!(
            value["type"] == "stream_rebound",
            "actual peer receives explicit epoch/stream change"
        );
        ensure!(
            value["previousStreamId"] == previous_stream.encode(),
            "notice preserves original consumer stream identity"
        );
        stream = acp_remote::domain::EntityId::decode(
            value["streamId"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("new platform stream id"))?,
        )?;
        ensure!(
            stream != previous_stream,
            "old stream never silently becomes the new stream"
        );
        ensure!(
            value["viewEpoch"] == notice.address().view_epoch().as_entity_id().encode(),
            "new view identity matches actual peer record address"
        );
        ensure!(
            notice.address().view_epoch() != previous_epoch,
            "unproven cross-load identity rebuilds view epoch"
        );
        ensure!(
            value["sourceId"] == target.key().source().0.encode(),
            "same configured source on rebound"
        );
        let _read_epoch = acp_remote::domain::EntityId::decode(
            value["readEpoch"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("actual read epoch metadata"))?,
        )?;
        let revision = value["snapshotRevision"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("decimal snapshot revision"))?;
        ensure!(
            revision.parse::<u64>()?.to_string() == revision,
            "snapshot revision is canonical unsigned decimal data"
        );
        let started = rig
            .module
            .probe()
            .stream(stream)
            .ok_or_else(|| anyhow::anyhow!("new actual stream inspection"))?;
        ensure!(
            started.highest_started() == 1 && started.highest_committed() == 0,
            "new Status packet1 must wait for its own peer credit"
        );
        timeout_at(deadline(), transfer.credit(1)).await??;
        let mut rebound_end = false;
        for _ in 0..32 {
            if rig.collect(&mut transfer, stream).await?.address().kind() == RecordKind::SnapshotEnd
            {
                rebound_end = true;
                break;
            }
        }
        ensure!(rebound_end, "new same-read H snapshot ends before its tail");
        ensure!(
            rig.module
                .probe()
                .stream(stream)
                .ok_or_else(|| anyhow::anyhow!("rebound actual stream"))?
                .snapshot_cursor()
                .exhausted(),
            "new snapshot End really committed before its same-H tail"
        );
        support::assert_no_more_rpc(&mut rig.peer).await?;
        support::stage(
            "P01",
            "original-read-rebound-no-new-rpc-before-outside-window-tail",
        );
        let later = update("created-session", "new-tail-only-after-snapshot-end")?;
        send_raw(&mut rig.peer, &later).await?;
        rig.wait_for_raw(&mut transfer, stream, &later).await?;
        support::stage("P01", "tail-original-raw-verified");
        let receipt = timeout_at(deadline(), transfer.detach()).await??;
        ensure!(
            receipt.stream_id() == stream,
            "detach returns original local stream only"
        );
        Ok::<(), anyhow::Error>(())
    })
    .catch_unwind()
    .await;
    let cleanup = rig.close().await;
    let result = result.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    result?;
    cleanup
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn module_p02_v2_real_resume_32mib_raw_and_large_output_identity() -> Result<()> {
    let mut rig = Rig::start(Version::V2, true, "P02")
        .await
        .context("P02 fixture start")?;
    let result = AssertUnwindSafe(async {
        let target = rig.resume(Version::V2, "resumed-session").await.context("P02 real v2 Resume with snapshot")?;
        support::stage("P02", "resume-complete-before-large-raw");
        let identity = "opaque-tool-".to_owned() + &"x".repeat(32 * 1024 + 17);
        let text = "z".repeat(32 * 1024 * 1024 + 1);
        let raw = format!(
            "{{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{{\"sessionId\":\"resumed-session\",\"update\":{{\"sessionUpdate\":\"tool_call_update\",\"toolCallId\":{},\"_meta\":{{\"opaqueNumber\":123456789012345678901234567890123456789,\"deepValue\":{{\"unknown\":null}}}},\"unrecognizedBody\":{}}}}}}}",
            serde_json::to_string(&identity)?, serde_json::to_string(&text)?
        ).into_bytes();
        ensure!(raw.len() > 32 * 1024 * 1024, "actual frame crosses original size boundary");
        rig.send_captured(&raw).await?;
        let (mut transfer, stream) = rig.open_transfer(&target).await?;
        rig.wait_for_raw(&mut transfer, stream, &raw).await?;
        support::stage("P02", "large-original-raw-sha-verified");
        let target_state = rig.module.probe().target(target.key())?;
        ensure!(target_state.anchor_count() > 0, "real projected/unknown identity retained");
        let receipt = timeout_at(deadline(), transfer.detach()).await??;
        ensure!(receipt.stream_id() == stream, "only this local reader detached");
        Ok::<(), anyhow::Error>(())
    }).catch_unwind().await;
    let cleanup = rig.close().await;
    let result = result.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    result?;
    cleanup
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn module_p03_v1_no_replay_no_output_actual_limitation_status_and_zero_rpc() -> Result<()> {
    let mut rig = Rig::start(Version::V1, false, "P03")
        .await
        .context("P03 fixture start")?;
    let result = AssertUnwindSafe(async {
        let target = rig.resume(Version::V1, "empty-current-only").await?;
        let (mut transfer, stream) = rig.open_transfer(&target).await?;
        let mut saw_limitation = false;
        for _ in 0..16 {
            let record = rig.collect(&mut transfer, stream).await?;
            ensure!(
                record.address().kind() != RecordKind::OriginalRaw,
                "no synthetic body for empty view"
            );
            if record.address().kind() == RecordKind::Status {
                let value: serde_json::Value = serde_json::from_slice(record.bytes())?;
                if value["limitation"] == "requested_replay_failed" {
                    ensure!(
                        value["type"] == "view_segment",
                        "real platform Status record"
                    );
                    ensure!(
                        value["failure"] == support::expected_unsupported(),
                        "precise unsupported static failure"
                    );
                    ensure!(
                        value["readMode"] == "CurrentOnly",
                        "unsupported reading is not ReplayComplete"
                    );
                    ensure!(
                        value["completionResponseSequence"].is_null(),
                        "no fabricated completion response"
                    );
                    saw_limitation = true;
                    break;
                }
            }
        }
        ensure!(
            saw_limitation,
            "empty view must still expose original read limitation"
        );
        support::assert_no_more_rpc(&mut rig.peer).await?;
        timeout_at(deadline(), transfer.detach()).await??;
        Ok::<(), anyhow::Error>(())
    })
    .catch_unwind()
    .await;
    let cleanup = rig.close().await;
    let result = result.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    result?;
    cleanup
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn module_p04_actual_revocation_rejects_old_reader_other_client_keeps_shared_raw()
-> Result<()> {
    let mut rig = Rig::start(Version::V2, true, "P04")
        .await
        .context("P04 fixture start")?;
    let result = AssertUnwindSafe(async {
        let target = rig
            .resume(Version::V2, "shared-auth-target")
            .await
            .context("P04 real v2 Resume with snapshot")?;
        support::stage("P04", "resume-complete");
        let raw = update(
            "shared-auth-target",
            "retained-for-another-real-trusted-client",
        )?;
        rig.send_captured(&raw).await?;
        let read = rig
            .module
            .reads()
            .open(&rig.client, target.key(), ReadReason::FirstOpen)
            .await?;
        ensure!(
            read.descriptor().read_mode() != ReadMode::ReplayComplete,
            "no replay response yet"
        );
        let other_signer = FixtureSigningClient::generate()?;
        let other = rig.auth.authenticate(&other_signer).await?;
        rig.auth.revoke(&rig.signer).await?;
        support::stage("P04", "real-revocation-returned");
        match rig.module.reads().next_snapshot(&rig.client, &read).await {
            Err(ViewError::Revoked) => {}
            _ => bail!("real revoked actor must not prepare another chunk"),
        }
        match rig.module.reads().wait_change(&rig.client, &read).await {
            Err(ViewError::Revoked) => {}
            _ => bail!("revoked actor cannot regain reader authority through wait"),
        }
        let original_stream = read.stream_id();
        let released = rig.module.reads().release(read);
        ensure!(
            released.stream_id() == original_stream,
            "release returns the actual original reader"
        );
        let (mut transfer, stream) = rig.open_transfer_for(&target, &other).await?;
        rig.wait_for_raw(&mut transfer, stream, &raw).await?;
        support::stage("P04", "other-real-client-original-raw-verified");
        timeout_at(deadline(), transfer.detach()).await??;
        Ok::<(), anyhow::Error>(())
    })
    .catch_unwind()
    .await;
    let cleanup = rig.close().await;
    let result = result.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    result?;
    cleanup
}

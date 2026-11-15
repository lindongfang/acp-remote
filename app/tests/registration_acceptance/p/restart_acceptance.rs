//! Hard process exit evidence is separate from graceful shutdown and simulated errors.
#[allow(
    dead_code,
    reason = "This restart harness uses a subset of shared independent fixtures."
)]
mod support;
use acp_remote::{
    application::request_registry::{QueueOutcome, RegistrationId, RequestFact},
    storage::Storage,
};
use support::{
    A, B, Fixture, Repository, T1, Validator, deadline, outcome, resolved, submit, view,
};
use tokio::time::timeout;

#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "Keep hard-exit and restart evidence together for each frozen boundary."
)]
async fn cl098_reg01_real_child_crashes_keep_durable_identity_without_restoring_applied_body() {
    for stage in [
        "registered",
        "begin_committed",
        "applied",
        "settle_committed",
        "settled",
    ] {
        let fixture = Fixture::new().await;
        fixture.storage.close().await;
        let mut command =
            tokio::process::Command::new(env!("CARGO_BIN_EXE_registration_crash_probe"));
        command.arg(fixture.area.db()).arg(stage).kill_on_drop(true);
        let output = timeout(deadline(), command.output())
            .await
            .expect("owned crash child did not reach its bounded exit")
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(93),
            "child fixture failed before intentional exit: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let encoded = stdout
            .lines()
            .find_map(|s| s.strip_prefix("CRASH_REQUEST_ID="))
            .expect("crash child did not publish its metadata identity");
        let request = RegistrationId::decode(encoded).unwrap();
        let old_phase = fixture.phase(request).await;
        assert_eq!(
            old_phase.as_deref(),
            match stage {
                "registered" => None,
                "begin_committed" | "applied" => Some("applying"),
                _ => Some("settled"),
            }
        );
        for entry in std::fs::read_dir(&fixture.area.0).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                let bytes = std::fs::read(path).unwrap();
                let marker = b"CRASH_BODY_CANARY_12be951a";
                assert!(!bytes.windows(marker.len()).any(|window| window == marker));
            }
        }
        let reopened = Storage::open(&fixture.area.db()).await.unwrap();
        let control = fixture.control.clone();
        let (service, _) = reopened
            .start_queue_service_with_repository_and_audit_for_fixture(
                fixture.authority.clone(),
                Validator,
                fixture.clock.clone(),
                fixture.environment.clone(),
                fixture.audit.clone(),
                move |inner| Ok(Repository::new(inner, control)),
            )
            .await
            .unwrap();
        let empty = view(&service, &B, T1).await;
        assert!(empty.waiting.is_empty() && empty.active.is_none());
        let fact = service.query_request(&A, T1, request).await.unwrap();
        match stage {
            "registered" => assert!(matches!(fact, RequestFact::Registered(_))),
            "begin_committed" | "applied" => {
                assert!(matches!(fact, RequestFact::Interrupted { .. }));
            }
            _ => assert_eq!(
                outcome(&fact),
                QueueOutcome::Queued {
                    item: request.entity(),
                    revision: 1
                }
            ),
        }
        let (replayed, returned) = resolved(
            service
                .consume(
                    &A,
                    T1,
                    request,
                    submit(&service, b"CRASH_BODY_CANARY_12be951a"),
                )
                .await,
        );
        if stage == "registered" {
            assert_eq!(
                outcome(&replayed),
                QueueOutcome::Queued {
                    item: request.entity(),
                    revision: 1
                }
            );
            assert!(returned.is_none());
            assert_eq!(view(&service, &B, T1).await.waiting.len(), 1);
        } else {
            assert!(returned.is_some());
            assert!(view(&service, &B, T1).await.waiting.is_empty());
        }
        let report = service.shutdown(support::closing_deadline()).await;
        assert!(report.joined);
        reopened.close().await;
        println!("CL098 hard-exit stage={stage}: child_exit=93, old_phase={old_phase:?}");
    }
}

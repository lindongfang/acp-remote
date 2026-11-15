#![cfg(all(windows, feature = "acp-runtime-acceptance-fixtures"))]
#[path = "support/mod.rs"]
mod support;
use acp_remote::{
    adapters::acp::*,
    platform::process_owner::{OwnedProcess, ProcessSpec, ShutdownDisposition},
};
use std::{path::PathBuf, sync::Arc, time::Duration};
use support::*;
use tokio::{io::AsyncReadExt, time::timeout};

#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "Keep complete paired protocol/IO/Job ownership oracle together"
)]
async fn io27_actual_ownedprocess_unbuffered_pipe_and_complete_owned_tree() {
    for version in [Version::V1, Version::V2] {
        let executable = PathBuf::from(env!("CARGO_BIN_EXE_acp_runtime_protocol_child"));
        let cwd = executable.parent().unwrap().to_path_buf();
        let mut process = OwnedProcess::spawn(
            ProcessSpec {
                executable,
                args: vec![],
                cwd,
                env: vec![],
            },
            deadline(),
        )
        .await
        .unwrap();
        let io = process.take_io().unwrap();
        let mut stderr = io.stderr;
        let drain = tokio::spawn(async move {
            let mut out = vec![];
            stderr.read_to_end(&mut out).await.unwrap();
            out
        });
        let (client, mut receiver, owner) = Connection::spawn(io.stdout, io.stdin).unwrap();
        let res = client.reserve_initialize(version, deadline()).unwrap();
        let t = send(&client, res);
        finish(&t).await;
        let initialized = timeout(Duration::from_secs(20), receiver.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(matches!(
            initialized.incoming(),
            Incoming::Response {
                response: Response::Initialized(_),
                ..
            }
        ));
        let strategy = client.strategy().unwrap();
        let req = strategy
            .new_session(&setup(version, "C:/independent-synthetic"))
            .unwrap();
        let res = client
            .reserve_request(req, ResponseDeadline::At(deadline()))
            .unwrap();
        let t = send(&client, res);
        finish(&t).await;
        assert!(receiver.next().await.unwrap().is_some());
        let req = strategy
            .prompt(
                sid("A"),
                owned(br#"[{"type":"text","text":"synthetic-only"}]"#),
                ExtensionFields::empty().unwrap(),
            )
            .unwrap();
        let res = client
            .reserve_request(req, ResponseDeadline::Unbounded)
            .unwrap();
        let t = send(&client, res);
        finish(&t).await;
        let approval_event = receiver.next().await.unwrap().unwrap();
        let Incoming::Permission(p) = approval_event.incoming() else {
            panic!("nested permission absent")
        };
        assert_eq!(p.scope(), Some(client.scope()));
        let res = client
            .reserve_permission_response(p, PermissionSelection::Selected(oid("allow")))
            .unwrap();
        let frame = res.encode().unwrap();
        let reply = client
            .enqueue(res, frame, Arc::new(Fence), deadline())
            .unwrap();
        finish(&reply).await;
        let result = receiver.next().await.unwrap().unwrap();
        assert!(matches!(result.incoming(), Incoming::Response { .. }));
        let cancel = client
            .reserve_cancel(sid("A"), ExtensionFields::empty().unwrap())
            .unwrap();
        let cancel = send(&client, cancel);
        finish(&cancel).await;
        let close = strategy
            .close(sid("A"), ExtensionFields::empty().unwrap())
            .unwrap();
        let close = client
            .reserve_request(close, ResponseDeadline::At(deadline()))
            .unwrap();
        let close = send(&client, close);
        finish(&close).await;
        let closed = receiver.next().await.unwrap().unwrap();
        assert!(
            matches!(closed.incoming(),Incoming::Response{response:Response::Closed{session},..}if session.as_str()=="A")
        );
        assert!(owner.shutdown(deadline()).await.joined);
        drop(client);
        drop(receiver);
        drop(owner);
        let report = process.shutdown(deadline()).await.unwrap();
        assert_eq!(report.active_processes, 0);
        assert_ne!(report.disposition, ShutdownDisposition::Incomplete);
        assert!(
            timeout(Duration::from_secs(20), drain)
                .await
                .unwrap()
                .unwrap()
                .is_empty()
        );
    }
}

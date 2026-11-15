//! Dedicated owned test child. Exit93 is intentional at a real registration/owner boundary.
#[allow(
    dead_code,
    reason = "The crash child needs only the shared trusted ports and SQL wrapper."
)]
mod support;
use acp_remote::{application::request_registry::ValidatedMutation, storage::Storage};
use std::{io::Write, path::Path};
use support::{
    A, Action, Audit, Authority, Clock, Control, Environment, Gate, Point, Repository, T1,
    Validator, submit,
};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 3);
    let path = Path::new(&args[1]);
    let stage = &args[2];
    let storage = Storage::open(path).await.unwrap();
    let control = Control::new();
    let wrapped = control.clone();
    let (service, _) = storage
        .start_queue_service_with_repository_and_audit_for_fixture(
            Authority::new(),
            Validator,
            Clock::new(),
            std::sync::Arc::new(Environment::new()),
            Audit::new(),
            move |inner| Ok(Repository::new(inner, wrapped)),
        )
        .await
        .unwrap();
    let mutation = ValidatedMutation::Submit(
        service
            .validate_body(b"CRASH_BODY_CANARY_12be951a")
            .unwrap(),
    );
    let registered = service.register(&A, T1, &mutation).await.unwrap();
    println!("CRASH_REQUEST_ID={}", registered.id().encode());
    std::io::stdout().flush().unwrap();
    if stage == "registered" {
        std::process::exit(93);
    }
    if stage == "settled" {
        support::resolved(service.consume(&A, T1, registered.id(), mutation).await);
        std::process::exit(93);
    }
    let point = match stage.as_str() {
        "begin_committed" => Point::BeginAfter,
        "applied" => Point::SettleBefore,
        "settle_committed" => Point::SettleAfter,
        _ => panic!("unknown crash fixture stage"),
    };
    let gate = Gate::new();
    control.arm(point, Action::Pause(gate.clone()));
    let request = registered.id();
    let for_consume = service.clone();
    let _owned = tokio::spawn(async move {
        let body = submit(&for_consume, b"CRASH_BODY_CANARY_12be951a");
        for_consume.consume(&A, T1, request, body).await
    });
    gate.wait().await;
    println!("CRASH_STAGE={stage}");
    std::io::stdout().flush().unwrap();
    std::process::exit(93);
}

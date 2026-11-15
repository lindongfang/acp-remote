//! P1-CODE-001: original contract ordering, not an altered product expectation.
use super::*;

#[tokio::test]
async fn p1_code_001_other_session_must_ignore_before_raw_consistency() {
    let (mut client, mut peer, strategy, _) = ready(Version::V2).await;
    let (mut windows, h) = window(ViewLimits::default(), "session-A");
    let mut read = ObservationRead::begin(&mut windows, &h, &strategy).unwrap();
    let incoming = value(&mut client, &mut peer, &update("session-B", 1)).await;
    let stats = read.stats();
    let usage = windows.usage();
    let result = read.observe(&mut windows, &incoming);
    println!("P1-CODE-001 other-session actual={result:?}");
    assert_eq!(result, Ok(ObservationOutcome::Ignored));
    assert_eq!(read.stats(), stats);
    assert_eq!(windows.usage(), usage);
}

#[tokio::test]
async fn p1_code_001_v2_state_must_ignore_before_raw_consistency() {
    let (mut client, mut peer, strategy, _) = ready(Version::V2).await;
    let (mut windows, h) = window(ViewLimits::default(), "session-A");
    let mut read = ObservationRead::begin(&mut windows, &h, &strategy).unwrap();
    let incoming=value(&mut client,&mut peer,&json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"session-A","update":{"sessionUpdate":"state_update","state":"idle"}}})).await;
    let stats = read.stats();
    let usage = windows.usage();
    let result = read.observe(&mut windows, &incoming);
    println!("P1-CODE-001 state-first actual={result:?}");
    assert_eq!(result, Ok(ObservationOutcome::Ignored));
    assert_eq!(read.stats(), stats);
    assert_eq!(windows.usage(), usage);
}

#[tokio::test]
async fn p1_code_001_stale_matching_must_precede_raw_consistency() {
    let (mut client, mut peer, strategy, _) = ready(Version::V2).await;
    let (mut windows, h) = window(ViewLimits::default(), "session-A");
    let mut read = ObservationRead::begin(&mut windows, &h, &strategy).unwrap();
    let incoming = value(&mut client, &mut peer, &update("session-A", 1)).await;
    windows.release(&h).unwrap();
    let next = windows.begin(native("session-A"), epoch(4)).unwrap();
    let stats = read.stats();
    let usage = windows.usage();
    let result = read.observe(&mut windows, &incoming);
    println!("P1-CODE-001 stale-first actual={result:?}");
    assert_eq!(result, Err(ObservationError::View(ViewError::Stale)));
    assert_eq!(read.stats(), stats);
    assert_eq!(windows.usage(), usage);
    assert_eq!(
        windows.window_status(&next).unwrap().state,
        ViewState::Reading
    );
}

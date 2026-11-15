//! Completion-container routing only; no fake `SourceReadCapability` or Ready state.
use super::*;
use crate::adapters::acp::native_dispatch::AcpNativeAdapter;
use crate::domain::native_dispatch::NativeProtocolFailure;

fn empty() -> NativeReadAcquisition<AcpNativeAdapter> {
    NativeReadAcquisition {
        state: Arc::new(ReadAcquisitionState::new()),
    }
}

#[cfg(all(windows, feature = "native-dispatch-dev-fixtures"))]
pub(in crate::application::native_dispatch) async fn completed<
    P: crate::application::native_dispatch::NativeApprovalPort,
>(
    acquisition: &NativeReadAcquisition<P>,
) -> Result<(), DispatchError> {
    let mut changes = acquisition.state.changes.subscribe();
    loop {
        let result = *acquisition.state.completed.lock().unwrap();
        if let Some(result) = result {
            return result;
        }
        changes.changed().await.unwrap();
    }
}

#[tokio::test]
async fn native_cause_completed_errors_preserve_original_dispatch_value() {
    for error in [
        DispatchError::ProtocolFault(NativeProtocolFailure::Io),
        DispatchError::ProtocolFault(NativeProtocolFailure::UnsupportedCapability),
        DispatchError::InvalidConfiguration,
        DispatchError::ProtocolRejected,
        DispatchError::SourcePlatform {
            operation: "synthetic-static-operation",
            code: 5,
        },
    ] {
        let acquisition = empty();
        acquisition.state.finish(Err(error));
        assert_eq!(
            acquisition.source_capability().await.err(),
            Some(ViewError::NativeFailure(error))
        );
    }
}

#[tokio::test]
async fn native_cause_completed_ok_without_capability_still_is_unavailable() {
    let acquisition = empty();
    acquisition.state.finish(Ok(()));
    assert_eq!(
        acquisition.source_capability().await.err(),
        Some(ViewError::CaptureUnavailable)
    );
}

#[tokio::test]
async fn native_cause_unfinished_wait_can_be_cancelled_without_fabricating_completion() {
    let acquisition = empty();
    {
        let waiting = acquisition.source_capability();
        tokio::pin!(waiting);
        tokio::select! {
            biased;
            _ = &mut waiting => panic!("unfinished container must remain pending"),
            () = tokio::task::yield_now() => {}
        }
    }
    assert!(acquisition.state.completed.lock().unwrap().is_none());
    let original = DispatchError::ProtocolFault(NativeProtocolFailure::Deadline);
    acquisition.state.finish(Err(original));
    assert_eq!(
        acquisition.source_capability().await.err(),
        Some(ViewError::NativeFailure(original))
    );
}

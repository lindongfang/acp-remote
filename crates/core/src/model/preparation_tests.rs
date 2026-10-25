//! 准备运行期类型与暂存主体绑定的纯测试。

use super::*;
use crate::model::{AgentId, AttachmentSubject, Digest, NodeId, Nonce};

fn nonce(seed: &str) -> Nonce {
    Nonce::new(seed).expect("nonce")
}

fn uuid(seed: u8) -> String {
    format!(
        "{seed:02x}{seed:02x}{seed:02x}{seed:02x}-1111-4111-8111-{seed:02x}{seed:02x}{seed:02x}{seed:02x}{seed:02x}{seed:02x}"
    )
}

fn binding() -> PreparationBinding {
    PreparationBinding {
        device: DeviceId::new(&uuid(1)).expect("device"),
        session: SessionId::new(&uuid(2)).expect("session"),
        tab_nonce: nonce(&"A".repeat(43)),
        runtime_epoch: RuntimeEpoch::new(&uuid(3)).expect("runtime epoch"),
        generation: ResourceGeneration::new(1),
        claims_digest: Digest::new(&"E".repeat(43)).expect("digest"),
    }
}

#[test]
fn preparation_states_are_closed_and_only_released_frees_capacity() {
    for state in PreparationState::ALL {
        assert_eq!(
            state.occupies_capacity(),
            state != PreparationState::Released
        );
    }
    let view = PreparationView {
        session: SessionId::new(&uuid(4)).expect("session"),
        runtime_epoch: RuntimeEpoch::new(&uuid(5)).expect("runtime epoch"),
        generation: ResourceGeneration::new(2),
        state: PreparationState::Ready,
        agent: AgentRef::try_new(AgentId::new("codex").expect("agent"), "Codex").expect("agent"),
        directory: None,
        owner: PreparationOwner::new(
            DeviceId::new(&uuid(6)).expect("device"),
            nonce(&"I".repeat(43)),
        ),
        lease_expires_at: None,
        failure: None,
    };
    assert!(view.is_ready());
    assert!(view.state.occupies_capacity());
}

#[test]
fn failures_and_stale_reasons_have_stable_tokens() {
    let reasons: Vec<&str> = PreparationFailureReason::ALL
        .iter()
        .map(|reason| reason.as_str())
        .collect();
    assert_eq!(
        reasons,
        vec![
            "endpoint_unavailable",
            "configuration_rejected",
            "lease_expired",
            "capacity_exceeded",
            "release_uncertain",
        ]
    );
    assert_eq!(
        PreparationStaleReason::RuntimeEpochMismatch.as_str(),
        "runtime_epoch_mismatch"
    );
    assert_eq!(PreparationStaleReason::Released.as_str(), "released");
}

#[test]
fn staging_accepts_only_the_bound_device_subject() {
    let binding = binding();
    assert!(
        require_device_subject(&AttachmentSubject::Device(binding.device.clone()), &binding)
            .is_ok()
    );
    assert!(
        require_device_subject(
            &AttachmentSubject::Device(DeviceId::new(&uuid(9)).expect("device")),
            &binding
        )
        .is_err()
    );
    assert!(
        require_device_subject(
            &AttachmentSubject::Node {
                access_node: NodeId::new(&uuid(8)).expect("node")
            },
            &binding
        )
        .is_err(),
        "准备暂存只接受设备主体"
    );
}

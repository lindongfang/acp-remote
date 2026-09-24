//! [PV3] R1–R29 / 任务 2.5：配对状态机的可观察行为。
//!
//! 每个用例用「新建 `Authority` + 同一组端口」表达进程重启，用定序熵源表达可重跑，
//! 用注入时钟表达过期，因此没有睡眠、没有系统时间、没有数据库。

mod support;

use acp_core::model::{NodeKind, PairingState, PeerIdentity};
use identity_auth::{
    CanonicalOrigin, ClaimKindFields, ClaimOutcome, ClaimRejection, ClaimedPairing, ClientKind,
    PairingDecision, PairingError, PairingProof, PairingSpec, RequestedCapabilities,
};

use support::{
    DEVICE, FakeClock, FakeKeystore, HOST, ORIGIN, OTHER_DEVICE, PAIRING, PeerKey, SequenceEntropy,
    authority, device, grants, node, nonce_from_hex, pairing, scopes, ts,
};

/// 基准时刻之后的若干秒（只用于过期类用例；状态机不读系统时间）。
const CREATED: &str = "2026-01-01T00:00:00.000Z";
const WITHIN_WINDOW: &str = "2026-01-01T00:04:59.000Z";
const AFTER_WINDOW: &str = "2026-01-01T00:05:00.000Z";

fn device_spec() -> PairingSpec {
    PairingSpec::Device {
        canonical_origin: CanonicalOrigin::parse(ORIGIN).expect("测试用 origin 必须规范"),
    }
}

fn device_request() -> RequestedCapabilities {
    RequestedCapabilities {
        scopes: scopes(&["session.list", "session.read"]),
        grants: grants(&[]),
    }
}

/// 一个已创建的设备配对及其内存 secret。
struct Created {
    authority: identity_auth::Authority,
    keystore: std::sync::Arc<FakeKeystore>,
    entropy: std::sync::Arc<SequenceEntropy>,
    clock: std::sync::Arc<FakeClock>,
    draft: identity_auth::PairingDraft,
}

fn create_device_pairing() -> Created {
    let keystore = FakeKeystore::new();
    let entropy = SequenceEntropy::new();
    let clock = FakeClock::new();
    let state_machine = authority(&keystore, &entropy, &clock);
    let draft = state_machine
        .begin_pairing(
            &pairing(PAIRING),
            &device_spec(),
            &device_request(),
            Some("Test Phone"),
            &ts(CREATED),
            &ts(WITHIN_WINDOW),
        )
        .expect("创建配对必须成功");
    Created {
        authority: state_machine,
        keystore,
        entropy,
        clock,
        draft,
    }
}

/// 构造一份合法认领（对端是「另一个实现」：用自己的 secret 与私钥算 proof）。
fn device_claim(
    created: &Created,
    peer: &PeerKey,
    client_nonce_hex: &str,
    requested: RequestedCapabilities,
) -> identity_auth::ClaimFields {
    let client_nonce = nonce_from_hex(client_nonce_hex);
    let mut fields = identity_auth::ClaimFields {
        pairing: pairing(PAIRING),
        peer: PeerIdentity::Device(device(DEVICE)),
        kind: ClaimKindFields::Device {
            client_kind: ClientKind::Pwa,
        },
        display_name: "Test Phone".to_owned(),
        host_binding: ORIGIN.to_owned(),
        public_key: peer.public_key(),
        client_nonce,
        requested,
        proof: PairingProof::try_from_bytes(&[0u8; 32]).expect("32 字节"),
    };
    let transcript = identity_auth::transcript::SyncPairingProof {
        host_id: node(HOST),
        device_id: device(DEVICE),
        pairing_id: pairing(PAIRING),
        pairing_expires_at: created.draft.record.expires_at().clone(),
        canonical_origin: CanonicalOrigin::parse(ORIGIN).expect("测试用 origin 必须规范"),
        device_public_key: fields.public_key.clone(),
        client_nonce: fields.client_nonce.clone(),
        device_name: fields.display_name.clone(),
        client_kind: ClientKind::Pwa,
    }
    .transcript()
    .expect("装配必须成功");
    fields.proof = support::client_hmac(&created.draft.secret, &transcript);
    fields
}

#[test]
fn device_pairing_accepts_only_scopes() {
    // R1 / 场景「设备配对只接受 scope」：带 grants 的创建被拒，且不产生记录。
    let keystore = FakeKeystore::new();
    let entropy = SequenceEntropy::new();
    let clock = FakeClock::new();
    let authority = authority(&keystore, &entropy, &clock);
    let both = RequestedCapabilities {
        scopes: scopes(&["session.list"]),
        grants: grants(&["grant.observe"]),
    };
    assert_eq!(
        authority.begin_pairing(
            &pairing(PAIRING),
            &device_spec(),
            &both,
            None,
            &ts(CREATED),
            &ts(WITHIN_WINDOW)
        ),
        Err(PairingError::CapabilityKindMismatch)
    );
    assert!(
        !authority.has_secret(&pairing(PAIRING)),
        "被拒的创建不留内存状态"
    );
}

#[test]
fn node_pairing_accepts_only_grants() {
    // R2 / 场景「节点配对只接受 grant」。
    let keystore = FakeKeystore::new();
    let entropy = SequenceEntropy::new();
    let clock = FakeClock::new();
    let authority = authority(&keystore, &entropy, &clock);
    let spec = PairingSpec::Node {
        endpoint: identity_auth::NodeEndpoint::parse(
            "wss://owner.example.ts.net:8443/node-link/v1",
        )
        .expect("测试用 endpoint 必须规范"),
        kind: NodeKind::Access,
    };
    let both = RequestedCapabilities {
        scopes: scopes(&["session.list"]),
        grants: grants(&["grant.observe"]),
    };
    assert_eq!(
        authority.begin_pairing(
            &pairing(PAIRING),
            &spec,
            &both,
            None,
            &ts(CREATED),
            &ts(WITHIN_WINDOW)
        ),
        Err(PairingError::CapabilityKindMismatch)
    );
}

#[test]
fn pairing_window_can_only_shrink() {
    // R3 / 场景「有效期只能收窄」。
    let keystore = FakeKeystore::new();
    let entropy = SequenceEntropy::new();
    let clock = FakeClock::new();
    let authority = authority(&keystore, &entropy, &clock);
    let short = authority
        .begin_pairing(
            &pairing(PAIRING),
            &device_spec(),
            &device_request(),
            None,
            &ts(CREATED),
            &ts("2026-01-01T00:01:00.000Z"),
        )
        .expect("收窄到 1 分钟必须成功");
    assert_eq!(
        short.record.expires_at().as_str(),
        "2026-01-01T00:01:00.000Z"
    );
    assert_eq!(
        authority.begin_pairing(
            &pairing(PAIRING),
            &device_spec(),
            &device_request(),
            None,
            &ts(CREATED),
            &ts("2026-01-01T00:06:00.000Z")
        ),
        Err(PairingError::InvalidWindow),
        "超过 5 分钟必须被拒，而不是静默截断"
    );
}

#[test]
fn status_hides_peer_before_claim() {
    // R4/R19 / 场景「创建后看不到其他设备的任何信息」「未认领前不展示 SAS」。
    let created = create_device_pairing();
    let authority = &created.authority;
    let view = authority.pairing_status(&created.draft.record, None, None);
    assert_eq!(view.state, PairingState::Created);
    assert!(view.display_name.is_none());
    assert!(view.public_key_fingerprint.is_none());
    assert!(view.sas.is_none(), "没有占位 SAS");
    assert!(view.requested.is_none());
    assert!(view.peer.is_none());
}

#[test]
fn binding_mismatch_is_rejected() {
    // R5 / 场景「绑定不一致的认领被拒」。
    let created = create_device_pairing();
    let authority = &created.authority;
    let peer = PeerKey::new();
    let mut fields = device_claim(&created, &peer, "00", device_request());
    fields.host_binding = "https://evil.example.test".to_owned();
    assert_eq!(
        authority.verify_claim(&created.draft.record, None, &fields),
        Ok(ClaimOutcome::Rejected(ClaimRejection::BindingMismatch))
    );
    // 认领失败不改变记录状态，也不写任何对端。
    assert_eq!(created.draft.record.state(), PairingState::Created);
}

#[test]
fn malformed_claim_is_rejected_before_crypto() {
    // R6 / 场景「结构错误先于密码学被拒」。
    let created = create_device_pairing();
    let authority = &created.authority;
    let peer = PeerKey::new();
    let mut fields = device_claim(&created, &peer, "00", device_request());
    fields.kind = ClaimKindFields::Node {
        node_kind: NodeKind::Access,
    };
    assert_eq!(
        authority.verify_claim(&created.draft.record, None, &fields),
        Ok(ClaimOutcome::Rejected(ClaimRejection::Malformed))
    );
    // 目标族与标识不一致同样是结构错误。
    fields.kind = ClaimKindFields::Device {
        client_kind: ClientKind::Pwa,
    };
    fields.pairing = pairing("11111111-2222-4333-8444-555555555555");
    assert_eq!(
        authority.verify_claim(&created.draft.record, None, &fields),
        Ok(ClaimOutcome::Rejected(ClaimRejection::Malformed))
    );
}

#[test]
fn first_claim_wins_and_second_conflicts() {
    // R7 / 场景「并发认领只有一个成功」。
    let created = create_device_pairing();
    let authority = &created.authority;
    let first = PeerKey::new();
    let second =
        PeerKey::from_seed("0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c");
    let claim = device_claim(&created, &first, "00", device_request());
    let outcome = authority
        .verify_claim(&created.draft.record, None, &claim)
        .expect("认领校验不返回内部错误");
    let ClaimOutcome::Claimed(claimed) = outcome else {
        panic!("首次认领必须成功：{outcome:?}");
    };
    assert_eq!(claimed.peer, PeerIdentity::Device(device(DEVICE)));

    // 第二个对端（不同公钥/nonce）带着「已有对端」再次认领 → 冲突，绝不改写首次对端。
    let mut other = device_claim(&created, &second, "01", device_request());
    other.peer = PeerIdentity::Device(device(OTHER_DEVICE));
    assert_eq!(
        authority.verify_claim(&created.draft.record, Some(&claimed), &other),
        Ok(ClaimOutcome::Rejected(ClaimRejection::NotClaimable))
    );
}

#[test]
fn identical_claim_is_idempotent() {
    // R8 / 场景「相同认领内容幂等」。
    let created = create_device_pairing();
    let authority = &created.authority;
    let peer = PeerKey::new();
    let claim = device_claim(&created, &peer, "00", device_request());
    let ClaimOutcome::Claimed(first) = authority
        .verify_claim(&created.draft.record, None, &claim)
        .expect("首次认领必须成功")
    else {
        panic!("首次认领必须成功");
    };
    let repeat = authority
        .verify_claim(&created.draft.record, Some(&first), &claim)
        .expect("重发不返回内部错误");
    assert_eq!(repeat, ClaimOutcome::Repeat(Box::new((*first).clone())));
}

#[test]
fn claimed_public_key_round_trips_into_core_claim() {
    // R9 / 场景「认领携带公钥并在重启后可读」：认领结果带 65 字节公钥，可构造 core 的 `PairingClaim`。
    let created = create_device_pairing();
    let authority = &created.authority;
    let peer = PeerKey::new();
    let claim = device_claim(&created, &peer, "00", device_request());
    let ClaimOutcome::Claimed(claimed) = authority
        .verify_claim(&created.draft.record, None, &claim)
        .expect("首次认领必须成功")
    else {
        panic!("首次认领必须成功");
    };
    let core_claim = claimed.to_claim().expect("必须能构造 core 的 PairingClaim");
    assert_eq!(
        core_claim.peer().public_key().as_bytes(),
        peer.public_key().as_bytes(),
        "落库的公钥必须与认领提交的 65 字节逐字节相同"
    );
}

#[test]
fn request_beyond_registered_is_rejected() {
    // 认领请求集合超出登记值 → 拒绝（不部分接受）。
    let created = create_device_pairing();
    let authority = &created.authority;
    let peer = PeerKey::new();
    let wider = RequestedCapabilities {
        scopes: scopes(&["session.list", "session.prompt"]),
        grants: grants(&[]),
    };
    let claim = device_claim(&created, &peer, "00", wider);
    assert_eq!(
        authority.verify_claim(&created.draft.record, None, &claim),
        Ok(ClaimOutcome::Rejected(
            ClaimRejection::CapabilitiesExceedRegistered
        ))
    );
}

#[test]
fn approve_requires_pending_and_within_requested() {
    // R10/R16 / 场景「批准后同时存在信任与身份材料」「最终集合不能超出请求集合」。
    let created = create_device_pairing();
    let authority = &created.authority;
    let peer = PeerKey::new();
    let claim = device_claim(&created, &peer, "00", device_request());
    let ClaimOutcome::Claimed(claimed) = authority
        .verify_claim(&created.draft.record, None, &claim)
        .expect("首次认领必须成功")
    else {
        panic!("首次认领必须成功");
    };
    // 尚未推进状态时不能批准（状态检查先于集合检查）。
    assert_eq!(
        authority.settle(
            &created.draft.record,
            &PairingDecision::Approve {
                granted_scopes: scopes(&["session.list"]),
                granted_grants: grants(&[]),
            },
            &ts(CREATED)
        ),
        Err(PairingError::WrongState)
    );
    let pending = pending_record(&created.draft.record);
    // 最终集合超出请求值 → 拒绝。
    assert_eq!(
        authority.settle(
            &pending,
            &PairingDecision::Approve {
                granted_scopes: scopes(&["session.list", "session.read", "session.cancel"]),
                granted_grants: grants(&[]),
            },
            &ts(CREATED)
        ),
        Err(PairingError::CapabilitiesExceedRequested)
    );
    // 设备配对提交 grant → 拒绝（方向错误）。
    assert_eq!(
        authority.settle(
            &pending,
            &PairingDecision::Approve {
                granted_scopes: scopes(&["session.list"]),
                granted_grants: grants(&["grant.observe"]),
            },
            &ts(CREATED)
        ),
        Err(PairingError::CapabilityKindMismatch)
    );
    let settlement = authority
        .settle(
            &pending,
            &PairingDecision::Approve {
                granted_scopes: scopes(&["session.list"]),
                granted_grants: grants(&[]),
            },
            &ts(CREATED),
        )
        .expect("在请求集合内批准必须成功");
    let acp_core::model::PairingSettlement::Approved { granted_scopes, .. } = settlement else {
        panic!("必须返回批准结果");
    };
    assert_eq!(
        granted_scopes.iter().collect::<Vec<_>>(),
        vec!["session.list"]
    );
    assert!(
        !authority.has_secret(&pairing(PAIRING)),
        "批准后 secret 立即清除"
    );
    assert_eq!(claimed.display_name, "Test Phone");
}

/// 认领后（`pending_confirmation`）的记录：状态推进由存储层提交，这里按同样形状构造。
fn pending_record(record: &acp_core::model::PairingRecord) -> acp_core::model::PairingRecord {
    acp_core::model::PairingRecord::try_new(
        record.id().clone(),
        record.target(),
        PairingState::PendingConfirmation,
        record.display_name().map(str::to_owned),
        record.requested_scopes().clone(),
        record.requested_grants().clone(),
        record.secret_digest().clone(),
        record.host_binding(),
        record.created_at().clone(),
        record.expires_at().clone(),
        Some(ts(CREATED)),
        None,
        None,
    )
    .expect("测试记录必须合法")
}

#[test]
fn reject_produces_no_trust() {
    // R11 / 场景「拒绝不产生信任」。
    let created = create_device_pairing();
    let authority = &created.authority;
    let settlement = authority
        .settle(
            &created.draft.record,
            &PairingDecision::Reject {
                reason: Some("user denied".to_owned()),
            },
            &ts(CREATED),
        )
        .expect("拒绝必须成功");
    let acp_core::model::PairingSettlement::Rejected { reason } = settlement else {
        panic!("必须返回拒绝结果");
    };
    assert_eq!(reason.as_deref(), Some("user denied"));
    assert!(
        !authority.has_secret(&pairing(PAIRING)),
        "拒绝后 secret 清除"
    );
}

#[test]
fn expiry_blocks_settlement() {
    // R12 / 场景「过期不产生信任」。
    let created = create_device_pairing();
    let authority = &created.authority;
    let pending = pending_record(&created.draft.record);
    created.clock.set(AFTER_WINDOW);
    assert_eq!(
        authority.settle(
            &pending,
            &PairingDecision::Approve {
                granted_scopes: scopes(&["session.list"]),
                granted_grants: grants(&[]),
            },
            &ts(AFTER_WINDOW)
        ),
        Err(PairingError::Expired)
    );
}

#[test]
fn sas_matches_fixed_vector() {
    // R17/R18 / 场景「同一输入得到同一 6 位结果」：与固定向量的期望值一致。
    // 向量 `fixtures/sync/v1/transcripts/pairing-sas.json` 的输入。
    let document: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/sync/v1/transcripts/pairing-sas.json"
    ))
    .expect("固定向量必须是合法 JSON");
    let input = &document["input"];
    let secret = acpr_transcript::decode_base64url(
        document["expected"]["hmacKeyBase64url"]
            .as_str()
            .expect("向量必须有 key"),
    )
    .expect("规范 base64url");
    let secret = identity_auth::PairingSecret::try_from_bytes(&secret).expect("32 字节 secret");
    let transcript = identity_auth::transcript::SyncPairingSas {
        host_id: support::node(input["hostId"].as_str().unwrap()),
        device_id: support::device(input["deviceId"].as_str().unwrap()),
        pairing_id: support::pairing(input["pairingId"].as_str().unwrap()),
        canonical_origin: CanonicalOrigin::parse(input["canonicalOrigin"].as_str().unwrap())
            .expect("规范 origin"),
        host_public_key: support::key_from_base64url(input["hostPublicKey"].as_str().unwrap()),
        device_public_key: support::key_from_base64url(input["devicePublicKey"].as_str().unwrap()),
        client_nonce: support::nonce_from_hex(input["clientNonceHex"].as_str().unwrap()),
        server_nonce: support::nonce_from_hex(input["serverNonceHex"].as_str().unwrap()),
        pairing_request_id: support::request_id(input["pairingRequestId"].as_str().unwrap()),
    }
    .transcript()
    .expect("装配必须成功");
    let sas = identity_auth::derive_sas(&transcript, &secret).expect("SAS 派生必须成功");
    assert_eq!(sas.as_str(), document["expected"]["sas"].as_str().unwrap());
    // 同输入重算必须得到同一结果（双方各自计算的前提）。
    assert_eq!(
        identity_auth::derive_sas(&transcript, &secret).expect("派生"),
        sas
    );
}

#[test]
fn restart_terminates_unconfirmed_pairings() {
    // R20/R21 / 场景「摘要不能恢复 secret」：重启后 secret 不在内存，认领只能失败。
    let created = create_device_pairing();
    let peer = PeerKey::new();
    let claim = device_claim(&created, &peer, "00", device_request());
    // 旧进程：认领成功（secret 就在这个进程的内存里）。
    let before = &created.authority;
    assert!(matches!(
        before
            .verify_claim(&created.draft.record, None, &claim)
            .expect("认领校验"),
        ClaimOutcome::Claimed(_)
    ));
    // 新进程：同一端口、同一记录，但状态是空的。
    let after = authority(&created.keystore, &created.entropy, &created.clock);
    assert_eq!(
        after.verify_claim(&created.draft.record, None, &claim),
        Ok(ClaimOutcome::Rejected(ClaimRejection::NotClaimable)),
        "重启后无内存 secret，认领必然失败"
    );
    assert_eq!(
        after.unrecoverable_after_restart(std::slice::from_ref(&created.draft.record)),
        vec![pairing(PAIRING)],
        "启动时必须终结未确认的配对"
    );
}

#[test]
fn approved_pairing_is_not_terminated_after_restart() {
    // R20 / 场景「批准后提前清除」+「已批准的信任记录保留」。
    let created = create_device_pairing();
    let authority = &created.authority;
    let pending = pending_record(&created.draft.record);
    authority
        .settle(
            &pending,
            &PairingDecision::Approve {
                granted_scopes: scopes(&["session.list"]),
                granted_grants: grants(&[]),
            },
            &ts(CREATED),
        )
        .expect("批准必须成功");
    // 已批准（终态）的配对不在「重启后不可恢复」集合里。
    let approved = approved_record(&pending);
    assert!(
        authority
            .unrecoverable_after_restart(&[approved])
            .is_empty()
    );
}

fn approved_record(record: &acp_core::model::PairingRecord) -> acp_core::model::PairingRecord {
    acp_core::model::PairingRecord::try_new(
        record.id().clone(),
        record.target(),
        PairingState::Approved,
        record.display_name().map(str::to_owned),
        record.requested_scopes().clone(),
        record.requested_grants().clone(),
        record.secret_digest().clone(),
        record.host_binding(),
        record.created_at().clone(),
        record.expires_at().clone(),
        Some(ts(CREATED)),
        Some(ts(CREATED)),
        None,
    )
    .expect("测试记录必须合法")
}

#[test]
fn rejection_survives_until_original_expiry() {
    // R23 / 场景「拒绝保留到原过期时间」：拒绝态在过期前仍可查询，过期后按 `Expired` 返回。
    let created = create_device_pairing();
    let authority = &created.authority;
    let rejected = acp_core::model::PairingRecord::try_new(
        created.draft.record.id().clone(),
        created.draft.record.target(),
        PairingState::Rejected,
        Some("Test Phone".to_owned()),
        created.draft.record.requested_scopes().clone(),
        created.draft.record.requested_grants().clone(),
        created.draft.record.secret_digest().clone(),
        created.draft.record.host_binding(),
        created.draft.record.created_at().clone(),
        created.draft.record.expires_at().clone(),
        Some(ts(CREATED)),
        None,
        Some(ts(CREATED)),
    )
    .expect("测试记录必须合法");
    assert_eq!(
        authority.pairing_status(&rejected, None, None).state,
        PairingState::Rejected
    );
    created.clock.set(AFTER_WINDOW);
    assert_eq!(
        authority.pairing_status(&rejected, None, None).state,
        PairingState::Expired,
        "到期后不再表现为有效状态"
    );
    // 拒绝态已是终态，因此不在「待终结」集合里（该集合只收未终结的配对）。
    assert!(
        authority
            .due_pairings(std::slice::from_ref(&rejected), &ts(AFTER_WINDOW))
            .is_empty()
    );
}

#[test]
fn fifth_failure_invalidates_pairing() {
    // R24 / 场景「第 5 次失败后配对失效」。
    let created = create_device_pairing();
    let authority = &created.authority;
    let peer = PeerKey::new();
    let claim = device_claim(&created, &peer, "00", device_request());
    let mut wrong = claim.clone();
    wrong.proof = PairingProof::try_from_bytes(&[9u8; 32]).expect("32 字节");
    for attempt in 1..=4 {
        assert_eq!(
            authority.verify_claim(&created.draft.record, None, &wrong),
            Ok(ClaimOutcome::Rejected(ClaimRejection::ProofInvalid)),
            "第 {attempt} 次失败是普通 proof 失败"
        );
        assert_eq!(authority.failure_count(&pairing(PAIRING)), attempt);
    }
    assert_eq!(
        authority.verify_claim(&created.draft.record, None, &wrong),
        Ok(ClaimOutcome::Rejected(ClaimRejection::TooManyFailures)),
        "第 5 次失败必须让配对失效并给出可落库的拒绝原因"
    );
    // 失效后即使载荷完全正确也不能再认领。
    assert_eq!(
        authority.verify_claim(&created.draft.record, None, &claim),
        Ok(ClaimOutcome::Rejected(ClaimRejection::NotClaimable))
    );
}

#[test]
fn all_rejections_share_one_public_class() {
    // R25 / 场景「失败不泄漏存在性」：本地细分不同，对端可见分类唯一。
    for rejection in [
        ClaimRejection::Malformed,
        ClaimRejection::NotClaimable,
        ClaimRejection::Expired,
        ClaimRejection::BindingMismatch,
        ClaimRejection::CapabilitiesExceedRegistered,
        ClaimRejection::ProofInvalid,
        ClaimRejection::TooManyFailures,
    ] {
        assert_eq!(
            rejection.public_class(),
            identity_auth::ClaimFailureClass::AuthenticationFailed
        );
    }
}

#[test]
fn claim_never_rebinds_existing_peer() {
    // R26–R29 / 场景「撤销后普通写入不能复活」「公钥变化被拒绝」：已固定对端后任何普通写入都不改变绑定。
    let created = create_device_pairing();
    let authority = &created.authority;
    let peer = PeerKey::new();
    let claim = device_claim(&created, &peer, "00", device_request());
    let ClaimOutcome::Claimed(claimed) = authority
        .verify_claim(&created.draft.record, None, &claim)
        .expect("首次认领必须成功")
    else {
        panic!("首次认领必须成功");
    };
    // 同一设备换一把公钥（「身份材料变化」）→ 冲突，不自动替换。
    let replacement =
        PeerKey::from_seed("0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d");
    let mut changed = claim.clone();
    changed.public_key = replacement.public_key();
    assert_eq!(
        authority.verify_claim(&created.draft.record, Some(&claimed), &changed),
        Ok(ClaimOutcome::Rejected(ClaimRejection::NotClaimable))
    );
    // 已认领的固定值保持不变。
    assert_eq!(claimed.public_key, peer.public_key());
    let _: ClaimedPairing = (*claimed).clone();
}

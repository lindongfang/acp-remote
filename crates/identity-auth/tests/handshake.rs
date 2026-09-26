//! [PV3] R30–R52 / 任务 2.6：逐连接 challenge-response 的可观察行为。
//!
//! 覆盖点：未知对端也签发挑战、宿主证明可被本节点公钥验证、注入时钟的 15 秒窗口、挑战一次性与重放、
//! 验签公钥只来自持久化信任、high-S/low-S、零值分量、凭据状态不降级、收尾写集与失败关闭。
//!
//! 用例走 `support::block_on`（无 runtime 的最小 executor）：`hello` 唯一的 `await` 是 keystore 调用，
//! 测试里的 keystore 立即返回，因此不引入 Tokio 依赖。

mod support;

use acp_core::model::{AuditAction, NodeKind, PeerIdentity};
use identity_auth::{
    Authenticated, CanonicalOrigin, ChallengeIssue, ChallengeRequest, ConnectionBinding,
    ConnectionKind, CredentialStatus, FeatureList, HandshakeError, HandshakeFailureClass,
    IdentityFact, NodeEndpoint, P1363Signature, PeerTrust, ProofSubmission, SyncDeviceProof,
    SyncHostChallenge,
};

use support::{
    DEVICE, FakeClock, FakeKeystore, HOST, ORIGIN, PeerKey, SequenceEntropy, authority, block_on,
    challenge_id, device, node, nonce_from_hex, pairing, sync_result, ts,
};

const CREATED: &str = "2026-01-01T00:00:00.000Z";
const WITHIN_TTL: &str = "2026-01-01T00:00:14.000Z";
const AFTER_TTL: &str = "2026-01-01T00:00:16.000Z";
const EXPIRES_AT: &str = "2026-01-01T00:00:15.000Z";
const CONNECTION: &str = "2de54db7-74ae-4c21-88e3-05df0dc2e637";
const NODE: &str = "10111213-1415-4617-9819-1a1b1c1d1e1f";
const CLIENT_NONCE_HEX: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

struct Handshake {
    keystore: std::sync::Arc<FakeKeystore>,
    entropy: std::sync::Arc<SequenceEntropy>,
    clock: std::sync::Arc<FakeClock>,
    authority: identity_auth::Authority,
    peer: PeerKey,
}

fn setup() -> Handshake {
    let keystore = FakeKeystore::new();
    let entropy = SequenceEntropy::new();
    let clock = FakeClock::new();
    let authority = authority(&keystore, &entropy, &clock);
    Handshake {
        keystore,
        entropy,
        clock,
        authority,
        peer: PeerKey::new(),
    }
}

fn origin() -> CanonicalOrigin {
    CanonicalOrigin::parse(ORIGIN).expect("测试用 origin 必须规范")
}

fn binding() -> ConnectionBinding {
    ConnectionBinding {
        canonical_origin: Some(origin()),
        host: "work-pc.example.ts.net".to_owned(),
        endpoint: None,
    }
}

fn features() -> FeatureList {
    FeatureList::new(vec![
        "acp.raw-payload.v1".to_owned(),
        "core.command-status.v1".to_owned(),
    ])
}

fn client_nonce() -> acp_core::model::Nonce {
    nonce_from_hex(CLIENT_NONCE_HEX)
}

fn request() -> ChallengeRequest {
    ChallengeRequest {
        kind: ConnectionKind::SyncDevice,
        peer: PeerIdentity::Device(device(DEVICE)),
        binding: binding(),
        client_nonce: client_nonce(),
        negotiated_features: features(),
        catalog_revision: None,
    }
}

/// 持久化信任里的设备快照（公钥来自对端私钥，绝不出现在握手消息里）。
fn trust(peer: &PeerKey, credential: CredentialStatus) -> PeerTrust {
    PeerTrust {
        peer: PeerIdentity::Device(device(DEVICE)),
        public_key: Some(peer.public_key()),
        credential,
        host_binding: Some(ORIGIN.to_owned()),
        node_kind: None,
        scopes: support::scopes(&["session.list", "session.read"]),
        grants: support::grants(&[]),
    }
}

fn device_transcript(issue: &ChallengeIssue) -> Vec<u8> {
    SyncDeviceProof {
        host_id: node(HOST),
        device_id: device(DEVICE),
        canonical_origin: origin(),
        client_nonce: client_nonce(),
        server_nonce: issue.server_nonce.clone(),
        connection_id: issue.challenge_id.clone(),
        negotiated_features: features(),
    }
    .transcript()
    .expect("装配必须成功")
}

fn host_transcript(issue: &ChallengeIssue) -> SyncHostChallenge {
    SyncHostChallenge {
        host_id: node(HOST),
        device_id: device(DEVICE),
        canonical_origin: origin(),
        client_nonce: client_nonce(),
        server_nonce: issue.server_nonce.clone(),
        connection_id: issue.challenge_id.clone(),
        negotiated_features: features(),
    }
}

fn submission(issue: &ChallengeIssue, sign: impl Fn(&[u8]) -> P1363Signature) -> ProofSubmission {
    let signature = sign(&device_transcript(issue));
    ProofSubmission {
        kind: ConnectionKind::SyncDevice,
        challenge_id: issue.challenge_id.clone(),
        server_nonce: issue.server_nonce.clone(),
        peer: PeerIdentity::Device(device(DEVICE)),
        client_nonce: client_nonce(),
        signature,
    }
}

#[test]
fn unknown_peer_still_receives_a_challenge() {
    // R30 / 场景「未知对端也拿到挑战」。
    let state = setup();
    let unknown = PeerTrust::unknown(PeerIdentity::Device(device(DEVICE)));
    let issue = block_on(state.authority.hello(&request(), &unknown))
        .expect("未知对端必须拿到挑战，而不是「不存在」这类可区分错误");
    assert_eq!(issue.expires_at.as_str(), EXPIRES_AT, "挑战窗口固定 15 秒");
    assert!(issue.host_proof.as_bytes().iter().any(|byte| *byte != 0));
}

#[test]
fn host_proof_verifies_with_node_public_key() {
    // R31 / 场景「挑战携带本节点身份证明」：宿主证明用本节点公钥可验证。
    let state = setup();
    let issue = block_on(state.authority.hello(
        &request(),
        &PeerTrust::unknown(PeerIdentity::Device(device(DEVICE))),
    ))
    .expect("签发必须成功");
    // 公钥来自 keystore 端口（不是消息自带的自选公钥）。
    assert!(
        host_transcript(&issue)
            .verify(&state.keystore.node_public_key(), &issue.host_proof)
            .is_ok(),
        "宿主证明必须能用本节点公钥验证"
    );
    // 换一把公钥必须失败（证明与消息内容绑定）。
    let stranger =
        PeerKey::from_seed("0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f");
    assert!(
        host_transcript(&issue)
            .verify(&stranger.public_key(), &issue.host_proof)
            .is_err()
    );
}

#[test]
fn authentication_result_has_no_long_lived_credential() {
    // R33 场景「不引入长期会话凭据」：结果只有事实、凭据状态与连接绑定。
    let state = setup();
    let issue = block_on(
        state
            .authority
            .hello(&request(), &trust(&state.peer, CredentialStatus::Active)),
    )
    .expect("签发必须成功");
    let authenticated = sync_result(state.authority.verify_proof(
        &submission(&issue, |transcript| state.peer.sign(transcript)),
        &trust(&state.peer, CredentialStatus::Active),
    ))
    .expect("证明必须通过");
    let Authenticated {
        fact,
        credential,
        connection,
    } = authenticated;
    assert_eq!(credential, CredentialStatus::Active);
    assert_eq!(connection, binding());
    let IdentityFact::Device {
        device: subject, ..
    } = fact
    else {
        panic!("设备侧事实必须是设备主体");
    };
    assert_eq!(subject.as_str(), DEVICE);
}

#[test]
fn self_chosen_public_key_does_not_pass() {
    // R34 / 场景「自选公钥不能通过」。
    let state = setup();
    let stranger =
        PeerKey::from_seed("0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e");
    let issue = block_on(
        state
            .authority
            .hello(&request(), &trust(&state.peer, CredentialStatus::Active)),
    )
    .expect("签发必须成功");
    let failure = sync_result(state.authority.verify_proof(
        &submission(&issue, |transcript| stranger.sign(transcript)),
        &trust(&state.peer, CredentialStatus::Active),
    ))
    .expect_err("与持久化信任不一致的公钥签名必须失败");
    assert_eq!(
        failure.reason,
        HandshakeError::Proof(identity_auth::ProofError::Signature)
    );
    assert_eq!(
        failure.public_class(),
        HandshakeFailureClass::AuthenticationFailed
    );
    assert_eq!(failure.audit(), Some(AuditAction::DeviceAuthFailed));
}

#[test]
fn high_s_and_low_s_both_verify() {
    // R36 / 场景「high-S 与 low-S 都被接受」。
    let state = setup();
    let low = block_on(
        state
            .authority
            .hello(&request(), &trust(&state.peer, CredentialStatus::Active)),
    )
    .expect("签发必须成功");
    assert!(
        sync_result(state.authority.verify_proof(
            &submission(&low, |transcript| state.peer.sign(transcript)),
            &trust(&state.peer, CredentialStatus::Active),
        ))
        .is_ok(),
        "low-S 必须被接受"
    );
    let high = block_on(
        state
            .authority
            .hello(&request(), &trust(&state.peer, CredentialStatus::Active)),
    )
    .expect("签发必须成功");
    assert!(
        sync_result(state.authority.verify_proof(
            &submission(&high, |transcript| state.peer.sign_high_s(transcript)),
            &trust(&state.peer, CredentialStatus::Active),
        ))
        .is_ok(),
        "合法的 high-S 必须被接受（不强制 low-S 规范化）"
    );
}

#[test]
fn zero_component_signature_is_rejected() {
    // R37 / 场景「零值签名分量被拒绝」。
    let state = setup();
    let mut zero_r_bytes = [0u8; 64];
    zero_r_bytes[63] = 1;
    let zero_r = P1363Signature::try_from_bytes(&zero_r_bytes).expect("64 字节形状合法");
    assert!(!zero_r.components_nonzero(), "`r` 为零必须被判定");
    let issue = block_on(
        state
            .authority
            .hello(&request(), &trust(&state.peer, CredentialStatus::Active)),
    )
    .expect("签发必须成功");
    let mut submission = submission(&issue, |transcript| state.peer.sign(transcript));
    submission.signature = zero_r;
    let failure = sync_result(
        state
            .authority
            .verify_proof(&submission, &trust(&state.peer, CredentialStatus::Active)),
    )
    .expect_err("零值分量必须失败");
    assert_eq!(
        failure.reason,
        HandshakeError::Proof(identity_auth::ProofError::Signature)
    );
}

#[test]
fn challenge_is_consumed_once() {
    // R38/R39 / 场景「同一证明第二次使用失败」：消费先于验签，重放必然失败。
    let state = setup();
    let issue = block_on(
        state
            .authority
            .hello(&request(), &trust(&state.peer, CredentialStatus::Active)),
    )
    .expect("签发必须成功");
    let submission = submission(&issue, |transcript| state.peer.sign(transcript));
    assert!(
        sync_result(
            state
                .authority
                .verify_proof(&submission, &trust(&state.peer, CredentialStatus::Active))
        )
        .is_ok()
    );
    let replay = sync_result(
        state
            .authority
            .verify_proof(&submission, &trust(&state.peer, CredentialStatus::Active)),
    )
    .expect_err("重放必须失败");
    assert_eq!(replay.reason, HandshakeError::UnknownChallenge);
    assert_eq!(
        replay.audit(),
        Some(AuditAction::DeviceAuthFailed),
        "重放必须能映射到认证失败审计"
    );
}

#[test]
fn unknown_challenge_is_rejected() {
    // R40 / 场景「未知挑战被拒绝」。
    let state = setup();
    let submission = ProofSubmission {
        kind: ConnectionKind::SyncDevice,
        challenge_id: challenge_id(CONNECTION),
        server_nonce: nonce_from_hex("00"),
        peer: PeerIdentity::Device(device(DEVICE)),
        client_nonce: nonce_from_hex("01"),
        signature: P1363Signature::try_from_bytes(&[1u8; 64]).expect("64 字节"),
    };
    let failure = sync_result(
        state
            .authority
            .verify_proof(&submission, &trust(&state.peer, CredentialStatus::Active)),
    )
    .expect_err("未知挑战必须失败");
    assert_eq!(failure.reason, HandshakeError::UnknownChallenge);
    assert_eq!(
        failure.public_class(),
        HandshakeFailureClass::AuthenticationFailed
    );
}

#[test]
fn expiry_uses_injected_clock() {
    // R42 / 场景「过期由注入时钟决定」：无需等待真实时间。
    let state = setup();
    let issue = block_on(
        state
            .authority
            .hello(&request(), &trust(&state.peer, CredentialStatus::Active)),
    )
    .expect("签发必须成功");
    state.clock.set(WITHIN_TTL);
    assert!(
        sync_result(state.authority.verify_proof(
            &submission(&issue, |transcript| state.peer.sign(transcript)),
            &trust(&state.peer, CredentialStatus::Active)
        ))
        .is_ok()
    );
    // 清零时间后再签发：新挑战的窗口从签发时刻起算（固定 15 秒）。
    state.clock.set(CREATED);
    let issue = block_on(
        state
            .authority
            .hello(&request(), &trust(&state.peer, CredentialStatus::Active)),
    )
    .expect("签发必须成功");
    state.clock.set(AFTER_TTL);
    let failure = sync_result(state.authority.verify_proof(
        &submission(&issue, |transcript| state.peer.sign(transcript)),
        &trust(&state.peer, CredentialStatus::Active),
    ))
    .expect_err("超过 15 秒的挑战必须失败");
    assert_eq!(failure.reason, HandshakeError::UnknownChallenge);
}

#[test]
fn revoked_and_scope_reduced_are_not_authorization_denials() {
    // R45/R46 / 场景「已撤销设备与权限不足可区分」「范围缩减提示需要重新认证」。
    let state = setup();
    for credential in [
        CredentialStatus::Revoked,
        CredentialStatus::Unknown,
        CredentialStatus::ScopeReduced,
    ] {
        let issue = block_on(
            state
                .authority
                .hello(&request(), &trust(&state.peer, CredentialStatus::Active)),
        )
        .expect("签发必须成功");
        let authenticated = sync_result(state.authority.verify_proof(
            &submission(&issue, |transcript| state.peer.sign(transcript)),
            &trust(&state.peer, credential),
        ))
        .unwrap_or_else(|error| panic!("签名有效时必须返回成功结果：{error}"));
        assert_eq!(
            authenticated.credential, credential,
            "凭据状态必须如实上报，而不是降级为「权限不足」"
        );
    }
    // 完全没有信任材料（无公钥）时无法验签，只能失败。
    let issue = block_on(state.authority.hello(
        &request(),
        &PeerTrust::unknown(PeerIdentity::Device(device(DEVICE))),
    ))
    .expect("签发必须成功");
    let failure = sync_result(state.authority.verify_proof(
        &submission(&issue, |transcript| state.peer.sign(transcript)),
        &PeerTrust::unknown(PeerIdentity::Device(device(DEVICE))),
    ))
    .expect_err("没有公钥时不可能验签成功");
    assert_eq!(failure.reason, HandshakeError::UntrustedPeer);
}

#[test]
fn fact_carries_no_authorization_conclusion() {
    // R47 / 场景「事实本身不携带授权结论」：只有主体与集合。
    let state = setup();
    let issue = block_on(
        state
            .authority
            .hello(&request(), &trust(&state.peer, CredentialStatus::Active)),
    )
    .expect("签发必须成功");
    let authenticated = sync_result(state.authority.verify_proof(
        &submission(&issue, |transcript| state.peer.sign(transcript)),
        &trust(&state.peer, CredentialStatus::Active),
    ))
    .expect("证明必须通过");
    let IdentityFact::Device {
        device: subject,
        scopes,
    } = authenticated.fact
    else {
        panic!("设备侧事实必须是设备主体");
    };
    assert_eq!(subject.as_str(), DEVICE);
    assert_eq!(scopes.len(), 2);
}

#[test]
fn completion_reports_consumption_target_once() {
    // R49/R51 / 场景「认证成功后才消费配对」：只有仍持有 secret 的配对才被声明为需要消费。
    let state = setup();
    let fact = IdentityFact::Device {
        device: device(DEVICE),
        scopes: support::scopes(&["session.list"]),
    };
    let completion =
        state
            .authority
            .complete_auth(fact.clone(), Some(&pairing(CONNECTION)), &ts(CREATED));
    assert_eq!(
        completion.consume_pairing, None,
        "内存里没有该配对的 secret"
    );
    assert_eq!(completion.fact, fact);
    assert_eq!(completion.at, ts(CREATED));
}

#[test]
fn keystore_failure_fails_closed() {
    // 任务 2.8 的失败关闭：keystore 不可用时不得降级为「无签名挑战」。
    let state = setup();
    state.keystore.fail_next_sign();
    let failure = block_on(
        state
            .authority
            .hello(&request(), &trust(&state.peer, CredentialStatus::Active)),
    )
    .expect_err("keystore 不可用必须失败");
    assert!(matches!(failure, HandshakeError::Keystore(_)));
}

#[test]
fn entropy_failure_fails_closed() {
    // 任务 2.8 的失败关闭：熵源不可用时不得退回弱随机。
    let state = setup();
    state.entropy.fail_next();
    let failure = block_on(
        state
            .authority
            .hello(&request(), &trust(&state.peer, CredentialStatus::Active)),
    )
    .expect_err("熵源不可用必须失败");
    assert_eq!(failure, HandshakeError::EntropyUnavailable);
}

#[test]
fn binding_must_be_self_consistent() {
    // 绑定自洽 + 与登记值一致：Host 与 origin authority 不符、或 origin 与登记值不同都必须拒绝。
    let state = setup();
    let mut probe = request();
    probe.binding.host = "other.example.test".to_owned();
    assert_eq!(
        block_on(
            state
                .authority
                .hello(&probe, &trust(&state.peer, CredentialStatus::Active))
        )
        .expect_err("Host 与 origin 不符必须拒绝"),
        HandshakeError::BindingMismatch
    );
    let mut probe = request();
    probe.binding.canonical_origin =
        Some(CanonicalOrigin::parse("https://elsewhere.example.test").expect("规范 origin"));
    probe.binding.host = "elsewhere.example.test".to_owned();
    assert_eq!(
        block_on(
            state
                .authority
                .hello(&probe, &trust(&state.peer, CredentialStatus::Active))
        )
        .expect_err("origin 与登记值不同必须拒绝"),
        HandshakeError::BindingMismatch
    );
    // 设备侧带 endpoint（节点侧字段）必须拒绝。
    let mut probe = request();
    probe.binding.endpoint = Some(
        NodeEndpoint::parse("wss://owner.example.ts.net:8443/node-link/v1").expect("规范 endpoint"),
    );
    assert_eq!(
        block_on(
            state
                .authority
                .hello(&probe, &trust(&state.peer, CredentialStatus::Active))
        )
        .expect_err("设备侧不得带 endpoint"),
        HandshakeError::BindingKind
    );
    // 节点侧带 canonical origin 必须拒绝。
    let mut probe = request();
    probe.kind = ConnectionKind::NodeLink;
    probe.peer = PeerIdentity::Node(node(NODE));
    probe.binding = ConnectionBinding {
        canonical_origin: Some(origin()),
        host: "owner.example.ts.net:8443".to_owned(),
        endpoint: None,
    };
    probe.catalog_revision = Some(42);
    assert_eq!(
        block_on(
            state
                .authority
                .hello(&probe, &PeerTrust::unknown(probe.peer.clone()))
        )
        .expect_err("节点侧不得带 canonical origin"),
        HandshakeError::BindingKind
    );
}

#[test]
fn node_link_challenge_requires_catalog_revision() {
    // Node Link 的必填 catalogRevision 与 owner 证明。
    let state = setup();
    let node_trust = PeerTrust {
        peer: PeerIdentity::Node(node(NODE)),
        public_key: Some(state.peer.public_key()),
        credential: CredentialStatus::Active,
        host_binding: Some("wss://owner.example.ts.net:8443/node-link/v1".to_owned()),
        node_kind: Some(NodeKind::Access),
        scopes: support::scopes(&[]),
        grants: support::grants(&["grant.observe"]),
    };
    let mut node_request = ChallengeRequest {
        kind: ConnectionKind::NodeLink,
        peer: PeerIdentity::Node(node(NODE)),
        binding: ConnectionBinding {
            canonical_origin: None,
            host: "owner.example.ts.net:8443".to_owned(),
            endpoint: Some(
                NodeEndpoint::parse("wss://owner.example.ts.net:8443/node-link/v1")
                    .expect("规范 endpoint"),
            ),
        },
        client_nonce: client_nonce(),
        negotiated_features: FeatureList::new(vec!["node-link.core.v1".to_owned()]),
        catalog_revision: None,
    };
    assert_eq!(
        block_on(state.authority.hello(&node_request, &node_trust))
            .expect_err("缺少 catalogRevision"),
        HandshakeError::MissingCatalogRevision
    );
    node_request.catalog_revision = Some(42);
    let issue = block_on(state.authority.hello(&node_request, &node_trust))
        .expect("Node Link 挑战必须签发");
    assert!(issue.host_proof.as_bytes().iter().any(|byte| *byte != 0));
    // Node Link 的失败映射到 `node.auth_failed`（§14.2 的两个节点握手动作随 design D12 进入词表）。
    let failure = identity_auth::HandshakeFailure::new(
        HandshakeError::UnknownChallenge,
        ConnectionKind::NodeLink,
    );
    assert_eq!(failure.audit(), Some(AuditAction::NodeAuthFailed));
    assert_eq!(
        failure.public_class(),
        HandshakeFailureClass::AuthenticationFailed
    );
    // 同步侧不允许出现 catalogRevision（设备侧连接）。
    let mut sync_probe = crate::request();
    sync_probe.catalog_revision = Some(1);
    let failure = block_on(
        state
            .authority
            .hello(&sync_probe, &trust(&state.peer, CredentialStatus::Active)),
    )
    .expect_err("Sync 侧出现 catalogRevision 必须拒绝");
    assert_eq!(failure, HandshakeError::MissingCatalogRevision);
}

#[test]
fn challenge_cache_is_bounded() {
    // R30 的资源边界（`AGENTS.md` §5：数量与资源限制）：反复 hello 但不提交 proof 的连接
    // 不得让内存单调增长。三条断言都必须在实现退化时失败：
    //   ① 缓存能被填满（否则说明条目没真正累积）；
    //   ② 填满后再签发不增长（淘汰分支真的执行）；
    //   ③ 时钟越过挑战 TTL 后签发一条会把过期条目清扫掉（清扫分支真的执行）。
    let state = setup();
    let unknown = || PeerTrust::unknown(PeerIdentity::Device(device(DEVICE)));
    for _ in 0..identity_auth::MAX_CHALLENGES {
        block_on(state.authority.hello(&request(), &unknown())).expect("签发必须成功");
    }
    assert_eq!(
        state.authority.challenge_cache_len(),
        identity_auth::MAX_CHALLENGES,
        "缓存应被填满（前提：每次 hello 的挑战标识互异）"
    );
    block_on(state.authority.hello(&request(), &unknown())).expect("签发必须成功");
    assert_eq!(
        state.authority.challenge_cache_len(),
        identity_auth::MAX_CHALLENGES,
        "满缓存再签发必须淘汰最旧条目而不是继续增长"
    );
    state.clock.set(AFTER_TTL);
    block_on(state.authority.hello(&request(), &unknown())).expect("签发必须成功");
    assert_eq!(
        state.authority.challenge_cache_len(),
        1,
        "时钟越过 TTL 后，过期挑战必须被清扫"
    );
}

#[test]
fn cache_evicts_the_earliest_expiring_challenge() {
    // 合同 §5.2 登记的淘汰语义（RV3-WP2-F3 的用例缺口）：满时淘汰**最早过期**的一条。
    //
    // 关键构造：让「最早过期」那一组**只有一条**（先签发第一条，再把时钟推进 1 秒后签发其余），
    // 否则同组的并列条目使「淘汰哪一条」不确定，断言也证明不了策略。
    // 时钟总推进控制在挑战 TTL（15 秒）内，否则第一条会因**过期清扫**而不是**淘汰**消失。
    let state = setup();
    let unknown = || PeerTrust::unknown(PeerIdentity::Device(device(DEVICE)));

    let earliest = block_on(state.authority.hello(&request(), &unknown())).expect("签发必须成功");
    state.clock.set("2026-01-01T00:00:01.000Z");
    for index in 0..(identity_auth::MAX_CHALLENGES - 1) {
        if index > 0 && index % 512 == 0 {
            let seconds = 1 + index / 512;
            state
                .clock
                .set(&format!("2026-01-01T00:00:{seconds:02}.000Z"));
        }
        block_on(state.authority.hello(&request(), &unknown())).expect("签发必须成功");
    }
    assert_eq!(
        state.authority.challenge_cache_len(),
        identity_auth::MAX_CHALLENGES,
        "缓存应被填满"
    );

    // 第 1025 次签发：都在 TTL 内（清扫不命中）→ 必须走淘汰分支，且只能淘汰唯一的最早过期者。
    let newest = block_on(state.authority.hello(&request(), &unknown())).expect("签发必须成功");
    assert_eq!(
        state.authority.challenge_cache_len(),
        identity_auth::MAX_CHALLENGES,
        "淘汰后尺寸不变"
    );

    let failure = state
        .authority
        .verify_proof(
            &submission(&earliest, |transcript| state.peer.sign(transcript)),
            &trust(&state.peer, CredentialStatus::Active),
        )
        .expect_err("最早过期的那条必须已被淘汰");
    assert_eq!(failure.reason, HandshakeError::UnknownChallenge);

    // 最新一条仍在缓存里可用——否则说明淘汰策略错成了「淘汰最新」。
    state
        .authority
        .verify_proof(
            &submission(&newest, |transcript| state.peer.sign(transcript)),
            &trust(&state.peer, CredentialStatus::Active),
        )
        .expect("最新签发的挑战必须仍然可用");
}

#[test]
fn trust_snapshot_must_belong_to_the_same_peer() {
    // 快照主体与提交主体错配（adapter 传错快照）必须拒绝，而不是用别的对端的公钥验签。
    let state = setup();
    let issue = block_on(
        state
            .authority
            .hello(&request(), &trust(&state.peer, CredentialStatus::Active)),
    )
    .expect("签发必须成功");
    let mut mismatched = trust(&state.peer, CredentialStatus::Active);
    mismatched.peer = PeerIdentity::Device(device("9c8f6b1d-7a35-4f0b-9b6a-2f6d5c4e3b1a"));
    let failure = state
        .authority
        .verify_proof(
            &submission(&issue, |transcript| state.peer.sign(transcript)),
            &mismatched,
        )
        .expect_err("快照主体错配必须拒绝");
    assert_eq!(failure.reason, HandshakeError::UntrustedPeer);
}

#[test]
fn connection_kind_must_match_peer_kind() {
    // 连接类型与对端身份类别必须一致（不得用设备标识走节点连接）。
    let state = setup();
    let mut request = request();
    request.kind = ConnectionKind::NodeLink;
    assert_eq!(
        block_on(
            state
                .authority
                .hello(&request, &PeerTrust::unknown(request.peer.clone()))
        )
        .expect_err("类别不一致必须拒绝"),
        HandshakeError::TrustKind
    );
}

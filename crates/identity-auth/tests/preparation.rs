//! [PV4] 任务 3.1–3.3：准备凭据的专用签发/验签、逐字段绑定、撤销优先级与重启/存储重建边界。
//!
//! 固定向量来自 `fixtures/sync/v1/transcripts/session-preparation.json`（WP1 冻结）；本文件同时覆盖
//! 篡改 claims、错误 domain、跨设备/跨节点、撤销与未知设备、正常重启（同一 `storeEpoch`）与整库重建
//! （新 `storeEpoch`），并断言凭据不是 bearer 凭证、错误与快照不含密钥或 claims 值。

mod support;

use identity_auth::{
    AgentId, Authenticated, CanonicalOrigin, ChallengeId, ConnectionBinding, CredentialStatus,
    FeatureList, IdentityFact, NodeKind, NormalizedWorkspacePath, P1363Signature,
    PreparationBinding, PreparationClaims, PreparationError, PreparationExpectation,
    PreparationReceipt, PreparationWorkspaceSource, ServerEpoch, SessionId, SyncHostChallenge,
    verify_preparation_receipt,
};
use support::{FakeClock, FakeKeystore, SequenceEntropy, block_on};

const STORE_EPOCH: &str = "9c6a1e34-5b7d-4f2a-9d18-6e0b2c3d4e5f";
/// 整库重建后 `meta.server_epoch` 的取值（同一 receipt 因此失效）。
const REBUILT_EPOCH: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
const SESSION: &str = "6f1c2d3e-4a5b-4c6d-8e7f-901122334455";
const OTHER_SESSION: &str = "11112222-3333-4444-8555-666677778888";
const OTHER_NODE: &str = "0f9c3b9a-2d31-4fd0-8e5d-4a4f7a5f1b10";
const OTHER_ORIGIN: &str = "https://other.example.ts.net";
const TAB_NONCE_HEX: &str = "404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f";
const WORKSPACE: &str = "D:\\Projects\\acp-remote";

/// 正常路径的 claims（`ownerNodeId` 必须是测试 `Authority` 的本节点 `HOST`）。
fn claims() -> PreparationClaims {
    PreparationClaims {
        device_id: support::device(support::DEVICE),
        owner_node_id: support::node(support::HOST),
        store_epoch: ServerEpoch::new(STORE_EPOCH).expect("规范 storeEpoch"),
        origin: CanonicalOrigin::parse(support::ORIGIN).expect("规范 origin"),
        tab_nonce: support::nonce_from_hex(TAB_NONCE_HEX),
        session_id: SessionId::new(SESSION).expect("规范 sessionId"),
        agent_id: AgentId::new("codex").expect("规范 agentId"),
        normalized_workspace_path: NormalizedWorkspacePath::parse(WORKSPACE).expect("规范路径"),
        workspace_source: PreparationWorkspaceSource::WorkspacePath,
    }
}

/// 与 claims 完全一致的「即将使用」绑定（正常路径）。
fn expectation(claims: &PreparationClaims) -> PreparationExpectation {
    PreparationExpectation {
        owner_node_id: claims.owner_node_id.clone(),
        store_epoch: claims.store_epoch.clone(),
        session_id: claims.session_id.clone(),
        tab_nonce: claims.tab_nonce.clone(),
        agent_id: claims.agent_id.clone(),
        normalized_workspace_path: claims.normalized_workspace_path.clone(),
        workspace_source: claims.workspace_source,
    }
}

fn authenticated(device: &str, credential: CredentialStatus, origin: &str) -> Authenticated {
    let origin = CanonicalOrigin::parse(origin).expect("测试 origin 必须规范");
    Authenticated {
        fact: IdentityFact::Device {
            device: support::device(device),
        },
        credential,
        connection: ConnectionBinding {
            host: origin.authority().to_owned(),
            canonical_origin: Some(origin),
            endpoint: None,
        },
    }
}

fn active_device(device: &str) -> Authenticated {
    authenticated(device, CredentialStatus::Active, support::ORIGIN)
}

/// 用本机 `Authority`（fake keystore 固定私钥）签发正常 claims。
fn sign(claims: &PreparationClaims) -> (std::sync::Arc<FakeKeystore>, PreparationReceipt) {
    let keystore = FakeKeystore::new();
    let authority = support::authority(&keystore, &SequenceEntropy::new(), &FakeClock::new());
    let receipt = block_on(authority.sign_preparation_receipt(claims)).expect("签名必须成功");
    (keystore, receipt)
}

/// 用同一把本节点私钥直接签一份**被篡改**的 claims（构造「签名有效但绑定不符」的负例）。
fn sign_directly(keystore: &FakeKeystore, claims: &PreparationClaims) -> P1363Signature {
    keystore.sign_direct(&claims.transcript().expect("装配必须成功"))
}

fn verify(
    keystore: &FakeKeystore,
    authenticated: &Authenticated,
    receipt: &PreparationReceipt,
    expected: &PreparationExpectation,
) -> Result<identity_auth::PreparationAdmission, PreparationError> {
    verify_preparation_receipt(
        authenticated,
        &keystore.node_public_key(),
        receipt,
        expected,
    )
}

#[test]
fn signing_uses_the_registered_transcript_and_the_keystore_port() {
    let claims = claims();
    let (keystore, receipt) = sign(&claims);

    assert_eq!(
        receipt.signature().as_bytes().len(),
        64,
        "必须是 P1363 64 字节"
    );
    assert_eq!(receipt.claims(), &claims);
    // 端口收到的是**按登记表装配**的 transcript，不是任意字节或普通 JSON。
    let signed = keystore.signed();
    assert_eq!(signed.len(), 1, "签名只能经 keystore 端口发生一次");
    assert_eq!(signed[0], claims.transcript().expect("装配必须成功"));
}

#[test]
fn signed_receipt_verifies_for_the_authenticated_device() {
    let claims = claims();
    let (keystore, receipt) = sign(&claims);
    let admission = verify(
        &keystore,
        &active_device(support::DEVICE),
        &receipt,
        &expectation(&claims),
    )
    .expect("正常绑定必须通过");
    assert_eq!(
        admission.session_id(),
        &SessionId::new(SESSION).expect("规范 sessionId")
    );
}

#[test]
fn normal_restart_keeps_the_receipt_valid() {
    let claims = claims();
    let (keystore, receipt) = sign(&claims);
    // 新 `Authority` = 新进程：验签只依赖持久 `storeEpoch` 与 keystore，不依赖任何内存准备状态。
    let restarted = support::authority(&keystore, &SequenceEntropy::new(), &FakeClock::new());
    let admission = block_on(restarted.verify_preparation_receipt(
        &active_device(support::DEVICE),
        &receipt,
        &expectation(&claims),
    ))
    .expect("同一 storeEpoch 的正常重启后仍必须有效");
    assert_eq!(
        admission.session_id(),
        &SessionId::new(SESSION).expect("规范 sessionId")
    );
}

#[test]
fn rebuilt_store_is_a_distinct_outcome_not_a_binding_or_stale() {
    let claims = claims();
    let (keystore, receipt) = sign(&claims);
    let mut expected = expectation(&claims);
    expected.store_epoch = ServerEpoch::new(REBUILT_EPOCH).expect("规范 storeEpoch");

    let error = verify(
        &keystore,
        &active_device(support::DEVICE),
        &receipt,
        &expected,
    )
    .expect_err("storeEpoch 不一致必须失败");
    assert!(matches!(error, PreparationError::StoreRebuilt));
    assert!(
        !matches!(error, PreparationError::Binding(_)),
        "整库重建不得折叠成普通绑定失败"
    );
    assert!(
        error.to_string().contains("重建"),
        "错误必须明确说明存储已重建"
    );
}

#[test]
fn tampering_any_claim_breaks_the_signature() {
    let base = claims();
    let (keystore, receipt) = sign(&base);
    let mut mutations: Vec<(&str, PreparationClaims)> = Vec::new();

    let mut mutated = base.clone();
    mutated.owner_node_id = support::node(OTHER_NODE);
    mutations.push(("ownerNodeId", mutated));

    let mut mutated = base.clone();
    mutated.device_id = support::device(support::OTHER_DEVICE);
    mutations.push(("deviceId", mutated));

    let mut mutated = base.clone();
    mutated.origin = CanonicalOrigin::parse(OTHER_ORIGIN).expect("规范 origin");
    mutations.push(("origin", mutated));

    let mut mutated = base.clone();
    mutated.store_epoch = ServerEpoch::new(REBUILT_EPOCH).expect("规范 storeEpoch");
    mutations.push(("storeEpoch", mutated));

    let mut mutated = base.clone();
    mutated.tab_nonce = support::nonce_from_hex("0102030405060708090a0b0c0d0e0f10");
    mutations.push(("tabNonce", mutated));

    let mut mutated = base.clone();
    mutated.session_id = SessionId::new(OTHER_SESSION).expect("规范 sessionId");
    mutations.push(("sessionId", mutated));

    let mut mutated = base.clone();
    mutated.agent_id = AgentId::new("other-agent").expect("规范 agentId");
    mutations.push(("agentId", mutated));

    let mut mutated = base.clone();
    mutated.normalized_workspace_path =
        NormalizedWorkspacePath::parse("D:\\Projects\\other").expect("规范路径");
    mutations.push(("normalizedWorkspacePath", mutated));

    let mut mutated = base.clone();
    mutated.workspace_source = PreparationWorkspaceSource::DirectoryId;
    mutations.push(("workspaceSource", mutated));

    for (field, mutated) in mutations {
        let tampered = PreparationReceipt::new(mutated, *receipt.signature());
        let error = verify(
            &keystore,
            &active_device(support::DEVICE),
            &tampered,
            &expectation(&base),
        )
        .expect_err("篡改 claims 必须验签失败");
        assert!(
            matches!(error, PreparationError::Signature),
            "{field} 被篡改后必须是密码学失败，实际 {error:?}"
        );
    }
}

#[test]
fn validly_signed_claims_with_wrong_bindings_are_rejected() {
    let base = claims();
    let keystore = FakeKeystore::new();
    let authenticated = active_device(support::DEVICE);
    let expected = expectation(&base);

    let mut cases: Vec<(&str, PreparationClaims, PreparationBinding)> = Vec::new();

    let mut claims = base.clone();
    claims.owner_node_id = support::node(OTHER_NODE);
    cases.push(("ownerNodeId", claims, PreparationBinding::OwnerNode));

    let mut claims = base.clone();
    claims.device_id = support::device(support::OTHER_DEVICE);
    cases.push(("deviceId", claims, PreparationBinding::Device));

    let mut claims = base.clone();
    claims.origin = CanonicalOrigin::parse(OTHER_ORIGIN).expect("规范 origin");
    cases.push(("origin", claims, PreparationBinding::Origin));

    let mut claims = base.clone();
    claims.session_id = SessionId::new(OTHER_SESSION).expect("规范 sessionId");
    cases.push(("sessionId", claims, PreparationBinding::Session));

    let mut claims = base.clone();
    claims.tab_nonce = support::nonce_from_hex("0102030405060708090a0b0c0d0e0f10");
    cases.push(("tabNonce", claims, PreparationBinding::TabNonce));

    let mut claims = base.clone();
    claims.agent_id = AgentId::new("other-agent").expect("规范 agentId");
    cases.push(("agentId", claims, PreparationBinding::Agent));

    let mut claims = base.clone();
    claims.normalized_workspace_path =
        NormalizedWorkspacePath::parse("D:\\Projects\\other").expect("规范路径");
    cases.push((
        "normalizedWorkspacePath",
        claims,
        PreparationBinding::WorkspacePath,
    ));

    let mut claims = base.clone();
    claims.workspace_source = PreparationWorkspaceSource::DirectoryId;
    cases.push((
        "workspaceSource",
        claims,
        PreparationBinding::WorkspaceSource,
    ));

    for (field, claims, binding) in cases {
        // 用同一把节点私钥签名：签名本身有效，失败只能来自绑定比较。
        let signature = sign_directly(&keystore, &claims);
        let receipt = PreparationReceipt::new(claims, signature);
        let error =
            verify(&keystore, &authenticated, &receipt, &expected).expect_err("绑定不一致必须失败");
        assert_eq!(
            error,
            PreparationError::Binding(binding),
            "{field} 的绑定失败必须落到对应分类"
        );
    }
}

#[test]
fn cross_device_and_cross_node_receipts_are_rejected() {
    let claims = claims();
    let (keystore, receipt) = sign(&claims);

    // 同一 receipt 拿到另一台已认证设备上必须失败（跨设备）。
    let error = verify(
        &keystore,
        &active_device(support::OTHER_DEVICE),
        &receipt,
        &expectation(&claims),
    )
    .expect_err("跨设备使用必须失败");
    assert_eq!(error, PreparationError::Binding(PreparationBinding::Device));

    // 签名有效但 claims 声明另一个 ownerNode：跨节点使用必须失败。
    let mut foreign_node = claims.clone();
    foreign_node.owner_node_id = support::node(OTHER_NODE);
    let foreign = PreparationReceipt::new(
        foreign_node.clone(),
        sign_directly(&keystore, &foreign_node),
    );
    let error = verify(
        &keystore,
        &active_device(support::DEVICE),
        &foreign,
        &expectation(&claims),
    )
    .expect_err("跨节点使用必须失败");
    assert_eq!(
        error,
        PreparationError::Binding(PreparationBinding::OwnerNode)
    );

    // 用另一把节点密钥签出的 receipt 在本节点必须验签失败。
    let other_key_receipt = PreparationReceipt::new(
        claims.clone(),
        FakeKeystore::sign_with_other(&claims.transcript().expect("装配必须成功")),
    );
    let error = verify(
        &keystore,
        &active_device(support::DEVICE),
        &other_key_receipt,
        &expectation(&claims),
    )
    .expect_err("另一个节点密钥签出的 receipt 必须失败");
    assert_eq!(error, PreparationError::Signature);
}

#[test]
fn signing_refuses_claims_for_another_owner_node() {
    let mut claims = claims();
    claims.owner_node_id = support::node(OTHER_NODE);
    let keystore = FakeKeystore::new();
    let authority = support::authority(&keystore, &SequenceEntropy::new(), &FakeClock::new());

    let error =
        block_on(authority.sign_preparation_receipt(&claims)).expect_err("不得为其他节点出证");
    assert_eq!(
        error,
        PreparationError::Binding(PreparationBinding::OwnerNode)
    );
    assert!(
        keystore.signed().is_empty(),
        "拒绝必须先于 keystore 签名调用（私钥不被使用）"
    );
}

#[test]
fn revoked_unknown_and_reduced_devices_are_rejected_before_signature_checks() {
    let claims = claims();
    let (keystore, receipt) = sign(&claims);
    // 故意破坏签名：若实现顺序错误，会先报 `Signature` 而不是凭据状态。
    let mut bytes = *receipt.signature().as_bytes();
    bytes[63] ^= 0x01;
    let broken = PreparationReceipt::new(
        claims.clone(),
        P1363Signature::try_from_bytes(&bytes).expect("仍是 64 字节 P1363 形状"),
    );
    let expected = expectation(&claims);

    for (credential, want) in [
        (CredentialStatus::Revoked, PreparationError::DeviceRevoked),
        (CredentialStatus::Unknown, PreparationError::DeviceUnknown),
        (
            CredentialStatus::ScopeReduced,
            PreparationError::ReauthenticationRequired,
        ),
    ] {
        let authenticated = authenticated(support::DEVICE, credential, support::ORIGIN);
        let error = verify(&keystore, &authenticated, &broken, &expected)
            .expect_err("非 Active 凭据必须失败");
        assert_eq!(error, want, "凭据状态必须先于签名校验");
    }
}

#[test]
fn node_link_connection_cannot_use_a_preparation_receipt() {
    let claims = claims();
    let (keystore, receipt) = sign(&claims);
    let authenticated = Authenticated {
        fact: IdentityFact::Node {
            node: support::node(OTHER_NODE),
            kind: NodeKind::Owner,
        },
        credential: CredentialStatus::Active,
        connection: ConnectionBinding {
            host: "work-pc.example.ts.net".to_owned(),
            canonical_origin: None,
            endpoint: None,
        },
    };
    let error = verify(&keystore, &authenticated, &receipt, &expectation(&claims))
        .expect_err("节点连接不能使用准备凭据");
    assert_eq!(error, PreparationError::Unauthenticated);
}

#[test]
fn device_connection_without_canonical_origin_is_rejected() {
    let claims = claims();
    let (keystore, receipt) = sign(&claims);
    let authenticated = Authenticated {
        fact: IdentityFact::Device {
            device: support::device(support::DEVICE),
        },
        credential: CredentialStatus::Active,
        connection: ConnectionBinding {
            host: "work-pc.example.ts.net".to_owned(),
            canonical_origin: None,
            endpoint: None,
        },
    };
    let error = verify(&keystore, &authenticated, &receipt, &expectation(&claims))
        .expect_err("没有连接 origin 绑定时必须失败");
    assert_eq!(error, PreparationError::Binding(PreparationBinding::Origin));
}

#[test]
fn signature_from_another_domain_is_rejected() {
    let claims = claims();
    let keystore = FakeKeystore::new();
    let authority = support::authority(&keystore, &SequenceEntropy::new(), &FakeClock::new());
    let challenge = SyncHostChallenge {
        host_id: support::node(support::HOST),
        device_id: support::device(support::DEVICE),
        canonical_origin: CanonicalOrigin::parse(support::ORIGIN).expect("规范 origin"),
        client_nonce: support::nonce_from_hex("01"),
        server_nonce: support::nonce_from_hex("02"),
        connection_id: ChallengeId::parse("20212223-2425-4627-a829-2a2b2c2d2e2f")
            .expect("规范 connectionId"),
        negotiated_features: FeatureList::new(vec!["core.session-preparation.v1".to_owned()]),
    };
    let cross_domain =
        block_on(authority.sign_sync_host_challenge(&challenge)).expect("跨 domain 签名用于负例");

    let receipt = PreparationReceipt::new(claims.clone(), cross_domain);
    let error = verify(
        &keystore,
        &active_device(support::DEVICE),
        &receipt,
        &expectation(&claims),
    )
    .expect_err("跨 domain 重放必须验签失败");
    assert_eq!(error, PreparationError::Signature);
}

#[test]
fn receipt_rejects_non_canonical_padded_and_wrong_length_signatures() {
    let claims = claims();
    let (_, receipt) = sign(&claims);

    for text in [
        format!("{}=", receipt.signature_base64url()),
        "+".repeat(86),
        acpr_transcript::encode_base64url(&[0u8; 71]),
        acpr_transcript::encode_base64url(&[0u8; 63]),
    ] {
        let error = PreparationReceipt::from_parts(claims.clone(), &text)
            .expect_err("非法签名编码必须被拒绝");
        assert!(
            matches!(error, PreparationError::InvalidEncoding(_)),
            "编码失败必须与密码学/绑定失败分开：{error:?}"
        );
    }
}

#[test]
fn closed_claim_value_objects_reject_unknown_tokens_and_bad_paths() {
    assert!(matches!(
        PreparationWorkspaceSource::parse("symlink"),
        Err(identity_auth::TranscriptError::InvalidField(
            "workspaceSource"
        ))
    ));
    assert!(NormalizedWorkspacePath::parse("").is_err());
    assert!(NormalizedWorkspacePath::parse("with\0nul").is_err());
    assert!(NormalizedWorkspacePath::parse(&"x".repeat(4097)).is_err());
    assert!(NormalizedWorkspacePath::parse(WORKSPACE).is_ok());
}

#[test]
fn receipt_and_errors_do_not_leak_private_key_material_or_claim_values() {
    let claims = claims();
    let (keystore, receipt) = sign(&claims);

    // fake keystore 的固定私钥标量与句柄文本：receipt 的 Debug 不得包含它们。
    let private_scalar = "0707070707070707070707070707070707070707070707070707070707070707";
    let rendered = format!("{receipt:?}");
    assert!(!rendered.contains(private_scalar));
    assert!(!rendered.contains("node-identity/primary"));

    // 绑定错误只在本地区分字段类别，不带路径/Agent/origin 等 claims 值。
    let mut other_path_claims = claims.clone();
    other_path_claims.normalized_workspace_path =
        NormalizedWorkspacePath::parse("D:\\Projects\\other").expect("规范路径");
    let other_path_receipt = PreparationReceipt::new(
        other_path_claims.clone(),
        sign_directly(&keystore, &other_path_claims),
    );
    let error = verify(
        &keystore,
        &active_device(support::DEVICE),
        &other_path_receipt,
        &expectation(&claims),
    )
    .expect_err("路径不一致必须失败");
    assert_eq!(
        error,
        PreparationError::Binding(PreparationBinding::WorkspacePath)
    );
    let rendered = format!("{error} {error:?}");
    assert!(!rendered.contains(WORKSPACE));
    assert!(!rendered.contains("Projects"));

    // 门槛只携带会话标识，不带授权/scope。
    let admission = verify(
        &keystore,
        &active_device(support::DEVICE),
        &receipt,
        &expectation(&claims),
    )
    .expect("正常路径必须通过");
    let rendered = format!("{admission:?}").to_lowercase();
    assert!(!rendered.contains("scope"));
    assert!(!rendered.contains("grant"));
}

#[test]
fn receipt_and_claims_debug_redact_all_values_and_signature() {
    let claims = claims();
    let (_, receipt) = sign(&claims);

    let rendered = format!("{receipt:?}");
    assert_eq!(
        rendered,
        "PreparationReceipt { claims: <redacted>, signature: <redacted> }"
    );
    for sensitive in [
        WORKSPACE,
        "Projects",
        support::ORIGIN,
        support::HOST,
        support::DEVICE,
        SESSION,
        TAB_NONCE_HEX,
        STORE_EPOCH,
        "codex",
    ] {
        assert!(
            !rendered.contains(sensitive),
            "receipt 的 Debug 不得包含 claims 值或标识：{sensitive}"
        );
    }
    assert!(
        !rendered.contains(&receipt.signature_base64url()),
        "receipt 的 Debug 不得包含签名内容"
    );

    let claims_rendered = format!("{:?}", receipt.claims());
    assert_eq!(claims_rendered, "PreparationClaims { <redacted> }");
    for sensitive in [
        WORKSPACE,
        "Projects",
        support::ORIGIN,
        support::DEVICE,
        SESSION,
    ] {
        assert!(
            !claims_rendered.contains(sensitive),
            "claims 的 Debug 不得包含字段值：{sensitive}"
        );
    }
}

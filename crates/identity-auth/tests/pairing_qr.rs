//! [WP2] 扫码取回凭据：独立生成、重复取回、认领关闭、全部清理路径与碰撞去重。
//!
//! 每个用例用「新建 `Authority` + 同一组端口」表达进程重启，用注入时钟/显式时间戳表达过期，
//! 用定序熵源表达可重跑，因此没有睡眠、没有系统时间、没有数据库。

mod support;

use std::sync::{Arc, Mutex};

use acp_core::model::{NodeKind, PairingRecord, PairingState, PeerIdentity};
use identity_auth::{
    Authority, CanonicalOrigin, ClaimFields, ClaimKindFields, ClaimOutcome, ClientKind,
    EntropyError, EntropySource, IdentityFact, KeyHandle, NodeEndpoint, PairingDecision,
    PairingDraft, PairingProof, PairingSpec, QrToken,
};

use support::{
    DEVICE, FakeClock, FakeKeystore, HOST, ORIGIN, PAIRING, PeerKey, SequenceEntropy, authority,
    client_hmac, device, node, nonce_from_hex, pairing, ts,
};

/// 基准时刻（配对创建时间）。
const CREATED: &str = "2026-01-01T00:00:00.000Z";
/// 5 分钟窗口之内。
const WITHIN_WINDOW: &str = "2026-01-01T00:04:59.000Z";
/// 原过期时间（`WITHIN_WINDOW` 的下一毫秒）。
const AFTER_WINDOW: &str = "2026-01-01T00:05:00.000Z";
/// 收窄到一分钟的有效期。
const SHORT_WINDOW: &str = "2026-01-01T00:01:00.000Z";
/// 第二个配对标识（碰撞/材料替换用例）。
const OTHER_PAIRING: &str = "7f3d1c2b-8a4e-4d9c-b1f0-5a6e7d8c9b0a";

fn device_spec() -> PairingSpec {
    PairingSpec::Device {
        canonical_origin: CanonicalOrigin::parse(ORIGIN).expect("测试用 origin 必须规范"),
    }
}

fn node_spec() -> PairingSpec {
    PairingSpec::Node {
        endpoint: NodeEndpoint::parse("wss://node.example.ts.net/node-link/v1")
            .expect("测试用 endpoint 必须规范"),
        kind: NodeKind::Access,
    }
}

fn create_device() -> (Authority, PairingDraft) {
    create_device_with_expiry(WITHIN_WINDOW)
}

fn create_device_with_expiry(expires_at: &str) -> (Authority, PairingDraft) {
    let keystore = FakeKeystore::new();
    let entropy = SequenceEntropy::new();
    let clock = FakeClock::new();
    let state_machine = authority(&keystore, &entropy, &clock);
    let draft = state_machine
        .begin_pairing(
            &pairing(PAIRING),
            &device_spec(),
            Some("Test Phone"),
            &ts(CREATED),
            &ts(expires_at),
        )
        .expect("创建配对必须成功");
    (state_machine, draft)
}

/// 落定前的 `pending_confirmation` 记录（认领成功但尚未确认）。
fn pending_record(record: &PairingRecord) -> PairingRecord {
    PairingRecord::try_new(
        record.id().clone(),
        record.target(),
        PairingState::PendingConfirmation,
        record.display_name().map(str::to_owned),
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

/// 构造一份合法认领（对端是「另一个实现」：用自己的 secret 与私钥算 proof）。
fn device_claim(draft: &PairingDraft, peer: &PeerKey) -> ClaimFields {
    let mut fields = ClaimFields {
        pairing: pairing(PAIRING),
        peer: PeerIdentity::Device(device(DEVICE)),
        kind: ClaimKindFields::Device {
            client_kind: ClientKind::Pwa,
        },
        display_name: "Test Phone".to_owned(),
        host_binding: ORIGIN.to_owned(),
        public_key: peer.public_key(),
        client_nonce: nonce_from_hex("00"),
        proof: PairingProof::try_from_bytes(&[0u8; 32]).expect("32 字节"),
    };
    let transcript = identity_auth::transcript::SyncPairingProof {
        host_id: node(HOST),
        device_id: device(DEVICE),
        pairing_id: pairing(PAIRING),
        pairing_expires_at: draft.record.expires_at().clone(),
        canonical_origin: CanonicalOrigin::parse(ORIGIN).expect("测试用 origin 必须规范"),
        device_public_key: fields.public_key.clone(),
        client_nonce: fields.client_nonce.clone(),
        device_name: fields.display_name.clone(),
        client_kind: ClientKind::Pwa,
    }
    .transcript()
    .expect("装配必须成功");
    fields.proof = client_hmac(&draft.secret, &transcript);
    fields
}

fn unknown_token() -> QrToken {
    QrToken::try_from_bytes(&[0xEE; 32]).expect("32 字节凭据")
}

#[test]
fn device_pairing_gets_an_independent_qr_token() {
    let (authority, draft) = create_device();
    let token = draft.qr_token.expect("设备配对必须生成扫码凭据");

    // 独立抽取：与 pairing secret 不同，且网页码仍由 secret 派生。
    assert_ne!(
        token.as_bytes(),
        draft.secret.as_bytes(),
        "扫码凭据与配对秘密必须分别随机生成"
    );
    assert_eq!(
        identity_auth::derive_pairing_code(&draft.secret).expect("派生必须成功"),
        draft.code,
        "网页码仍只从配对秘密派生"
    );
    assert_eq!(QrToken::LEN, 32);

    let lookup = authority
        .retrieve_qr_material(&draft.record, &token)
        .expect("有效且未认领的设备配对必须可取回");
    assert_eq!(lookup.pairing, pairing(PAIRING));
    assert_eq!(lookup.secret, draft.secret);
    assert_eq!(
        authority
            .lookup_pairing_by_qr_token(&token)
            .map(|lookup| lookup.pairing),
        Some(pairing(PAIRING)),
        "反查入口返回唯一绑定的配对"
    );
    assert!(
        authority
            .retrieve_qr_material(&draft.record, &unknown_token())
            .is_none(),
        "未知凭据不得取回"
    );
}

#[test]
fn node_pairing_never_gets_a_qr_token() {
    let keystore = FakeKeystore::new();
    let entropy = SequenceEntropy::new();
    let clock = FakeClock::new();
    let state_machine = authority(&keystore, &entropy, &clock);
    let draft = state_machine
        .begin_pairing(
            &pairing(PAIRING),
            &node_spec(),
            Some("Owner Node"),
            &ts(CREATED),
            &ts(WITHIN_WINDOW),
        )
        .expect("节点配对必须成功");

    assert!(draft.qr_token.is_none(), "节点配对不得生成设备扫码凭据");
    // 直接证据：节点创建只抽取 server nonce（32）、请求标识（16）与 pairing secret（32）。
    // 扫码凭据是一份独立的 32 字节抽取（设备配对会多出第 4 次 `fill`），节点配对没有它。
    assert_eq!(
        entropy.fill_sizes(),
        vec![32, 16, 32],
        "节点创建恰好 3 次 fill：nonce 32 + request id 16 + secret 32，不生成/不登记扫码凭据"
    );
    assert!(
        state_machine
            .lookup_pairing_by_qr_token(&unknown_token())
            .is_none()
    );
    assert!(
        state_machine
            .retrieve_qr_material(&draft.record, &unknown_token())
            .is_none(),
        "节点记录永远不是扫码取回目标"
    );
}

#[test]
fn repeated_retrieval_returns_the_same_material() {
    let (authority, draft) = create_device();
    let token = draft.qr_token.expect("设备配对必须生成扫码凭据");

    let first = authority
        .retrieve_qr_material(&draft.record, &token)
        .expect("首次取回必须成功");
    let second = authority
        .retrieve_qr_material(&draft.record, &token)
        .expect("重复取回必须成功（响应丢失可重试）");
    let third = authority
        .retrieve_qr_material(&draft.record, &token)
        .expect("凭据不被消耗");

    assert_eq!(first, second);
    assert_eq!(first, third);
    assert_eq!(first.pairing, pairing(PAIRING));
    assert_eq!(first.secret, draft.secret);
    assert_eq!(
        draft.record.expires_at().as_str(),
        WITHIN_WINDOW,
        "取回不得改写到期时间"
    );
    assert_eq!(
        draft.record.state(),
        PairingState::Created,
        "取回不得推进状态"
    );
}

#[test]
fn qr_retrieval_does_not_consume_the_web_code() {
    let (authority, draft) = create_device();
    let token = draft.qr_token.expect("设备配对必须生成扫码凭据");

    assert!(
        authority
            .retrieve_qr_material(&draft.record, &token)
            .is_some()
    );
    assert!(
        authority
            .retrieve_qr_material(&draft.record, &token)
            .is_some()
    );

    let code_lookup = authority
        .consume_pairing_code(&draft.code)
        .expect("扫码取回之后网页码仍是一次性可用的");
    assert_eq!(code_lookup.pairing, pairing(PAIRING));
    assert_eq!(code_lookup.secret, draft.secret);

    assert!(
        authority.consume_pairing_code(&draft.code).is_none(),
        "网页码仍然只消耗一次"
    );
    assert!(
        authority
            .retrieve_qr_material(&draft.record, &token)
            .is_some(),
        "扫码取回不得恢复已消耗的码，但自身仍可用"
    );
}

#[test]
fn successful_claim_closes_retrieval_while_keeping_the_secret() {
    let (authority, draft) = create_device();
    let token = draft.qr_token.expect("设备配对必须生成扫码凭据");
    let peer = PeerKey::new();
    let fields = device_claim(&draft, &peer);

    let ClaimOutcome::Claimed(claimed) = authority
        .verify_claim(&draft.record, None, &fields)
        .expect("首次认领必须成功")
    else {
        panic!("首次认领必须成功");
    };

    // 服务端在持久化提交成功后调用关闭（identity-auth 不读存储，因此不自行推断提交结果）。
    assert!(authority.close_qr_retrieval(&pairing(PAIRING)));
    assert!(
        authority.has_secret(&pairing(PAIRING)),
        "关闭取回必须保留 secret 供幂等 claim/status 使用"
    );
    assert!(authority.lookup_pairing_by_qr_token(&token).is_none());
    assert!(
        authority
            .retrieve_qr_material(&draft.record, &token)
            .is_none(),
        "关闭后即使传入旧的 created 记录也不能取回"
    );
    assert!(
        !authority.close_qr_retrieval(&pairing(PAIRING)),
        "关闭是幂等的"
    );

    // 相同内容的幂等重试仍返回首次结果，但绝不重新开放取回。
    assert_eq!(
        authority.verify_claim(&draft.record, Some(&claimed), &fields),
        Ok(ClaimOutcome::Repeat(claimed))
    );
    assert!(
        authority
            .retrieve_qr_material(&draft.record, &token)
            .is_none()
    );

    // 即使调用方尚未关闭，已认领（pending_confirmation）的持久状态也不再可取回。
    let (other_authority, other_draft) = create_device_with_expiry(WITHIN_WINDOW);
    let other_token = other_draft.qr_token.expect("设备配对必须生成扫码凭据");
    let pending = pending_record(&other_draft.record);
    assert!(
        other_authority
            .retrieve_qr_material(&pending, &other_token)
            .is_none(),
        "非 created 状态一律不可取回"
    );
    assert!(
        other_authority
            .retrieve_qr_material(&other_draft.record, &other_token)
            .is_some(),
        "对照：同一凭据在 created 状态下仍可取回"
    );
}

#[test]
fn rejection_closes_retrieval_even_though_the_secret_is_kept() {
    let (authority, draft) = create_device();
    let token = draft.qr_token.expect("设备配对必须生成扫码凭据");
    assert!(
        authority
            .retrieve_qr_material(&draft.record, &token)
            .is_some()
    );

    let settlement = authority
        .settle(
            &draft.record,
            &PairingDecision::Reject {
                reason: Some("user denied".to_owned()),
            },
            &ts(CREATED),
        )
        .expect("拒绝必须成功");
    assert!(!settlement.is_approved());
    assert!(
        authority.has_secret(&pairing(PAIRING)),
        "为状态轮询保留 secret（合同 §4.3）"
    );
    assert!(
        authority
            .pairing_request_material(&pairing(PAIRING))
            .is_some(),
        "状态查询材料仍可读取"
    );
    assert!(authority.lookup_pairing_by_qr_token(&token).is_none());
    assert!(
        authority
            .retrieve_qr_material(&draft.record, &token)
            .is_none(),
        "拒绝后不得再用同一凭据取回"
    );
}

#[test]
fn expiry_closes_retrieval() {
    let (authority, draft) = create_device();
    let token = draft.qr_token.expect("设备配对必须生成扫码凭据");
    assert!(
        authority
            .retrieve_qr_material(&draft.record, &token)
            .is_some()
    );

    let due = authority.due_pairings(std::slice::from_ref(&draft.record), &ts(AFTER_WINDOW));
    assert_eq!(due, vec![pairing(PAIRING)], "created 记录到期需终结");
    assert!(!authority.has_secret(&pairing(PAIRING)));
    assert!(authority.lookup_pairing_by_qr_token(&token).is_none());
    assert!(
        authority
            .retrieve_qr_material(&draft.record, &token)
            .is_none()
    );
}

#[test]
fn proof_failure_invalidation_closes_retrieval() {
    let (authority, draft) = create_device();
    let token = draft.qr_token.expect("设备配对必须生成扫码凭据");
    let peer = PeerKey::new();
    let claim = device_claim(&draft, &peer);
    assert!(
        authority
            .retrieve_qr_material(&draft.record, &token)
            .is_some()
    );

    let mut wrong = claim;
    wrong.proof = PairingProof::try_from_bytes(&[9u8; 32]).expect("32 字节");
    for attempt in 1..=4 {
        assert_eq!(
            authority.verify_claim(&draft.record, None, &wrong),
            Ok(ClaimOutcome::Rejected(
                identity_auth::ClaimRejection::ProofInvalid
            )),
            "第 {attempt} 次失败还是普通 proof 失败"
        );
    }
    assert_eq!(
        authority.verify_claim(&draft.record, None, &wrong),
        Ok(ClaimOutcome::Rejected(
            identity_auth::ClaimRejection::TooManyFailures
        ))
    );
    assert_eq!(authority.failure_count(&pairing(PAIRING)), 5);
    assert!(
        authority.has_secret(&pairing(PAIRING)),
        "失效不清除 secret（它仍受原过期时间约束）"
    );
    assert!(authority.lookup_pairing_by_qr_token(&token).is_none());
    assert!(
        authority
            .retrieve_qr_material(&draft.record, &token)
            .is_none(),
        "证明失败失效后不得取回"
    );
}

#[test]
fn restart_recovery_closes_retrieval() {
    // ① 组合根启动时的 `reset_memory`。
    let (authority, draft) = create_device();
    let token = draft.qr_token.expect("设备配对必须生成扫码凭据");
    assert!(
        authority
            .retrieve_qr_material(&draft.record, &token)
            .is_some()
    );
    authority.reset_memory();
    assert!(!authority.has_secret(&pairing(PAIRING)));
    assert!(authority.lookup_pairing_by_qr_token(&token).is_none());
    assert!(
        authority
            .retrieve_qr_material(&draft.record, &token)
            .is_none()
    );

    // ② `unrecoverable_after_restart` 对未确认配对的终结。
    let (authority, draft) = create_device();
    let token = draft.qr_token.expect("设备配对必须生成扫码凭据");
    assert_eq!(
        authority.unrecoverable_after_restart(std::slice::from_ref(&draft.record)),
        vec![pairing(PAIRING)]
    );
    assert!(!authority.has_secret(&pairing(PAIRING)));
    assert!(authority.lookup_pairing_by_qr_token(&token).is_none());
    assert!(
        authority
            .retrieve_qr_material(&draft.record, &token)
            .is_none()
    );
}

#[test]
fn first_authentication_closes_retrieval() {
    let (authority, draft) = create_device();
    let token = draft.qr_token.expect("设备配对必须生成扫码凭据");
    let pending = pending_record(&draft.record);
    authority
        .settle(&pending, &PairingDecision::Approve, &ts(CREATED))
        .expect("批准必须成功");
    assert!(authority.mark_pairing_approved(&pairing(PAIRING)));
    assert!(
        authority.lookup_pairing_by_qr_token(&token).is_some(),
        "批准本身不消耗内存材料"
    );

    let completion = authority.complete_auth(
        IdentityFact::Device {
            device: device(DEVICE),
        },
        Some(&pairing(PAIRING)),
        &ts(CREATED),
    );
    assert_eq!(completion.consume_pairing, Some(pairing(PAIRING)));
    assert!(!authority.has_secret(&pairing(PAIRING)));
    assert!(authority.lookup_pairing_by_qr_token(&token).is_none());
    assert!(
        authority
            .retrieve_qr_material(&draft.record, &token)
            .is_none()
    );
}

#[test]
fn re_registering_the_same_pairing_replaces_its_qr_token() {
    let (authority, first) = create_device();
    let first_token = first.qr_token.expect("设备配对必须生成扫码凭据");
    assert!(
        authority
            .retrieve_qr_material(&first.record, &first_token)
            .is_some()
    );

    let second = authority
        .begin_pairing(
            &pairing(PAIRING),
            &device_spec(),
            Some("Test Phone"),
            &ts(CREATED),
            &ts(WITHIN_WINDOW),
        )
        .expect("同一配对标识可重新登记（材料替换）");
    let second_token = second.qr_token.expect("设备配对必须生成扫码凭据");
    assert_ne!(first_token, second_token, "替换材料必须生成新凭据");

    assert!(
        authority.lookup_pairing_by_qr_token(&first_token).is_none(),
        "旧凭据立即失效"
    );
    assert_eq!(
        authority
            .lookup_pairing_by_qr_token(&second_token)
            .map(|lookup| lookup.pairing),
        Some(pairing(PAIRING))
    );
    assert!(
        authority
            .retrieve_qr_material(&second.record, &second_token)
            .is_some()
    );
    assert!(
        authority
            .retrieve_qr_material(&first.record, &first_token)
            .is_none(),
        "旧凭据不能取回新材料"
    );
    assert!(
        authority
            .retrieve_qr_material(&first.record, &second_token)
            .is_none(),
        "旧记录的摘要与新材料不一致，必须失败关闭"
    );
}

#[test]
fn duplicate_qr_tokens_are_regenerated() {
    // 脚本化熵源：按第几次 `fill` 决定字节；第 7 次（第二次设备创建的凭据抽取）刻意返回与
    // 第 3 次（第一次的凭据）相同的字节，模拟碰撞。
    #[derive(Debug, Default)]
    struct CollidingEntropy {
        calls: Mutex<u64>,
    }

    impl EntropySource for CollidingEntropy {
        fn fill(&self, out: &mut [u8]) -> Result<(), EntropyError> {
            let index = {
                let mut calls = self.calls.lock().expect("熵源锁");
                let index = *calls;
                *calls += 1;
                index
            };
            let source = if index == 7 { 3 } else { index };
            let byte = (source as u8).wrapping_mul(0x25).wrapping_add(0x11);
            out.fill(byte);
            Ok(())
        }
    }

    let keystore = FakeKeystore::new();
    let entropy: Arc<CollidingEntropy> = Arc::new(CollidingEntropy::default());
    let clock = FakeClock::new();
    let state_machine = Authority::new(
        node(HOST),
        KeyHandle::new("node-identity/primary").expect("合法引用"),
        keystore.clone(),
        entropy.clone(),
        clock.clone(),
    );

    let first = state_machine
        .begin_pairing(
            &pairing(PAIRING),
            &device_spec(),
            Some("Test Phone"),
            &ts(CREATED),
            &ts(WITHIN_WINDOW),
        )
        .expect("第一次创建必须成功");
    let second = state_machine
        .begin_pairing(
            &pairing(OTHER_PAIRING),
            &device_spec(),
            Some("Other Phone"),
            &ts(CREATED),
            &ts(WITHIN_WINDOW),
        )
        .expect("第二次创建必须成功");

    let first_token = first.qr_token.expect("设备配对必须生成扫码凭据");
    let second_token = second.qr_token.expect("设备配对必须生成扫码凭据");
    assert_ne!(
        first_token, second_token,
        "碰撞后必须重新生成，两个配对不得共享同一凭据"
    );
    assert_eq!(
        *entropy.calls.lock().expect("熵源锁"),
        9,
        "第一次 4 次 + 第二次 4 次 + 碰撞重试 1 次"
    );
    assert_eq!(
        state_machine
            .lookup_pairing_by_qr_token(&first_token)
            .map(|lookup| lookup.pairing),
        Some(pairing(PAIRING))
    );
    assert_eq!(
        state_machine
            .lookup_pairing_by_qr_token(&second_token)
            .map(|lookup| lookup.pairing),
        Some(pairing(OTHER_PAIRING))
    );
    assert!(
        state_machine
            .retrieve_qr_material(&first.record, &first_token)
            .is_some()
    );
    assert!(
        state_machine
            .retrieve_qr_material(&second.record, &second_token)
            .is_some()
    );
    assert!(
        state_machine
            .retrieve_qr_material(&first.record, &second_token)
            .is_none(),
        "凭据只绑定自己的配对"
    );
}

#[test]
fn repeated_retrieval_never_extends_a_short_expiry() {
    let keystore = FakeKeystore::new();
    let entropy = SequenceEntropy::new();
    let clock = FakeClock::new();
    let state_machine = authority(&keystore, &entropy, &clock);
    let draft = state_machine
        .begin_pairing(
            &pairing(PAIRING),
            &device_spec(),
            Some("Test Phone"),
            &ts(CREATED),
            &ts(SHORT_WINDOW),
        )
        .expect("收窄到一分钟必须成功");
    let token = draft.qr_token.expect("设备配对必须生成扫码凭据");

    for at in [
        CREATED,
        "2026-01-01T00:00:30.000Z",
        "2026-01-01T00:00:59.000Z",
    ] {
        clock.set(at);
        let lookup = state_machine
            .retrieve_qr_material(&draft.record, &token)
            .expect("到期前重复取回必须成功");
        assert_eq!(lookup.secret, draft.secret);
        assert_eq!(
            draft.record.expires_at().as_str(),
            SHORT_WINDOW,
            "重复取回不得续期"
        );
    }

    clock.set(SHORT_WINDOW);
    assert!(
        state_machine
            .retrieve_qr_material(&draft.record, &token)
            .is_none(),
        "到达原到期时间后即使凭据正确也不得取回"
    );
    assert!(
        state_machine.has_secret(&pairing(PAIRING)),
        "取回失败不等于清理（清理由 due_pairings 完成）"
    );
}

#[test]
fn qr_token_is_redacted_and_never_persisted() {
    let fixed = QrToken::try_from_bytes(&[0xAB; 32]).expect("32 字节凭据");
    assert_eq!(format!("{fixed:?}"), "QrToken(<redacted>)");
    assert!(
        QrToken::try_from_bytes(&[0xAB; 31]).is_err(),
        "短长度必须拒绝"
    );
    assert!(
        QrToken::try_from_bytes(&[0xAB; 33]).is_err(),
        "长长度必须拒绝"
    );

    let (_, draft) = create_device();
    let token = draft.qr_token.expect("设备配对必须生成扫码凭据");
    let encoded = acpr_transcript::encode_base64url(token.as_bytes());
    let hexed: String = token
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();

    let draft_debug = format!("{draft:?}");
    assert!(draft_debug.contains("QrToken(<redacted>)"));
    assert!(
        !draft_debug.contains(&encoded),
        "Draft 的 Debug 不得包含凭据"
    );
    assert!(
        !draft_debug.contains(&hexed),
        "Draft 的 Debug 不得包含凭据的 hex 形态"
    );

    let record_debug = format!("{:?}", draft.record);
    assert!(!record_debug.contains(&encoded));
    assert!(!record_debug.contains(&hexed));
    assert!(
        !format!("{:?}", draft.secret).contains(&hexed)
            && !format!("{:?}", draft.code).contains(&hexed),
        "既有敏感类型仍保持脱敏"
    );
}

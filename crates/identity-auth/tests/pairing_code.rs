//! [WP2] 网页入口配对码：派生固定向量、Crockford 归一化、单次消耗、碰撞去重与生命周期清理。
//!
//! 每个用例用「新建 `Authority` + 同一组端口」表达进程重启，用注入时钟/显式时间戳表达过期，
//! 因此没有睡眠、没有系统时间、没有数据库。

mod support;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use acp_core::model::{PairingRecord, PairingState};
use acp_core::ports::Clock;
use identity_auth::{
    Authority, CanonicalOrigin, EntropyError, EntropySource, IdentityFact, InvalidPairingCode,
    KeyHandle, PairingCode, PairingDecision, PairingDraft, PairingSecret, PairingSpec,
    derive_pairing_code,
};

use support::{
    DEVICE, FakeClock, FakeKeystore, HOST, ORIGIN, PAIRING, SequenceEntropy, authority, device,
    hex, node, pairing, ts,
};

/// 基准时刻（配对创建时间）。
const CREATED: &str = "2026-01-01T00:00:00.000Z";
/// 5 分钟窗口之内。
const WITHIN_WINDOW: &str = "2026-01-01T00:04:59.000Z";
/// 原过期时间（`WITHIN_WINDOW` 的下一毫秒）。
const AFTER_WINDOW: &str = "2026-01-01T00:05:00.000Z";

/// 第二个配对标识（碰撞用例）。
const OTHER_PAIRING: &str = "7f3d1c2b-8a4e-4d9c-b1f0-5a6e7d8c9b0a";

fn device_spec() -> PairingSpec {
    PairingSpec::Device {
        canonical_origin: CanonicalOrigin::parse(ORIGIN).expect("测试用 origin 必须规范"),
    }
}

fn create_device_pairing() -> (Authority, PairingDraft) {
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
            &ts(WITHIN_WINDOW),
        )
        .expect("创建配对必须成功");
    (state_machine, draft)
}

/// 落定前的 `pending_confirmation` 记录（批准只在此状态可用）。
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

/// 落定后的 `rejected` 记录（终态）。
fn rejected_record(record: &PairingRecord) -> PairingRecord {
    PairingRecord::try_new(
        record.id().clone(),
        record.target(),
        PairingState::Rejected,
        record.display_name().map(str::to_owned),
        record.secret_digest().clone(),
        record.host_binding(),
        record.created_at().clone(),
        record.expires_at().clone(),
        Some(ts(CREATED)),
        None,
        Some(ts(CREATED)),
    )
    .expect("测试记录必须合法")
}

fn parse(text: &str) -> PairingCode {
    PairingCode::parse(text).expect("测试用配对码必须合法")
}

#[test]
fn derivation_matches_fixed_vectors() {
    // 固定向量（独立于实现计算：HMAC-SHA256 + 40 bit 大端 + Crockford）。
    let cases = [
        (
            "0000000000000000000000000000000000000000000000000000000000000000",
            "BK93C9RB",
        ),
        (
            "0101010101010101010101010101010101010101010101010101010101010101",
            "9NV80NVC",
        ),
        (
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            "Q08BQR90",
        ),
        (
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
            "PHX2D077",
        ),
    ];
    for (secret_hex, expected) in cases {
        let secret = PairingSecret::try_from_bytes(&hex(secret_hex)).expect("32 字节 secret");
        let code = derive_pairing_code(&secret).expect("HMAC 初始化对任意长度 key 都成功");
        assert_eq!(code.as_str(), expected, "固定向量必须逐字符相同");
        assert_eq!(code.as_str().len(), PairingCode::LEN);
        assert_eq!(
            derive_pairing_code(&secret).expect("重复派生"),
            code,
            "同一 secret 必须得到同一组 8 位"
        );
    }
}

#[test]
fn parse_normalizes_case_ambiguous_characters_and_separator() {
    assert_eq!(parse("abcd-efgh").as_str(), "ABCDEFGH");
    assert_eq!(parse("BK93C9RB").as_str(), "BK93C9RB");
    assert_eq!(
        parse("oil1-2345").as_str(),
        "01112345",
        "I/L → 1、O → 0，大小写不敏感"
    );
    // 一个 `-` 可以出现在任意位置；规范形态总是无分隔符。
    assert_eq!(parse("-BK93C9RB").as_str(), "BK93C9RB");
}

#[test]
fn parse_rejects_wrong_shape() {
    for invalid in [
        "",
        "BK93C9R",         // 7 位
        "BK93C9RBX",       // 9 位
        "BK93C9RU",        // U 不在 Crockford 字母表
        "BK93-C9-RB",      // 两个分隔符
        "BK93 C9RB",       // 空格
        "BK93C9R!",        // 非法字符
        "BK93C9R\u{00e9}", // 非 ASCII
    ] {
        assert_eq!(
            PairingCode::parse(invalid),
            Err(InvalidPairingCode),
            "{invalid:?} 必须被拒绝"
        );
    }
}

#[test]
fn successful_lookup_returns_binding_and_secret_and_consumes_the_code() {
    let (state_machine, draft) = create_device_pairing();
    let lookup = state_machine
        .consume_pairing_code(&draft.code)
        .expect("活跃配对的码必须能查到");
    assert_eq!(lookup.pairing, pairing(PAIRING));
    assert!(
        lookup.secret.matches(&draft.secret),
        "lookup 必须返回完整 secret，供后续 claim/status 使用"
    );

    // 单次消耗：第二次查找与不存在的码形状一致。
    assert!(state_machine.consume_pairing_code(&draft.code).is_none());
    let absent = parse("ZZZZZZZZ");
    assert!(state_machine.consume_pairing_code(&absent).is_none());

    // 码被消耗不改变配对本身：secret 仍在、状态仍是 created。
    assert!(state_machine.has_secret(&pairing(PAIRING)));
    assert_eq!(
        state_machine.pairing_status(&draft.record, None).state,
        PairingState::Created
    );
}

#[test]
fn lookup_normalizes_user_input_before_comparing() {
    let (state_machine, draft) = create_device_pairing();
    let text = draft.code.as_str();
    let typed = format!("{}-{}", &text[..4], &text[4..]).to_ascii_lowercase();
    let lookup = state_machine
        .consume_pairing_code(&parse(&typed))
        .expect("归一化后的输入必须命中");
    assert_eq!(lookup.pairing, pairing(PAIRING));
}

#[test]
fn expiry_scan_clears_the_code_with_the_secret() {
    let (state_machine, draft) = create_device_pairing();
    let due = state_machine.due_pairings(std::slice::from_ref(&draft.record), &ts(AFTER_WINDOW));
    assert_eq!(due, vec![pairing(PAIRING)]);
    assert!(!state_machine.has_secret(&pairing(PAIRING)));
    assert!(
        state_machine.consume_pairing_code(&draft.code).is_none(),
        "过期清理后码必须与 secret 一起失效"
    );
}

#[test]
fn rejected_pairing_keeps_its_code_until_the_original_expiry() {
    let (state_machine, draft) = create_device_pairing();
    state_machine
        .settle(
            &draft.record,
            &PairingDecision::Reject { reason: None },
            &ts(CREATED),
        )
        .expect("拒绝必须成功");
    // 未到 expires_at：secret 与码都保留（合同 §4.3 允许为可靠轮询保留到原过期时间）。
    let lookup = state_machine
        .consume_pairing_code(&draft.code)
        .expect("拒绝后到原过期时间之前码仍可用");
    assert!(lookup.secret.matches(&draft.secret));
}

#[test]
fn rejected_pairing_code_dies_at_the_original_expiry() {
    let (state_machine, draft) = create_device_pairing();
    state_machine
        .settle(
            &draft.record,
            &PairingDecision::Reject { reason: None },
            &ts(CREATED),
        )
        .expect("拒绝必须成功");
    // 拒绝记录已是终态：不会出现在 `due` 里，但 secret 与码仍必须被清除。
    let due = state_machine.due_pairings(
        std::slice::from_ref(&rejected_record(&draft.record)),
        &ts(AFTER_WINDOW),
    );
    assert!(due.is_empty(), "终态记录不需要推进落库状态");
    assert!(!state_machine.has_secret(&pairing(PAIRING)));
    assert!(state_machine.consume_pairing_code(&draft.code).is_none());
}

#[test]
fn restart_does_not_recover_codes_from_digests() {
    let (state_machine, draft) = create_device_pairing();
    assert_eq!(state_machine.reset_memory().0, 1, "一条内存 secret 被清理");
    assert!(!state_machine.has_secret(&pairing(PAIRING)));
    assert!(
        state_machine.consume_pairing_code(&draft.code).is_none(),
        "重启后不能从持久化摘要重建 secret 或配对码"
    );
}

#[test]
fn restart_termination_also_clears_the_code() {
    let (state_machine, draft) = create_device_pairing();
    let terminated = state_machine.unrecoverable_after_restart(std::slice::from_ref(&draft.record));
    assert_eq!(terminated, vec![pairing(PAIRING)]);
    assert!(state_machine.consume_pairing_code(&draft.code).is_none());
}

#[test]
fn first_authentication_clears_the_code_with_the_secret() {
    let (state_machine, draft) = create_device_pairing();
    let pending = pending_record(&draft.record);
    state_machine
        .settle(&pending, &PairingDecision::Approve, &ts(CREATED))
        .expect("批准必须成功");
    assert!(state_machine.mark_pairing_approved(&pairing(PAIRING)));
    assert!(
        state_machine.has_secret(&pairing(PAIRING)),
        "批准不提前清除 secret，码也仍在"
    );
    let completion = state_machine.complete_auth(
        IdentityFact::Device {
            device: device(DEVICE),
        },
        Some(&pairing(PAIRING)),
        &ts(CREATED),
    );
    assert_eq!(completion.consume_pairing, Some(pairing(PAIRING)));
    assert!(!state_machine.has_secret(&pairing(PAIRING)));
    assert!(state_machine.consume_pairing_code(&draft.code).is_none());
}

/// 脚本熵源：按调用顺序返回固定块（用于构造确定性碰撞）。
#[derive(Debug)]
struct ScriptedEntropy {
    blocks: Mutex<VecDeque<Vec<u8>>>,
}

impl ScriptedEntropy {
    fn new(blocks: Vec<Vec<u8>>) -> Arc<Self> {
        Arc::new(Self {
            blocks: Mutex::new(VecDeque::from(blocks)),
        })
    }
}

impl EntropySource for ScriptedEntropy {
    fn fill(&self, out: &mut [u8]) -> Result<(), EntropyError> {
        let block = self
            .blocks
            .lock()
            .expect("脚本熵源锁")
            .pop_front()
            .ok_or(EntropyError::Unavailable)?;
        if block.len() != out.len() {
            return Err(EntropyError::Unavailable);
        }
        out.copy_from_slice(&block);
        Ok(())
    }
}

#[test]
fn colliding_code_regenerates_the_secret_until_unique() {
    let first_secret = vec![0x11u8; 32];
    let second_secret = vec![0x22u8; 32];
    // `begin_pairing` 的熵序：server nonce(32) → request id(16) → secret(32) → 设备扫码凭据(32)；
    // 第二次创建的第一个 secret 与第一次相同（派生码必然碰撞），必须被丢弃并重新生成，
    // 因此碰撞那次不抽取扫码凭据（凭据在码去重之后生成）。
    let entropy = ScriptedEntropy::new(vec![
        vec![0x01u8; 32],
        vec![0x02u8; 16],
        first_secret.clone(),
        vec![0x05u8; 32],
        vec![0x03u8; 32],
        vec![0x04u8; 16],
        first_secret,
        second_secret.clone(),
        vec![0x06u8; 32],
    ]);
    let clock: Arc<dyn Clock> = FakeClock::new();
    let state_machine = Authority::new(
        node(HOST),
        KeyHandle::new("node-identity/primary").expect("合法引用"),
        FakeKeystore::new(),
        entropy,
        clock,
    );

    let first = state_machine
        .begin_pairing(
            &pairing(PAIRING),
            &device_spec(),
            None,
            &ts(CREATED),
            &ts(WITHIN_WINDOW),
        )
        .expect("第一次创建必须成功");
    let second = state_machine
        .begin_pairing(
            &pairing(OTHER_PAIRING),
            &device_spec(),
            None,
            &ts(CREATED),
            &ts(WITHIN_WINDOW),
        )
        .expect("碰撞后重新生成必须成功");

    assert_eq!(
        first.code,
        derive_pairing_code(&first.secret).expect("码由 secret 派生")
    );
    assert_eq!(
        second.secret.as_bytes(),
        &[0x22u8; 32],
        "碰撞必须丢弃第一个 secret 并重新生成"
    );
    assert_eq!(
        second.code,
        derive_pairing_code(&second.secret).expect("码由新 secret 派生")
    );
    assert_ne!(first.code, second.code, "两个活跃配对不得共享同一个码");

    // 两个码各自指向自己的配对（碰撞去重没有覆盖或丢失条目）。
    let via_first = state_machine
        .consume_pairing_code(&first.code)
        .expect("第一个码仍可查到");
    assert_eq!(via_first.pairing, pairing(PAIRING));
    let via_second = state_machine
        .consume_pairing_code(&second.code)
        .expect("第二个码必须指回第二个配对");
    assert_eq!(via_second.pairing, pairing(OTHER_PAIRING));
    assert!(via_second.secret.matches(&second.secret));
}

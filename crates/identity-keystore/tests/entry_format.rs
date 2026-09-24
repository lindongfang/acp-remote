//! 条目格式：头部往返、绑定与篡改检测（全平台可跑，不依赖 DPAPI）。
//!
//! 这些用例把 `design.md` D6 的格式约束固定下来：明文头字段必须与请求一致、附加熵由
//! 「盐 + 版本 + 用途 + 标签 + 域分离标签」派生、超长与非法标签在构造期失败。

mod support;

use identity_keystore::entry::{
    EntryHeader, EntryPurpose, FORMAT_VERSION, MAGIC, MAX_LABEL_LEN, PRIVATE_KEY_LEN, SALT_LEN,
};

fn make_header(purpose: EntryPurpose, label: &str, salt: [u8; SALT_LEN]) -> EntryHeader {
    EntryHeader::new(purpose, label, FORMAT_VERSION, salt).expect("合法头部")
}

#[test]
fn header_round_trips_and_reports_header_length() {
    let salt = [7u8; SALT_LEN];
    let header = make_header(EntryPurpose::NodeIdentity, "primary", salt);
    let mut bytes = header.encode();
    bytes.extend_from_slice(b"wrapped-secret");
    let (decoded, header_len, wrapped) = EntryHeader::decode(&bytes).expect("必须可解析");
    assert_eq!(decoded, header);
    assert_eq!(header_len, bytes.len() - "wrapped-secret".len());
    assert_eq!(wrapped, b"wrapped-secret");
}

#[test]
fn decode_rejects_bad_magic_version_and_truncation() {
    let header = make_header(EntryPurpose::NodeIdentity, "primary", [1u8; SALT_LEN]);
    let mut bytes = header.encode();
    bytes.extend_from_slice(b"x");

    let mut bad_magic = bytes.clone();
    bad_magic[0] = b'X';
    assert!(EntryHeader::decode(&bad_magic).is_err());

    let mut bad_version = bytes.clone();
    bad_version[5] = 9;
    assert!(EntryHeader::decode(&bad_version).is_err());

    let mut bad_purpose = bytes.clone();
    bad_purpose[6] = 9;
    assert!(EntryHeader::decode(&bad_purpose).is_err());

    // 截断到盐中间：必须失败，不能「读到多少算多少」。
    assert!(EntryHeader::decode(&bytes[..bytes.len() - 40]).is_err());
    // 头部之后的秘密段为空：视为损坏。
    assert!(EntryHeader::decode(&header.encode()).is_err());
    assert_eq!(&bytes[..4], MAGIC);
}

#[test]
fn header_must_match_the_requested_identity() {
    let header = make_header(EntryPurpose::NodeIdentity, "primary", [2u8; SALT_LEN]);
    assert!(
        header
            .matches(EntryPurpose::NodeIdentity, "primary")
            .is_ok()
    );
    assert!(
        header
            .matches(EntryPurpose::NodeIdentity, "secondary")
            .is_err()
    );
    assert!(
        header
            .matches(EntryPurpose::ProviderCredential, "primary")
            .is_err()
    );
}

#[test]
fn entropy_is_stable_and_bound_to_the_header() {
    let salt = [3u8; SALT_LEN];
    let header = make_header(EntryPurpose::NodeIdentity, "primary", salt);
    let entropy = header.additional_entropy();
    assert_eq!(entropy, header.additional_entropy(), "同一条目必须稳定");

    // 换盐、换标签、换用途都必须改变附加熵（复制/剪接条目不能解密）。
    let other_salt = make_header(EntryPurpose::NodeIdentity, "primary", [4u8; SALT_LEN]);
    assert_ne!(entropy, other_salt.additional_entropy());
    let other_label = make_header(EntryPurpose::NodeIdentity, "secondary", salt);
    assert_ne!(entropy, other_label.additional_entropy());
    let other_purpose = make_header(EntryPurpose::ProviderCredential, "primary", salt);
    assert_ne!(entropy, other_purpose.additional_entropy());
}

#[test]
fn header_rejects_illegal_labels_and_versions() {
    assert!(
        EntryHeader::new(
            EntryPurpose::NodeIdentity,
            "",
            FORMAT_VERSION,
            [0u8; SALT_LEN]
        )
        .is_err()
    );
    assert!(
        EntryHeader::new(
            EntryPurpose::NodeIdentity,
            &"a".repeat(MAX_LABEL_LEN + 1),
            FORMAT_VERSION,
            [0u8; SALT_LEN]
        )
        .is_err()
    );
    assert!(
        EntryHeader::new(
            EntryPurpose::NodeIdentity,
            "bad/label",
            FORMAT_VERSION,
            [0u8; SALT_LEN]
        )
        .is_err()
    );
    assert!(
        EntryHeader::new(
            EntryPurpose::NodeIdentity,
            "bad\nlabel",
            FORMAT_VERSION,
            [0u8; SALT_LEN]
        )
        .is_err()
    );
    assert!(EntryHeader::new(EntryPurpose::NodeIdentity, "primary", 9, [0u8; SALT_LEN]).is_err());
    // 边界：恰好 128 字节是允许的。
    assert!(
        EntryHeader::new(
            EntryPurpose::NodeIdentity,
            &"a".repeat(MAX_LABEL_LEN),
            FORMAT_VERSION,
            [0u8; SALT_LEN]
        )
        .is_ok()
    );
}

#[test]
fn purpose_length_rules_are_closed() {
    assert!(EntryPurpose::NodeIdentity.accepts_len(PRIVATE_KEY_LEN));
    assert!(!EntryPurpose::NodeIdentity.accepts_len(PRIVATE_KEY_LEN - 1));
    assert!(!EntryPurpose::NodeIdentity.accepts_len(PRIVATE_KEY_LEN + 1));
    assert!(EntryPurpose::ProviderCredential.accepts_len(1));
    assert!(!EntryPurpose::ProviderCredential.accepts_len(0));
    assert_eq!(
        EntryPurpose::from_token(1).unwrap(),
        EntryPurpose::NodeIdentity
    );
    assert_eq!(
        EntryPurpose::from_token(2).unwrap(),
        EntryPurpose::ProviderCredential
    );
    assert!(EntryPurpose::from_token(3).is_err());
    assert_eq!(EntryPurpose::NodeIdentity.directory(), "node-identity");
    assert_eq!(
        EntryPurpose::ProviderCredential.directory(),
        "provider-credential"
    );
}

//! [PV3] R69–R71 / 任务 2.8：端口与秘密类型的不变量。
//!
//! 这里断言的是「端口形状能被实现」与「秘密不进日志/不进错误/可清零」：
//! 真正的平台实现（DPAPI/Keychain/Secret Service）在 `identity-keystore`（WP3）。

mod support;

use identity_auth::{
    EntropySource, IdentityKeystore, KeyHandle, KeyPurpose, KeystoreError, P1363Signature,
    PairingSecret, SecretBytes, SecretPurpose,
};

use support::{FakeKeystore, block_on};

#[test]
fn key_purpose_and_secret_purpose_are_closed_vocabularies() {
    // R69 / 用途词表闭合：`KeyPurpose` 只有 Node Identity（`DeviceIdentity` 已从合同删除）。
    assert_eq!(KeyPurpose::ALL, &[KeyPurpose::NodeIdentity]);
    assert_eq!(KeyPurpose::NodeIdentity.as_str(), "node-identity");
    assert_eq!(SecretPurpose::ALL, &[SecretPurpose::ProviderCredential]);
    assert_eq!(
        SecretPurpose::ProviderCredential.as_str(),
        "provider-credential"
    );
}

#[test]
fn keystore_handles_are_opaque_in_debug_output() {
    // R70 / 引用不进日志：`Debug` 只表明类型。
    let handle = KeyHandle::new("node-identity/primary").expect("合法引用");
    assert_eq!(format!("{handle:?}"), "KeyHandle(<opaque>)");
    assert!(!format!("{handle:?}").contains("primary"));
    // 形状校验：空、超长、控制字符一律拒绝。
    assert_eq!(KeyHandle::new(""), Err(KeystoreError::EntryInvalid));
    assert_eq!(
        KeyHandle::new(&"x".repeat(257)),
        Err(KeystoreError::EntryInvalid)
    );
    assert_eq!(
        KeyHandle::new("bad\nhandle"),
        Err(KeystoreError::EntryInvalid)
    );
}

#[test]
fn secret_bytes_never_reach_logs_or_errors() {
    // R71 / 秘密不进日志、不进错误：错误文案里没有秘密内容，`SecretBytes` 也没有 `Debug`。
    let secret = SecretBytes::new(b"super-secret-value");
    assert_eq!(secret.len(), b"super-secret-value".len());
    assert!(!secret.is_empty());
    let moved = secret.into_bytes();
    assert_eq!(moved, b"super-secret-value".to_vec());
    for error in [
        KeystoreError::Unavailable,
        KeystoreError::EntryMissing,
        KeystoreError::EntryCorrupt,
        KeystoreError::EntryInvalid,
        KeystoreError::PurposeMismatch,
        KeystoreError::SecretMissing,
    ] {
        let text = format!("{error}");
        assert!(!text.contains("secret-value"));
        assert!(!text.is_empty(), "每个失败分类都必须有可读文案");
    }
}

#[test]
fn pairing_secret_debug_is_redacted_and_digest_differs() {
    // R71 / pairing secret 也不进日志：`Debug` 标记为 redacted，摘要不是 secret 本身。
    let secret = PairingSecret::try_from_bytes(&[7u8; 32]).expect("32 字节");
    assert_eq!(format!("{secret:?}"), "PairingSecret(<redacted>)");
    let digest = secret.digest();
    assert_ne!(
        digest.as_str(),
        acpr_transcript::encode_base64url(secret.as_bytes()),
        "落库的必须是摘要而不是 secret 本身"
    );
    assert_eq!(digest, secret.digest(), "同一 secret 的摘要稳定");
    let other = PairingSecret::try_from_bytes(&[8u8; 32]).expect("32 字节");
    assert_ne!(digest, other.digest());
    assert!(secret.matches(&secret));
    assert!(!secret.matches(&other));
}

#[test]
fn signature_type_only_accepts_p1363() {
    // R71 / 签名形状：64 字节 P1363 才能构造，base64url 往返保持一致。
    let signature = P1363Signature::try_from_bytes(&[3u8; 64]).expect("64 字节");
    assert_eq!(signature.as_bytes().len(), 64);
    assert_eq!(
        P1363Signature::try_from_base64url(&signature.to_base64url()).expect("往返"),
        signature
    );
    assert!(
        P1363Signature::try_from_bytes(&[3u8; 71]).is_err(),
        "DER 必须被拒"
    );
    assert!(
        P1363Signature::try_from_bytes(&[3u8; 33]).is_err(),
        "33 字节压缩点形态必须被拒"
    );
}

#[test]
fn fake_keystore_satisfies_the_port_shape() {
    // R71 / 端口形状可被实现：`public_key` 与 `sign` 必须来自同一把私钥。
    let keystore = FakeKeystore::new();
    let handle =
        block_on(keystore.generate(KeyPurpose::NodeIdentity, "primary")).expect("生成引用必须成功");
    let public_key = block_on(keystore.public_key(&handle)).expect("读公钥必须成功");
    let transcript = b"port-shape-probe";
    let signature = block_on(keystore.sign(&handle, transcript)).expect("签名必须成功");
    assert_eq!(
        keystore.signed().len(),
        1,
        "签名端口必须记录被签名的 transcript（证明签的是装配后的字节）"
    );
    assert_eq!(keystore.signed()[0], transcript.to_vec());
    // 公钥必须是 65 字节 SEC1 未压缩点。
    assert_eq!(public_key.as_bytes().len(), 65);
    assert_eq!(public_key.as_bytes()[0], 0x04);
    assert_eq!(signature.as_bytes().len(), 64);
}

#[test]
fn secret_store_round_trips_through_the_port() {
    // R71 / secret 读写删：`get` 在缺失时返回 `None`（不是错误），删除后回到 `None`。
    let keystore = FakeKeystore::new();
    let purpose = SecretPurpose::ProviderCredential;
    assert!(
        block_on(keystore.get_secret(purpose, "codex"))
            .expect("读 secret")
            .is_none()
    );
    let value = SecretBytes::new(b"provider-token");
    block_on(keystore.put_secret(purpose, "codex", &value)).expect("写 secret");
    let read = block_on(keystore.get_secret(purpose, "codex"))
        .expect("读 secret")
        .expect("必须存在");
    assert_eq!(read.as_bytes(), b"provider-token");
    block_on(keystore.delete_secret(purpose, "codex")).expect("删 secret");
    assert!(
        block_on(keystore.get_secret(purpose, "codex"))
            .expect("读 secret")
            .is_none()
    );
}

#[test]
fn entropy_source_failure_is_observable() {
    // R71 / 熵源端口只有「不可用」一种失败，调用方必须失败关闭。
    let entropy = support::SequenceEntropy::new();
    let mut buffer = [0u8; 32];
    entropy.fill(&mut buffer).expect("正常熵源必须填充");
    assert!(buffer.iter().any(|byte| *byte != 0), "必须真的写入字节");
    entropy.fail_next();
    let failure = entropy.fill(&mut buffer).expect_err("失败必须可观");
    assert_eq!(format!("{failure}"), "系统熵源不可用");
}

#[test]
fn remove_operation_reports_missing_entry() {
    // R71 / `delete` 对不存在的条目必须返回明确错误（不静默成功）。
    struct Empty;
    #[async_trait::async_trait]
    impl IdentityKeystore for Empty {
        async fn generate(
            &self,
            _purpose: KeyPurpose,
            _label: &str,
        ) -> Result<KeyHandle, KeystoreError> {
            Err(KeystoreError::Unavailable)
        }

        async fn public_key(
            &self,
            _handle: &KeyHandle,
        ) -> Result<acp_core::model::PeerPublicKey, KeystoreError> {
            Err(KeystoreError::EntryMissing)
        }

        async fn sign(
            &self,
            _handle: &KeyHandle,
            _transcript: &[u8],
        ) -> Result<P1363Signature, KeystoreError> {
            Err(KeystoreError::Unavailable)
        }

        async fn delete(&self, _handle: &KeyHandle) -> Result<(), KeystoreError> {
            Err(KeystoreError::EntryMissing)
        }

        async fn get_secret(
            &self,
            _purpose: SecretPurpose,
            _key: &str,
        ) -> Result<Option<SecretBytes>, KeystoreError> {
            Err(KeystoreError::Unavailable)
        }

        async fn put_secret(
            &self,
            _purpose: SecretPurpose,
            _key: &str,
            _value: &SecretBytes,
        ) -> Result<(), KeystoreError> {
            Err(KeystoreError::Unavailable)
        }

        async fn delete_secret(
            &self,
            _purpose: SecretPurpose,
            _key: &str,
        ) -> Result<(), KeystoreError> {
            Err(KeystoreError::Unavailable)
        }
    }

    let empty = Empty;
    assert_eq!(
        block_on(empty.delete(&KeyHandle::new("node-identity/primary").expect("合法引用"))),
        Err(KeystoreError::EntryMissing)
    );
    assert!(matches!(
        block_on(empty.generate(KeyPurpose::NodeIdentity, "primary")),
        Err(KeystoreError::Unavailable)
    ));
}

//! [PV5] Windows DPAPI 实证用例（本文件只在 Windows 编译；`cargo test … dpapi` 选中它）。
//!
//! 覆盖 `docs/SECURITY_DESIGN.md` §20 要求的四件事，全部用**真实 DPAPI**（不是 mock）：
//!
//! 1. 包裹/解开往返：被包裹的字节与明文不同、且不含明文；
//! 2. 附加熵绑定：换熵（=换用途/标签/盐）解不开；
//! 3. 篡改检测：改动被包裹字节后解不开（完整性由平台保证）；
//! 4. 条目级语义：一个条目的被包裹段换到另一个条目下解不开（附加熵把条目绑死）。
//!
//! `Scope::User`（当前用户）是**唯一**档位：本 crate 的公开包裹入口不接受 scope 参数，
//! 因此调用方无法顺手改用 `Scope::Machine`。

#![cfg(windows)]

mod support;

use identity_auth::{IdentityKeystore, KeyPurpose, KeystoreError, SecretBytes, SecretPurpose};
use identity_keystore::FileKeystore;
use identity_keystore::entry::EntryPurpose;
use identity_keystore::platform::{unwrap_secret, wrap_secret};

use support::{FixedEntropy, TempRoot, block_on};

const ENTROPY: [u8; 32] = [0x11; 32];

#[test]
fn dpapi_wraps_and_unwraps_without_leaking_plaintext() {
    let plaintext = b"acp-remote dpapi probe";
    let wrapped = wrap_secret(plaintext, &ENTROPY).expect("DPAPI 包裹必须成功");
    assert_ne!(wrapped.as_slice(), plaintext, "被包裹字节不得等于明文");
    assert!(
        !wrapped
            .windows(plaintext.len())
            .any(|window| window == plaintext),
        "被包裹字节里不得出现明文"
    );
    assert!(
        wrapped.len() > plaintext.len(),
        "DPAPI 输出必须带自己的头部与完整性数据"
    );
    let unwrapped = unwrap_secret(&wrapped, &ENTROPY).expect("DPAPI 解开必须成功");
    assert_eq!(unwrapped.as_slice(), plaintext);
}

#[test]
fn dpapi_rejects_wrong_entropy_and_tampering() {
    let plaintext = b"acp-remote dpapi probe";
    let wrapped = wrap_secret(plaintext, &ENTROPY).expect("DPAPI 包裹必须成功");
    // 换熵：等价于换了用途/标签/版本/盐。
    let other = [0x22u8; 32];
    assert_eq!(
        unwrap_secret(&wrapped, &other),
        Err(identity_keystore::StoreError::Corrupt)
    );
    // 篡改任意一个字节：DPAPI 的完整性保护必须检出。
    let mut tampered = wrapped.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0x01;
    assert_eq!(
        unwrap_secret(&tampered, &ENTROPY),
        Err(identity_keystore::StoreError::Corrupt)
    );
    // 截断：同样解不开。
    assert_eq!(
        unwrap_secret(&wrapped[..wrapped.len() - 1], &ENTROPY),
        Err(identity_keystore::StoreError::Corrupt)
    );
}

#[test]
fn dpapi_entry_round_trip_signs_in_process() {
    let root = TempRoot::new("dpapi-sign");
    let entropy = FixedEntropy::new();
    let store = FileKeystore::new(root.root().join("keystore"), entropy.clone());
    block_on(async {
        let handle = store
            .generate(KeyPurpose::NodeIdentity, "primary")
            .await
            .expect("Windows 上生成必须成功");
        let public_key = store.public_key(&handle).await.expect("公钥必须可读");
        assert_eq!(public_key.as_bytes().len(), 65);
        let signature = store
            .sign(&handle, b"acp-remote/dpapi-probe/v1")
            .await
            .expect("进程内签名必须成功");
        assert_eq!(signature.as_bytes().len(), 64);
        // 条目文件里只有头部 + 被包裹的秘密值（明文标量不存在于文件里）。
        let path = store.entry_path(EntryPurpose::NodeIdentity, "primary");
        let bytes = std::fs::read(&path).expect("条目必须存在");
        assert_eq!(&bytes[..4], b"ACPK");
        assert!(bytes.len() > 41);
    });
}

#[test]
fn dpapi_entropy_binds_entries_to_their_identity() {
    // 把条目 A 的被包裹段整段搬到条目 B 下（保留 B 的头部与盐）：必须解不开。
    let root = TempRoot::new("dpapi-bind");
    let entropy = FixedEntropy::new();
    let store = FileKeystore::new(root.root().join("keystore"), entropy.clone());
    block_on(async {
        let first = store
            .generate(KeyPurpose::NodeIdentity, "primary")
            .await
            .expect("生成必须成功");
        let second = store
            .generate(KeyPurpose::NodeIdentity, "secondary")
            .await
            .expect("生成必须成功");

        let first_path = store.entry_path(EntryPurpose::NodeIdentity, "primary");
        let second_path = store.entry_path(EntryPurpose::NodeIdentity, "secondary");
        let first_bytes = std::fs::read(&first_path).expect("条目必须存在");
        let second_bytes = std::fs::read(&second_path).expect("条目必须存在");
        let (_, first_header_len, first_wrapped) =
            identity_keystore::entry::EntryHeader::decode(&first_bytes).expect("头部必须可解析");
        let (_, second_header_len, _) =
            identity_keystore::entry::EntryHeader::decode(&second_bytes).expect("头部必须可解析");

        let mut spliced = second_bytes[..second_header_len].to_vec();
        spliced.extend_from_slice(&first_bytes[first_header_len..]);
        assert!(!first_wrapped.is_empty());
        std::fs::write(&second_path, &spliced).expect("写入剪接内容");

        assert_eq!(
            store.public_key(&second).await,
            Err(KeystoreError::EntryCorrupt),
            "剪接的条目必须解不开（附加熵绑定了用途/标签/盐）"
        );
        // 原条目不受影响。
        assert!(store.public_key(&first).await.is_ok());
    });
}

#[test]
fn dpapi_provider_credentials_round_trip() {
    let root = TempRoot::new("dpapi-secret");
    let entropy = FixedEntropy::new();
    let store = FileKeystore::new(root.root().join("keystore"), entropy.clone());
    block_on(async {
        let purpose = SecretPurpose::ProviderCredential;
        let value = SecretBytes::new(b"provider-token-value");
        store
            .put_secret(purpose, "codex", &value)
            .await
            .expect("写入必须成功");
        let read = store
            .get_secret(purpose, "codex")
            .await
            .expect("读取必须成功")
            .expect("必须存在");
        assert_eq!(read.as_bytes(), b"provider-token-value");
        let path = store.entry_path(EntryPurpose::ProviderCredential, "codex");
        let bytes = std::fs::read(&path).expect("条目必须存在");
        assert!(
            !bytes
                .windows(b"provider-token-value".len())
                .any(|window| window == b"provider-token-value"),
            "凭据明文不得出现在条目文件里"
        );
    });
}

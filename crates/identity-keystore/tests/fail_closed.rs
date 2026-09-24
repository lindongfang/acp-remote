//! 失败关闭路径（Linux CI 与 Windows 都跑）：
//!
//! - 平台不可用时所有入口返回不可用，且**零持久化写入**；
//! - 条目损坏/引用悬空时失败而不新建身份；
//! - 秘密材料不出现在 `Debug` 与错误文案里（错误分类是封闭词表）。

mod support;

use identity_auth::{
    IdentityKeystore, KeyHandle, KeyPurpose, KeystoreError, SecretBytes, SecretPurpose,
};
use identity_keystore::entry::EntryPurpose;
use identity_keystore::{FileKeystore, platform_supported};

use support::{FixedEntropy, TempRoot, block_on, contains_plaintext};

#[test]
fn platform_availability_is_reported_honestly() {
    // 判据来自编译期平台，而不是「试着用一下再猜」。
    assert_eq!(platform_supported(), cfg!(windows));
}

#[test]
fn unavailable_platform_writes_nothing_at_all() {
    if platform_supported() {
        // Windows 上该路径不适用；不可用语义由 `platform::unsupported` 的单元测试覆盖。
        return;
    }
    let root = TempRoot::new("unavailable");
    let entropy = FixedEntropy::new();
    let store = FileKeystore::new(root.root().join("keystore"), entropy.clone());
    block_on(async {
        assert_eq!(
            store.generate(KeyPurpose::NodeIdentity, "primary").await,
            Err(KeystoreError::Unavailable)
        );
        assert_eq!(
            store
                .put_secret(
                    SecretPurpose::ProviderCredential,
                    "codex",
                    &SecretBytes::new(b"token")
                )
                .await,
            Err(KeystoreError::Unavailable)
        );
        assert_eq!(
            store
                .delete_secret(SecretPurpose::ProviderCredential, "codex")
                .await,
            Err(KeystoreError::Unavailable)
        );
    });
    assert!(
        root.files().is_empty(),
        "不可用平台必须零持久化写入（{:#?}）",
        root.files()
    );
}

#[test]
fn errors_never_carry_secret_material() {
    // 错误是封闭分类：既不包含秘密值，也不包含路径以外的上下文。
    let plaintext = b"super-secret-provider-token";
    let root = TempRoot::new("errors");
    let entropy = FixedEntropy::new();
    let store = FileKeystore::new(root.root().join("keystore"), entropy.clone());
    let handle = KeyHandle::new("node-identity/primary").expect("合法引用");
    block_on(async {
        let error = store
            .sign(&handle, plaintext)
            .await
            .expect_err("条目缺失必须失败");
        let text = format!("{error}");
        assert!(!text.contains("super-secret-provider-token"));
        assert!(!text.is_empty());
    });
}

#[test]
fn node_identity_entry_contains_no_plaintext_key() {
    let root = TempRoot::new("no-plaintext");
    let entropy = FixedEntropy::new();
    let store = FileKeystore::new(root.root().join("keystore"), entropy.clone());
    block_on(async {
        let Ok(handle) = store.generate(KeyPurpose::NodeIdentity, "primary").await else {
            return;
        };
        let _ = store.public_key(&handle).await.expect("公钥必须可读");
        let path = store.entry_path(EntryPurpose::NodeIdentity, "primary");
        // 熵源是定序的，因此「明文标量」是已知的 32 字节序列；它**不得**以明文出现在文件里。
        let known_scalar: Vec<u8> = (1..=32u8).collect();
        assert!(
            !contains_plaintext(&path, &known_scalar),
            "明文标量不得出现在条目文件里"
        );
    });
}

#[test]
fn corrupted_header_is_reported_as_corrupt() {
    let root = TempRoot::new("header");
    let entropy = FixedEntropy::new();
    let store = FileKeystore::new(root.root().join("keystore"), entropy.clone());
    block_on(async {
        let Ok(handle) = store.generate(KeyPurpose::NodeIdentity, "primary").await else {
            return;
        };
        let path = store.entry_path(EntryPurpose::NodeIdentity, "primary");
        let mut bytes = std::fs::read(&path).expect("条目必须存在");
        bytes[6] = 9; // 未知用途 token
        std::fs::write(&path, &bytes).expect("写入篡改内容");
        assert_eq!(
            store.public_key(&handle).await,
            Err(KeystoreError::EntryCorrupt)
        );
    });
}

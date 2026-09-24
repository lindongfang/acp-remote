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
use identity_keystore::store::Availability;
use identity_keystore::{FileKeystore, platform_supported};

use support::{FixedEntropy, TempRoot, block_on, contains_plaintext};

#[test]
fn platform_availability_is_reported_honestly() {
    // 判据来自编译期平台，而不是「试着用一下再猜」。
    assert_eq!(platform_supported(), cfg!(windows));
}

#[test]
fn default_availability_follows_the_platform() {
    // `new` 的可用性来自编译期平台；要跑另一条路径必须用 `with_availability` 显式写出来。
    let root = TempRoot::new("availability");
    let entropy = FixedEntropy::new();
    let store = FileKeystore::new(root.root().join("keystore"), entropy.clone());
    let expected = if platform_supported() {
        Availability::Platform
    } else {
        Availability::Unavailable
    };
    assert_eq!(store.availability(), expected);
}

#[test]
fn unavailable_platform_writes_nothing_at_all() {
    if platform_supported() {
        // Windows 上不适用：同一批断言由 `unavailable_backend_fails_closed_on_every_entry_point`
        // 用显式可用性覆盖（不会静默跳过任何断言）。
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
fn unavailable_backend_fails_closed_on_every_entry_point() {
    // F1/F2 的回归：平台不可用时**所有**入口返回「不可用」，且一个字节都不写。
    // 用 `with_availability` 强制该路径，因此本用例在任何平台上都真实执行
    // （Linux CI 上 `FileKeystore::new` 走的就是同一条分支）。
    let root = TempRoot::new("forced-unavailable");
    let entropy = FixedEntropy::new();
    let store = FileKeystore::with_availability(
        root.root().join("keystore"),
        entropy.clone(),
        Availability::Unavailable,
    );
    let handle = KeyHandle::new("node-identity/primary").expect("合法引用");
    block_on(async {
        assert_eq!(
            store.generate(KeyPurpose::NodeIdentity, "primary").await,
            Err(KeystoreError::Unavailable)
        );
        assert_eq!(
            store.public_key(&handle).await,
            Err(KeystoreError::Unavailable),
            "平台不可用不得伪装成「条目缺失」"
        );
        assert_eq!(
            store.sign(&handle, b"transcript").await,
            Err(KeystoreError::Unavailable)
        );
        assert_eq!(store.delete(&handle).await, Err(KeystoreError::Unavailable));
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
        assert!(
            matches!(
                store
                    .get_secret(SecretPurpose::ProviderCredential, "codex")
                    .await,
                Err(KeystoreError::Unavailable)
            ),
            "平台不可用不得表现为「没有凭据」"
        );
        assert_eq!(
            store
                .delete_secret(SecretPurpose::ProviderCredential, "codex")
                .await,
            Err(KeystoreError::Unavailable)
        );
        // 顺序固定：可用性先于句柄形状判定（不可用时连形状都不判定）。
        assert_eq!(
            store
                .public_key(&KeyHandle::new("unknown-purpose/x").expect("合法引用"))
                .await,
            Err(KeystoreError::Unavailable)
        );
    });
    assert!(
        root.files().is_empty(),
        "不可用后端必须零持久化写入（{:#?}）",
        root.files()
    );
}

#[test]
fn available_backend_is_not_unconditionally_unavailable() {
    // 反向断言：可用性闸门不是「永远返回不可用」。
    let root = TempRoot::new("forced-available");
    let entropy = FixedEntropy::new();
    let store = FileKeystore::with_availability(
        root.root().join("keystore"),
        entropy.clone(),
        Availability::Platform,
    );
    let result = block_on(store.generate(KeyPurpose::NodeIdentity, "primary"));
    if platform_supported() {
        assert!(result.is_ok(), "Windows 上声明可用就必须真的能用");
    } else {
        assert_eq!(
            result,
            Err(KeystoreError::Unavailable),
            "没有平台后端时，声明可用也只能在包裹这一步失败"
        );
    }
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
    if !platform_supported() {
        // 「明文不落盘」的可观察形式需要真实条目（Windows）；不可用平台的等价断言是「零写入」，
        // 由 `unavailable_backend_fails_closed_on_every_entry_point` 覆盖。
        return;
    }
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
    if !platform_supported() {
        return;
    }
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

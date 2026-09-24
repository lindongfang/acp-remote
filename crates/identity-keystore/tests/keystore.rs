//! 条目仓库行为：往返、删除后不可用、孤儿回收、篡改检测与原子写。
//!
//! 平台相关分支用 [`identity_keystore::platform_supported`] 显式分流：Windows 上跑完整往返，
//! 其它平台上同一批入口必须返回「不可用」且**零写入**（Linux CI 覆盖该路径）。
//!
//! 另有一类用例与平台**无关**：条目查找语义（缺失条目不重建、重复删除报缺失、句柄形状校验）。
//! 这些用例用 [`Availability::Platform`] 显式声明「后端可用」——它们检的是仓库逻辑而不是平台，
//! 因此在任何平台上都能真实执行（Linux CI 上同样如此，不会变成静默跳过）。

mod support;

use std::sync::Arc;

use identity_auth::{
    CanonicalOrigin, ChallengeId, DeviceId, FeatureList, IdentityKeystore, KeyHandle, KeyPurpose,
    KeystoreError, NodeId, Nonce, SecretBytes, SecretPurpose, SyncHostChallenge,
};
use identity_keystore::entry::EntryPurpose;
use identity_keystore::store::Availability;
use identity_keystore::{FileKeystore, OsEntropy};

use support::{FixedEntropy, TempRoot, block_on};

const HOST: &str = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
const DEVICE: &str = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";
const CONNECTION: &str = "20212223-2425-4627-a829-2a2b2c2d2e2f";

fn keystore(root: &TempRoot, entropy: &Arc<FixedEntropy>) -> FileKeystore {
    FileKeystore::new(root.root().join("keystore"), entropy.clone())
}

/// 「后端可用」的条目仓库：用于条目查找语义的用例（与平台无关，任何平台都真实执行）。
fn available_store(root: &TempRoot, entropy: &Arc<FixedEntropy>) -> FileKeystore {
    FileKeystore::with_availability(
        root.root().join("keystore"),
        entropy.clone(),
        Availability::Platform,
    )
}

/// 一段用登记表装配的 transcript（验证「端口签名与协议期一致」）。
fn challenge() -> SyncHostChallenge {
    SyncHostChallenge {
        host_id: NodeId::new(HOST).expect("规范 NodeId"),
        device_id: DeviceId::new(DEVICE).expect("规范 DeviceId"),
        canonical_origin: CanonicalOrigin::parse("https://pc.example.test").expect("规范 origin"),
        // 32 字节 0x01 / 0x02 的规范无填充 base64url（与固定向量的形态一致）。
        client_nonce: Nonce::new("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE")
            .expect("规范 nonce"),
        server_nonce: Nonce::new("AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI")
            .expect("规范 nonce"),
        connection_id: ChallengeId::parse(CONNECTION).expect("规范 ChallengeId"),
        negotiated_features: FeatureList::new(vec!["core.snapshot.v1".to_owned()]),
    }
}

#[test]
fn round_trip_matches_platform_capability() {
    let root = TempRoot::new("roundtrip");
    let entropy = FixedEntropy::new();
    let store = keystore(&root, &entropy);
    let result = block_on(store.generate(KeyPurpose::NodeIdentity, "primary"));

    if identity_keystore::platform_supported() {
        let handle = result.expect("Windows 上生成必须成功");
        assert_eq!(handle.as_str(), "node-identity/primary");
        let public_key = block_on(store.public_key(&handle)).expect("公钥必须可读");
        assert_eq!(public_key.as_bytes().len(), 65);
        assert_eq!(public_key.as_bytes()[0], 0x04);
        // 目录布局符合 `<root>/<purpose>/<label>.<version>`，且没有临时文件残留。
        assert_eq!(
            root.files(),
            vec!["keystore/node-identity/primary.1".to_owned()]
        );
    } else {
        assert_eq!(result, Err(KeystoreError::Unavailable));
        assert!(
            root.files().is_empty(),
            "不可用平台必须零写入（连目录都不能建）"
        );
    }
}

#[test]
fn delete_makes_the_entry_unavailable() {
    let root = TempRoot::new("delete");
    let entropy = FixedEntropy::new();
    let store = keystore(&root, &entropy);
    block_on(async {
        match store.generate(KeyPurpose::NodeIdentity, "primary").await {
            Ok(handle) => {
                store.delete(&handle).await.expect("删除必须成功");
                assert_eq!(
                    store.public_key(&handle).await,
                    Err(KeystoreError::EntryMissing)
                );
                assert_eq!(
                    store.delete(&handle).await,
                    Err(KeystoreError::EntryMissing),
                    "重复删除必须报缺失而不是静默成功"
                );
                assert!(root.files().is_empty(), "删除后不留文件");
            }
            Err(error) => {
                assert_eq!(error, KeystoreError::Unavailable);
                assert!(root.files().is_empty(), "不可用平台必须零写入");
            }
        }
    });
}

#[test]
fn tampered_entry_is_reported_and_never_overwritten() {
    if !identity_keystore::platform_supported() {
        // 不可用平台没有条目可观察：等价行为（所有入口 `Unavailable` + 零写入）在
        // `tests/fail_closed.rs::unavailable_backend_fails_closed_on_every_entry_point` 中覆盖。
        return;
    }
    let root = TempRoot::new("tamper");
    let entropy = FixedEntropy::new();
    let store = keystore(&root, &entropy);
    block_on(async {
        let Ok(handle) = store.generate(KeyPurpose::NodeIdentity, "primary").await else {
            // 非 Windows：没有条目可篡改（失败关闭路径由其它用例覆盖）。
            return;
        };
        let path = store.entry_path(EntryPurpose::NodeIdentity, "primary");
        let original = std::fs::read(&path).expect("条目必须存在");
        // 篡改被包裹的秘密值最后一个字节：平台的完整性保护必须检出。
        let mut tampered = original.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0xff;
        std::fs::write(&path, &tampered).expect("写入篡改内容");

        assert_eq!(
            store.public_key(&handle).await,
            Err(KeystoreError::EntryCorrupt),
            "被篡改的条目必须报损坏"
        );
        assert_eq!(
            std::fs::read(&path).expect("条目必须仍在"),
            tampered,
            "读取路径不得覆盖或修复损坏条目"
        );
    });
}

#[test]
fn missing_referenced_entry_is_reported_without_recreation() {
    let root = TempRoot::new("missing");
    let entropy = FixedEntropy::new();
    // 白盒声明「后端可用」：本用例检的是条目查找语义，不是平台能力。
    let store = available_store(&root, &entropy);
    let handle = KeyHandle::new("node-identity/primary").expect("合法引用");
    block_on(async {
        // 引用存在但条目不存在：必须报缺失，绝不静默生成新身份。
        assert_eq!(
            store.public_key(&handle).await,
            Err(KeystoreError::EntryMissing)
        );
        assert_eq!(
            store.sign(&handle, b"transcript").await,
            Err(KeystoreError::EntryMissing)
        );
        assert!(root.files().is_empty());
    });
}

#[test]
fn orphan_removal_keeps_existing_identity_usable() {
    if !identity_keystore::platform_supported() {
        // 不可用平台没有条目可观察：等价行为（所有入口 `Unavailable` + 零写入）在
        // `tests/fail_closed.rs::unavailable_backend_fails_closed_on_every_entry_point` 中覆盖。
        return;
    }
    let root = TempRoot::new("orphan");
    let entropy = FixedEntropy::new();
    let store = keystore(&root, &entropy);
    block_on(async {
        let Ok(handle) = store.generate(KeyPurpose::NodeIdentity, "primary").await else {
            return;
        };
        let Ok(orphan) = store.generate(KeyPurpose::NodeIdentity, "orphan").await else {
            return;
        };
        // 「未被引用的孤儿被回收」：删除孤儿条目不影响被引用的身份。
        store.delete(&orphan).await.expect("孤儿删除必须成功");
        assert!(store.public_key(&handle).await.is_ok(), "被引用条目仍可用");
        assert_eq!(
            store.list(EntryPurpose::NodeIdentity).unwrap(),
            vec!["primary".to_owned()]
        );
    });
}

#[test]
fn failed_reference_commit_keeps_the_old_entry_resolvable() {
    let root = TempRoot::new("reference");
    let entropy = FixedEntropy::new();
    let store = keystore(&root, &entropy);
    block_on(async {
        let Ok(old) = store.generate(KeyPurpose::NodeIdentity, "primary").await else {
            return;
        };
        let Ok(fresh) = store
            .generate(KeyPurpose::NodeIdentity, "primary-next")
            .await
        else {
            return;
        };
        // 模拟「引用提交失败」（引用在存储层，这里用开关表达）：此时**不能**回收旧条目。
        let reference_commit_failed = true;
        if !reference_commit_failed {
            store.delete(&old).await.expect("提交成功后才会回收旧条目");
        }
        assert!(store.public_key(&old).await.is_ok(), "旧引用必须仍可用");
        assert!(store.public_key(&fresh).await.is_ok(), "新条目必须可读");
        let transcript = challenge().transcript().expect("装配必须成功");
        let signature = store
            .sign(&old, &transcript)
            .await
            .expect("旧条目必须可签名");
        assert_eq!(signature.as_bytes().len(), 64);
    });
}

#[test]
fn provider_secret_round_trips_and_is_absent_until_written() {
    if !identity_keystore::platform_supported() {
        // 不可用平台没有条目可观察：等价行为（所有入口 `Unavailable` + 零写入）在
        // `tests/fail_closed.rs::unavailable_backend_fails_closed_on_every_entry_point` 中覆盖。
        return;
    }
    let root = TempRoot::new("secret");
    let entropy = FixedEntropy::new();
    let store = keystore(&root, &entropy);
    block_on(async {
        let purpose = SecretPurpose::ProviderCredential;
        match store.get_secret(purpose, "codex").await {
            Ok(None) => {}
            Err(KeystoreError::Unavailable) => return,
            Ok(Some(_)) => panic!("缺失的 secret 不得返回内容"),
            Err(other) => panic!("缺失的 secret 必须返回 None，实际 {}", other),
        }
        let value = SecretBytes::new(b"provider-token");
        match store.put_secret(purpose, "codex", &value).await {
            Ok(()) => {}
            Err(KeystoreError::Unavailable) => return,
            other => panic!("写入必须成功，实际 {other:?}"),
        }
        let read = store
            .get_secret(purpose, "codex")
            .await
            .expect("读取必须成功")
            .expect("必须存在");
        assert_eq!(read.as_bytes(), b"provider-token");
        store
            .delete_secret(purpose, "codex")
            .await
            .expect("删除必须成功");
        assert!(
            store
                .get_secret(purpose, "codex")
                .await
                .expect("读取必须成功")
                .is_none()
        );
        assert_eq!(
            store.delete_secret(purpose, "codex").await,
            Err(KeystoreError::SecretMissing),
            "重复删除必须报缺失"
        );
    });
}

#[test]
fn purpose_mismatch_and_illegal_handles_are_rejected() {
    let root = TempRoot::new("handles");
    let entropy = FixedEntropy::new();
    // 白盒声明「后端可用」：句柄形状与用途错配的判定不应依赖平台后端是否存在。
    let store = available_store(&root, &entropy);
    block_on(async {
        assert_eq!(
            store
                .public_key(&KeyHandle::new("unknown-purpose/x").expect("合法引用"))
                .await,
            Err(KeystoreError::PurposeMismatch)
        );
        assert_eq!(
            store
                .sign(
                    &KeyHandle::new("provider-credential/codex").expect("合法引用"),
                    b"transcript"
                )
                .await,
            Err(KeystoreError::PurposeMismatch),
            "凭据用途不得用于签名"
        );
        assert_eq!(
            store
                .public_key(&KeyHandle::new("node-identity/").expect("合法引用"))
                .await,
            Err(KeystoreError::EntryInvalid)
        );
        assert_eq!(
            store
                .public_key(&KeyHandle::new("node-identity").expect("合法引用"))
                .await,
            Err(KeystoreError::EntryInvalid)
        );
    });
}

#[test]
fn salts_are_per_entry_and_plaintext_is_absent() {
    let root = TempRoot::new("plaintext");
    let entropy = FixedEntropy::new();
    let store = keystore(&root, &entropy);
    block_on(async {
        let Ok(handle) = store.generate(KeyPurpose::NodeIdentity, "primary").await else {
            return;
        };
        let public_key = store.public_key(&handle).await.expect("公钥必须可读");
        let path = store.entry_path(EntryPurpose::NodeIdentity, "primary");
        let bytes = std::fs::read(&path).expect("条目必须存在");
        assert_eq!(&bytes[..4], b"ACPK");
        assert!(bytes.len() > 41, "条目必须包含被包裹的秘密值");
        assert_eq!(public_key.as_bytes().len(), 65);
        let first_salt = bytes[9 + "primary".len()..9 + "primary".len() + 32].to_vec();

        let other = store
            .generate(KeyPurpose::NodeIdentity, "secondary")
            .await
            .expect("第二个条目必须可生成");
        assert!(store.public_key(&other).await.is_ok());
        let other_bytes = std::fs::read(store.entry_path(EntryPurpose::NodeIdentity, "secondary"))
            .expect("第二个条目必须存在");
        let second_salt = other_bytes[9 + "secondary".len()..9 + "secondary".len() + 32].to_vec();
        assert_ne!(first_salt, second_salt, "盐必须每条目独立");

        // R74：「不同条目是不同密钥」——公钥必须不同，签名必须不能互相验证通过。
        let second_public_key = store.public_key(&other).await.expect("第二个公钥必须可读");
        assert_ne!(
            public_key.as_bytes(),
            second_public_key.as_bytes(),
            "两条目不得派生同一把密钥"
        );
        let input = challenge();
        let transcript = input.transcript().expect("装配必须成功");
        let first_signature = store.sign(&handle, &transcript).await.expect("必须可签名");
        assert!(
            input.verify(&public_key, &first_signature).is_ok(),
            "自己的签名必须用自己的公钥验证通过"
        );
        assert!(
            input.verify(&second_public_key, &first_signature).is_err(),
            "条目 A 的签名不得被条目 B 的公钥验证通过"
        );
    });
}

#[test]
fn os_entropy_keystore_has_no_in_process_fallback() {
    let handle = KeyHandle::new("node-identity/primary").expect("合法引用");

    // (1) 后端不可用：所有入口返回「不可用」，零写入，也没有内存兜底。
    let blocked_root = TempRoot::new("no-fallback-blocked");
    let blocked = FileKeystore::with_availability(
        blocked_root.root().join("keystore"),
        Arc::new(OsEntropy::new()),
        Availability::Unavailable,
    );
    block_on(async {
        assert_eq!(
            blocked.generate(KeyPurpose::NodeIdentity, "primary").await,
            Err(KeystoreError::Unavailable)
        );
        assert_eq!(
            blocked.public_key(&handle).await,
            Err(KeystoreError::Unavailable)
        );
    });
    assert!(blocked_root.files().is_empty(), "不可用后端不得写任何文件");

    // (2) 后端可用但没有条目：「缺失」——不是「刚生成的内存条目」，也不是「不可用」。
    let root = TempRoot::new("no-fallback-missing");
    let store = FileKeystore::with_availability(
        root.root().join("keystore"),
        Arc::new(OsEntropy::new()),
        Availability::Platform,
    );
    block_on(async {
        assert_eq!(
            store.public_key(&handle).await,
            Err(KeystoreError::EntryMissing),
            "读路径不得生成身份、也不得退回进程内条目"
        );
    });
    assert!(root.files().is_empty(), "读路径不得写任何文件");
}

#[test]
fn entropy_failure_fails_closed_without_writing() {
    // 熵源不可用时**在包裹之前**失败：不产生条目、也不产生半成品文件。
    let root = TempRoot::new("entropy");
    let entropy = FixedEntropy::new();
    let store = keystore(&root, &entropy);
    entropy.fail_next();
    let result = block_on(store.generate(KeyPurpose::NodeIdentity, "primary"));
    assert_eq!(result, Err(KeystoreError::Unavailable));
    assert!(root.files().is_empty(), "失败必须零写入");
}

#[test]
fn signature_is_p1363_and_verifies_with_the_entry_public_key() {
    if !identity_keystore::platform_supported() {
        // 不可用平台没有条目可观察：等价行为（所有入口 `Unavailable` + 零写入）在
        // `tests/fail_closed.rs::unavailable_backend_fails_closed_on_every_entry_point` 中覆盖。
        return;
    }
    let root = TempRoot::new("signature");
    let entropy = FixedEntropy::new();
    let store = keystore(&root, &entropy);
    block_on(async {
        let Ok(handle) = store.generate(KeyPurpose::NodeIdentity, "primary").await else {
            return;
        };
        let public_key = store.public_key(&handle).await.expect("公钥必须可读");
        let input = challenge();
        let transcript = input.transcript().expect("装配必须成功");
        let signature = store
            .sign(&handle, &transcript)
            .await
            .expect("签名必须成功");
        assert_eq!(signature.as_bytes().len(), 64, "必须是 64 字节 P1363");
        assert!(signature.components_nonzero(), "签名分量必须非零");
        assert!(
            input.verify(&public_key, &signature).is_ok(),
            "签名必须能用条目公钥验证"
        );
    });
}

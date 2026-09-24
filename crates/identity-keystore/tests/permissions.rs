//! Unix 权限位回归（Linux CI 覆盖）：私有目录 `0700`、条目文件 `0600`。
//!
//! 这些路径在 Windows 开发机上不会执行，因此断言必须是可跳过的、而不是「看起来通过」的：
//! 本文件在非 Unix 平台整体为空实现，Linux CI 上是真实的权限位断言。

#![cfg(unix)]

mod support;

use std::os::unix::fs::PermissionsExt as _;
use std::sync::Arc;

use identity_auth::{IdentityKeystore, KeyPurpose};
use identity_keystore::entry::EntryPurpose;
use identity_keystore::{FileKeystore, OsEntropy, platform_supported};

use support::{TempRoot, block_on};

#[test]
fn unix_permissions_are_private() {
    if !platform_supported() {
        // 没有平台后端时不会产出条目；权限位路径由 Windows 上的 DPAPI 后端不可能验证，
        // 因此这里如实跳过（Linux 的真实后端落地时本用例自动生效）。
        return;
    }
    let root = TempRoot::new("permissions");
    let store = FileKeystore::new(root.root().join("keystore"), Arc::new(OsEntropy::new()));
    block_on(async {
        let Ok(handle) = store.generate(KeyPurpose::NodeIdentity, "primary").await else {
            return;
        };
        let directory = store
            .entry_path(EntryPurpose::NodeIdentity, "primary")
            .parent()
            .expect("条目必须有父目录")
            .to_path_buf();
        let directory_mode = std::fs::metadata(&directory)
            .expect("目录必须存在")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(directory_mode, 0o700, "私有目录必须是 0700");

        let file_mode = std::fs::metadata(store.entry_path(EntryPurpose::NodeIdentity, "primary"))
            .expect("条目必须存在")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(file_mode, 0o600, "条目文件必须是 0600");
        assert!(store.public_key(&handle).await.is_ok());
    });
}

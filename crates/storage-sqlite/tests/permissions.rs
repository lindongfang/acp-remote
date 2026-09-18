//! §7.1/§9.12：权限判定的纯函数（合成权限视图）+ 正式模式的失败关闭。

mod support;

use std::path::Path;

use storage_sqlite::migrate::{PermissionView, enforce_path_permissions};
use support::*;

/// §9.12：「组/其他可写」视图必须判为宽松。
#[test]
fn group_or_other_writable_views_are_relaxed() {
    let cases = [
        PermissionView {
            group_write: true,
            ..PermissionView::default()
        },
        PermissionView {
            other_write: true,
            ..PermissionView::default()
        },
        PermissionView {
            other_read: true,
            ..PermissionView::default()
        },
        PermissionView {
            group_read: true,
            ..PermissionView::default()
        },
    ];
    for view in cases {
        assert!(view.is_relaxed(), "{view:?} must be relaxed");
    }
    assert!(!PermissionView::default().is_relaxed(), "owner-only");
}

/// §9.12：「非当前用户可读写」的 ACL 视图同样判为宽松。
#[test]
fn foreign_principal_views_are_relaxed() {
    for view in [
        PermissionView {
            foreign_principal_read: true,
            ..PermissionView::default()
        },
        PermissionView {
            foreign_principal_write: true,
            ..PermissionView::default()
        },
    ] {
        assert!(view.is_relaxed(), "{view:?} must be relaxed");
    }
}

/// Unix 模式位映射：`0700`/`0600` 是目标状态，任何 group/other 位都算宽松。
#[test]
fn unix_mode_bits_map_to_the_expected_verdicts() {
    for mode in [0o700_u32, 0o600, 0o500, 0o400] {
        assert!(
            !PermissionView::from_unix_mode(mode).is_relaxed(),
            "{mode:o} is owner-only"
        );
    }
    for mode in [0o770_u32, 0o707, 0o660, 0o604, 0o620, 0o755] {
        assert!(
            PermissionView::from_unix_mode(mode).is_relaxed(),
            "{mode:o} grants access beyond the owner"
        );
    }
}

/// §7.1：正式模式下宽松即失败关闭；数据目录由本 crate 以 `0700` 创建，因此正常路径不会触发。
#[cfg(unix)]
#[tokio::test]
async fn relaxed_data_directory_fails_closed_in_strict_mode() {
    use acp_core::model::Timestamp;
    use std::os::unix::fs::PermissionsExt;
    use storage_sqlite::migrate::{PermissionCheck, StorageConfig, inspect_path_permissions};
    use storage_sqlite::session_store::SqliteStore;

    let dir = temp_dir("permissions-strict");
    let strict = StorageConfig::new(&dir);
    // 目录本来就是新建的 0700；把它放松成 0777 后必须被拒。
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).expect("chmod");
    assert!(matches!(
        inspect_path_permissions(&dir).expect("inspect"),
        PermissionCheck::Relaxed(_)
    ));
    let error = SqliteStore::open(strict, &Timestamp::new(AT).expect("timestamp"))
        .await
        .expect_err("strict mode must refuse a world-writable directory");
    assert!(matches!(
        error,
        storage_sqlite::error::StorageError::InsecurePermissions { .. }
    ));

    // 非正式模式（不检查权限）可以继续。
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).expect("chmod");
    let mut lenient = StorageConfig::new(&dir);
    lenient.strict_permissions = false;
    let store = SqliteStore::open(lenient, &Timestamp::new(AT).expect("timestamp"))
        .await
        .expect("lenient mode opens");
    store.close().await;
}

/// `enforce_path_permissions` 在无法判定时**不**失败关闭（Windows 的 ACL 读取见 crate 文档）。
#[test]
fn enforcement_only_fails_on_observed_relaxation() {
    let dir = temp_dir("permissions-owner-only");
    assert!(enforce_path_permissions(&dir).is_ok());
    // 不存在的路径：Unix 上读模式位失败 → I/O 错误；Windows 上无法读取 ACL（`Unverifiable`），
    // 按设计**不**失败关闭（失败关闭针对已观察到的宽松权限，见 `migrate` 的文档）。
    let missing = enforce_path_permissions(Path::new("definitely-not-there"));
    #[cfg(unix)]
    assert!(missing.is_err());
    #[cfg(not(unix))]
    assert!(missing.is_ok());
}

/// `open` 后的数据库/WAL 文件在 Unix 上是 `0600`、目录是 `0700`。
#[cfg(unix)]
#[tokio::test]
async fn created_files_are_owner_only() {
    use acp_core::model::Timestamp;
    use std::os::unix::fs::PermissionsExt;
    use storage_sqlite::migrate::StorageConfig;
    use storage_sqlite::session_store::SqliteStore;

    let dir = temp_dir("permissions-files");
    let store = SqliteStore::open(
        StorageConfig::new(&dir),
        &Timestamp::new(AT).expect("timestamp"),
    )
    .await
    .expect("open");
    store.close().await;

    let mode = |path: &Path| {
        std::fs::metadata(path)
            .unwrap_or_else(|error| panic!("{path:?}: {error}"))
            .permissions()
            .mode()
            & 0o777
    };
    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&dir.join("attachments")), 0o700);
    assert_eq!(mode(&dir.join("acp-remote.sqlite3")), 0o600);
}

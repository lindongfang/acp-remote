//! `direct` 模式证书/私钥文件的平台权限判定（`SECURITY_DESIGN.md` §13.2）。
//!
//! 判定被拆成**纯函数**（输入是合成的权限视图）以便在任一平台上单测；真实平台读取只在
//! [`inspect_file_permissions`] 里发生。形状与既有 `storage-sqlite` 的判定同构
//! （`docs/CORE_PORTS_AND_STORAGE.md` §7.1 的 `[决定]`、§9 判据 12）——这里不能复用那个实现，
//! 因为 `server` 与 `storage-sqlite` 是平级适配器，互不依赖（`AGENTS.md` §4）。
//!
//! **Windows 的 ACL 读取**：workspace 以 forbid 级 lint 禁止不安全代码（根 `Cargo.toml` 的
//! `[workspace.lints.rust]`），且没有 ACL 封装依赖，因此
//! Windows 返回 [`PermissionCheck::Unverifiable`]，**不**失败关闭，只记一次结构化警告——这正是
//! `docs/CORE_PORTS_AND_STORAGE.md` §7.1 已裁定的平台口径（`[已裁定]` 条目），本模块沿用同一口径
//! 而不是另立一套。Unix 仍按真实模式位判定并失败关闭。

use std::path::Path;

/// 合成权限视图：除所有者位以外的任何读/写授予都算宽松。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PermissionView {
    /// 同组用户可读。
    pub group_read: bool,
    /// 同组用户可写。
    pub group_write: bool,
    /// 其他用户可读。
    pub other_read: bool,
    /// 其他用户可写。
    pub other_write: bool,
    /// 存在非当前用户的显式 ACL 条目（Windows ACL 视图）。
    pub foreign_principal_read: bool,
    /// 存在非当前用户的显式 ACL 写条目（Windows ACL 视图）。
    pub foreign_principal_write: bool,
}

impl PermissionView {
    /// §13.2 的目标状态是「只允许当前 OS 用户访问」，因此任何额外的读/写授予都算宽松。
    pub fn is_relaxed(self) -> bool {
        self.group_read
            || self.group_write
            || self.other_read
            || self.other_write
            || self.foreign_principal_read
            || self.foreign_principal_write
    }

    /// Unix 模式位 → 视图。
    pub fn from_unix_mode(mode: u32) -> Self {
        Self {
            group_read: mode & 0o040 != 0,
            group_write: mode & 0o020 != 0,
            other_read: mode & 0o004 != 0,
            other_write: mode & 0o002 != 0,
            foreign_principal_read: false,
            foreign_principal_write: false,
        }
    }
}

/// 一次权限检查的结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionCheck {
    /// 证据表明只有当前用户可访问。
    OwnerOnly,
    /// 证据表明存在额外的读/写授予。
    Relaxed(PermissionView),
    /// 平台无法在不引入不安全代码的前提下读取 ACL。
    Unverifiable,
}

/// 读取文件的权限证据（`Err` 表示文件不可读或不存在）。
#[cfg(unix)]
pub fn inspect_file_permissions(path: &Path) -> std::io::Result<PermissionCheck> {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = std::fs::metadata(path)?.permissions().mode();
    let view = PermissionView::from_unix_mode(mode);
    Ok(if view.is_relaxed() {
        PermissionCheck::Relaxed(view)
    } else {
        PermissionCheck::OwnerOnly
    })
}

/// 非 Unix：见模块文档（ACL 不可读 → `Unverifiable`，不失败关闭）。
#[cfg(not(unix))]
pub fn inspect_file_permissions(_path: &Path) -> std::io::Result<PermissionCheck> {
    Ok(PermissionCheck::Unverifiable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_only_modes_are_not_relaxed() {
        assert!(!PermissionView::from_unix_mode(0o600).is_relaxed());
        assert!(!PermissionView::from_unix_mode(0o700).is_relaxed());
    }

    #[test]
    fn group_or_other_grants_are_relaxed() {
        for mode in [0o640, 0o620, 0o604, 0o602, 0o644, 0o666, 0o770] {
            assert!(
                PermissionView::from_unix_mode(mode).is_relaxed(),
                "{mode:o} 必须判为宽松"
            );
        }
    }

    #[test]
    fn foreign_principal_acl_entries_are_relaxed() {
        let view = PermissionView {
            foreign_principal_read: true,
            ..PermissionView::default()
        };
        assert!(view.is_relaxed());
    }

    #[cfg(unix)]
    #[test]
    fn unix_inspection_reports_relaxed_for_world_readable_file() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = std::env::temp_dir().join(format!("acpr-net-perm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("临时目录");
        let path = dir.join("key.pem");
        std::fs::write(&path, b"not a real key").expect("写文件");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("chmod 600");
        assert_eq!(
            inspect_file_permissions(&path).expect("读取权限"),
            PermissionCheck::OwnerOnly
        );
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod 644");
        assert!(matches!(
            inspect_file_permissions(&path).expect("读取权限"),
            PermissionCheck::Relaxed(_)
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(not(unix))]
    #[test]
    fn windows_inspection_is_unverifiable() {
        // Windows 上不写不安全代码就读不到 ACL，因此判定是 `Unverifiable`（不失败关闭，只告警）。
        let path = std::env::temp_dir().join(format!("acpr-net-perm-{}", std::process::id()));
        std::fs::write(&path, b"not a real key").expect("写文件");
        assert_eq!(
            inspect_file_permissions(&path).expect("读取权限"),
            PermissionCheck::Unverifiable
        );
        let _ = std::fs::remove_file(&path);
    }
}

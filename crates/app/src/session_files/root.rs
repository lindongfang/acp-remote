//! 根边界与实际打开对象校验（design D3）。
//!
//! 字符串前缀比较不足以证明一个路径仍在会话根内：symlink、Windows junction/reparse point、盘符
//! 大小写与 verbatim 前缀都会让「看起来在根内」的路径指向根外对象。本模块的规则是：
//!
//! 1. 每次请求都重新规范化根与会话根内的相对路径，**解析链接后的规范路径**必须逐组件落在规范根内；
//! 2. 请求形状再次按 `SessionFilePath` 的规则复核（core 已判定一次，这里是适配器的纵深防御）；
//! 3. 打开对象时比较**打开句柄的元数据身份**与解析时记录的身份；任一不同即拒绝。这样即便校验后
//!    链接被替换，也不会读取被替换后的对象；
//! 4. 目录链接指向自身或祖先时判为循环并拒绝，避免无限深入；
//! 5. `.git` 任一组件（大小写不敏感）永久不可见、不可读；Windows 设备保留名在打开前直接拒绝，
//!    避免 `CON`/`NUL` 一类路径触发设备语义或阻塞。
//!
//! 平台差异只体现在 [`FileIdentity`] 的字段：Unix 用 `dev + ino`（精确身份），Windows 用创建时间、
//! 修改时间、大小与属性（std 的 `file_index` 仍是不稳定 API，仓库固定 `unsafe_code = "forbid"`，
//! 不能直接取 Win32 句柄）。无法证明时一律拒绝，不猜测放行。

use std::fs::{self, File, Metadata, ReadDir};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use acp_core::model::{PortError, SessionFilePath, UnavailableKind};

/// 路径形状/管理路径不合法（wire 上映射为 `protocol.schema_invalid`）。
pub(crate) const INVALID_PATH: &str = "session file path is invalid";
/// 目标不存在、不可读或无法安全证明在根内（wire 上映射为 `internal.unavailable`）。
pub(crate) fn unavailable() -> PortError {
    PortError::Unavailable(UnavailableKind::IoError)
}

/// 复核根内相对路径的形状：拒绝空串、绝对路径、`..`/`.`、NUL、控制字符、冒号、重复分隔符与
/// `.git` 组件；Windows 上额外拒绝设备保留名。调用方拿到的 [`SessionFilePath`] 通常已经通过 core
/// 的判定，这里仍按实际打开前的输入再判一次（design D3：不能只依赖上游检查）。
pub(crate) fn validate_relative(path: &SessionFilePath) -> Result<(), PortError> {
    let text = path.as_str();
    if text.is_empty()
        || text
            .chars()
            .any(|c| c == '\0' || c.is_control() || c == ':')
        || text.starts_with(['/', '\\'])
    {
        return Err(PortError::InvalidRequest(INVALID_PATH));
    }
    let mut components = 0usize;
    for component in text.split(['/', '\\']) {
        if component.is_empty() || component == "." || component == ".." {
            return Err(PortError::InvalidRequest(INVALID_PATH));
        }
        if is_git_management_name(component) {
            return Err(PortError::InvalidRequest(INVALID_PATH));
        }
        if is_windows_reserved(component) {
            return Err(PortError::InvalidRequest(INVALID_PATH));
        }
        components += 1;
    }
    if components == 0 {
        return Err(PortError::InvalidRequest(INVALID_PATH));
    }
    Ok(())
}

/// `.git` 目录与 worktree 的 `.git` 管理文件都按同一规则隐藏：任一分量等于 `.git`（大小写不敏感）。
pub(crate) fn is_git_management_name(name: &str) -> bool {
    name.eq_ignore_ascii_case(".git")
}

/// Windows 设备保留名（含 `CONIN$`/`CONOUT$`）。只在 Windows 上启用：Unix 上 `CON` 只是普通文件名。
fn is_windows_reserved(name: &str) -> bool {
    if !cfg!(windows) {
        return false;
    }
    // Win32 会忽略结尾的空格与句点，因此 `CON `、`NUL.` 同样落在设备命名空间。
    let trimmed = name.trim_end_matches([' ', '.']);
    let stem = trimmed.split('.').next().unwrap_or(trimmed);
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
    ) || is_numbered_device(&upper, "COM")
        || is_numbered_device(&upper, "LPT")
}

fn is_numbered_device(upper: &str, prefix: &str) -> bool {
    upper
        .strip_prefix(prefix)
        .is_some_and(|rest| rest.len() == 1 && matches!(rest.as_bytes()[0], b'1'..=b'9'))
}

/// 一个实际打开对象的身份。用于「校验 → 打开」之间链接被替换的检测：
/// 只要打开后的句柄身份与解析时记录的不同，就拒绝本次读取。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FileIdentity {
    size: u64,
    created: Option<SystemTime>,
    modified: Option<SystemTime>,
    is_dir: bool,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(windows)]
    attributes: u32,
}

impl FileIdentity {
    pub(crate) fn of(metadata: &Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        #[cfg(windows)]
        use std::os::windows::fs::MetadataExt;

        Self {
            size: metadata.len(),
            created: metadata.created().ok(),
            modified: metadata.modified().ok(),
            is_dir: metadata.is_dir(),
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
            #[cfg(windows)]
            attributes: metadata.file_attributes(),
        }
    }
}

/// 解析并验证过的根内目标。
#[derive(Debug)]
pub(crate) struct Resolved {
    canonical: PathBuf,
    metadata: Metadata,
    identity: FileIdentity,
}

impl Resolved {
    pub(crate) fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    pub(crate) fn identity(&self) -> FileIdentity {
        self.identity
    }

    /// 打开普通文件并确认打开句柄仍是校验时那个对象。
    pub(crate) fn open_file(&self) -> Result<(File, Metadata), PortError> {
        if !self.metadata.is_file() {
            return Err(unavailable());
        }
        let file = File::open(&self.canonical).map_err(|_| unavailable())?;
        let metadata = file.metadata().map_err(|_| unavailable())?;
        if !metadata.is_file() || FileIdentity::of(&metadata) != self.identity {
            return Err(unavailable());
        }
        self.verify_path_unchanged()?;
        Ok((file, metadata))
    }

    /// 打开目录枚举器并确认目录对象仍在校验时的位置与身份上。
    pub(crate) fn open_dir(&self) -> Result<ReadDir, PortError> {
        if !self.metadata.is_dir() {
            return Err(unavailable());
        }
        let entries = fs::read_dir(&self.canonical).map_err(|_| unavailable())?;
        self.verify_path_unchanged()?;
        Ok(entries)
    }

    /// 读取普通文件的有界头部（用于类型分类与二进制判定）。
    pub(crate) fn read_head(&self, limit: usize) -> Result<Vec<u8>, PortError> {
        use std::io::Read;
        let (mut file, _) = self.open_file()?;
        let mut buffer = vec![0_u8; limit];
        let mut filled = 0usize;
        while filled < buffer.len() {
            match file.read(&mut buffer[filled..]) {
                Ok(0) => break,
                Ok(read) => filled += read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(unavailable()),
            }
        }
        buffer.truncate(filled);
        Ok(buffer)
    }

    /// 再次规范化同一路径并核对身份：覆盖「校验后链接被替换」的竞态。
    pub(crate) fn verify_path_unchanged(&self) -> Result<(), PortError> {
        let again = fs::canonicalize(&self.canonical).map_err(|_| unavailable())?;
        if again != self.canonical {
            return Err(unavailable());
        }
        let metadata = fs::metadata(&again).map_err(|_| unavailable())?;
        if FileIdentity::of(&metadata) != self.identity {
            return Err(unavailable());
        }
        Ok(())
    }
}

/// 一个已规范化的会话根。每次调用都重新规范化，避免长时间持有过期根。
#[derive(Debug, Clone)]
pub(crate) struct SecureRoot {
    canonical: PathBuf,
}

impl SecureRoot {
    pub(crate) fn open(root: &str) -> Result<Self, PortError> {
        let path = Path::new(root);
        if !path.is_absolute() {
            return Err(unavailable());
        }
        let canonical = fs::canonicalize(path).map_err(|_| unavailable())?;
        let metadata = fs::metadata(&canonical).map_err(|_| unavailable())?;
        if !metadata.is_dir() {
            return Err(unavailable());
        }
        Ok(Self { canonical })
    }

    /// 根内相对路径 → 解析后的实际对象。`rel = None` 表示会话根自身（只用于目录枚举）。
    pub(crate) fn resolve(&self, rel: Option<&SessionFilePath>) -> Result<Resolved, PortError> {
        if let Some(rel) = rel {
            validate_relative(rel)?;
        }
        let joined = match rel {
            Some(rel) => self.join(rel),
            None => self.canonical.clone(),
        };
        let canonical = fs::canonicalize(&joined).map_err(|_| unavailable())?;
        let relative = canonical
            .strip_prefix(&self.canonical)
            .map_err(|_| unavailable())?;
        // 规范路径中也不得出现 `.git`：内部链接可能把根内路径指向 `.git` 管理目录。
        if relative.components().any(|component| {
            component
                .as_os_str()
                .to_str()
                .is_some_and(is_git_management_name)
        }) {
            return Err(PortError::InvalidRequest(INVALID_PATH));
        }
        let metadata = fs::metadata(&canonical).map_err(|_| unavailable())?;
        if let Some(rel) = rel {
            self.reject_cycle(rel, &canonical)?;
        }
        Ok(Resolved {
            identity: FileIdentity::of(&metadata),
            canonical,
            metadata,
        })
    }

    /// 只解析目录（枚举与 Git 根都要求目录）。
    pub(crate) fn resolve_dir(&self, rel: Option<&SessionFilePath>) -> Result<Resolved, PortError> {
        let resolved = self.resolve(rel)?;
        if !resolved.metadata().is_dir() {
            return Err(unavailable());
        }
        Ok(resolved)
    }

    pub(crate) fn join(&self, rel: &SessionFilePath) -> PathBuf {
        let mut path = self.canonical.clone();
        for component in rel.as_str().split(['/', '\\']) {
            path.push(component);
        }
        path
    }

    /// 枚举防线：确认 `rel` 解析出的非链接对象确实是被枚举目录的直接子项。
    ///
    /// `read_dir` 打开后目录路径仍可能被替换成根外 junction 再换回；此时枚举器返回的名称
    /// 可能来自根外。逐项复核「规范父目录 == 已校验目录」后，无法证明归属的名称一律跳过。
    /// 链接不适用（链接本身可以是任意目录项），仍由 [`Self::inspect_child`] 做边界与循环检查。
    pub(crate) fn child_of(&self, dir: &Resolved, rel: &SessionFilePath) -> Option<Resolved> {
        let child = self.resolve(Some(rel)).ok()?;
        (child.canonical.parent() == Some(dir.canonical.as_path())).then_some(child)
    }

    /// 目录链接指向自身或祖先时形成循环：拒绝，避免 `link/link/...` 的无限深入。
    fn reject_cycle(&self, rel: &SessionFilePath, target: &Path) -> Result<(), PortError> {
        let components: Vec<&str> = rel.as_str().split(['/', '\\']).collect();
        let parent = if components.len() <= 1 {
            self.canonical.clone()
        } else {
            let mut parent = self.canonical.clone();
            for component in &components[..components.len() - 1] {
                parent.push(component);
            }
            fs::canonicalize(&parent).map_err(|_| unavailable())?
        };
        if target == parent || parent.starts_with(target) {
            return Err(unavailable());
        }
        Ok(())
    }

    /// 供列表使用：判定一个直接子项（跟随链接后）是否可安全打开，并返回受限原因。
    /// 失效链接、循环链接与特殊文件都仍返回条目，只是不可打开。
    pub(crate) fn inspect_child(
        &self,
        rel: &SessionFilePath,
    ) -> Result<ChildInspection, PortError> {
        let joined = self.join(rel);
        let link_metadata = fs::symlink_metadata(&joined).map_err(|_| unavailable())?;
        let is_link = link_metadata.file_type().is_symlink();
        if !is_link {
            let is_dir = link_metadata.is_dir();
            let is_file = link_metadata.is_file();
            return Ok(ChildInspection {
                is_link: false,
                target_is_dir: is_dir,
                openable: is_dir || is_file,
                restricted: if is_dir || is_file {
                    None
                } else {
                    Some(acp_core::model::SessionFileRestrictedReason::Unsupported)
                },
            });
        }
        let metadata = match fs::metadata(&joined) {
            Ok(metadata) => metadata,
            Err(_) => {
                return Ok(ChildInspection {
                    is_link: true,
                    target_is_dir: false,
                    openable: false,
                    restricted: Some(acp_core::model::SessionFileRestrictedReason::Unreadable),
                });
            }
        };
        if !metadata.is_file() && !metadata.is_dir() {
            return Ok(ChildInspection {
                is_link: true,
                target_is_dir: false,
                openable: false,
                restricted: Some(acp_core::model::SessionFileRestrictedReason::Unsupported),
            });
        }
        let canonical = match fs::canonicalize(&joined) {
            Ok(path) => path,
            Err(_) => {
                return Ok(ChildInspection {
                    is_link: true,
                    target_is_dir: metadata.is_dir(),
                    openable: false,
                    restricted: Some(acp_core::model::SessionFileRestrictedReason::Unreadable),
                });
            }
        };
        let relative = match canonical.strip_prefix(&self.canonical) {
            Ok(relative) => relative,
            Err(_) => {
                return Ok(ChildInspection {
                    is_link: true,
                    target_is_dir: metadata.is_dir(),
                    openable: false,
                    restricted: Some(acp_core::model::SessionFileRestrictedReason::OutsideRoot),
                });
            }
        };
        if relative.components().any(|component| {
            component
                .as_os_str()
                .to_str()
                .is_some_and(is_git_management_name)
        }) {
            return Ok(ChildInspection {
                is_link: true,
                target_is_dir: metadata.is_dir(),
                openable: false,
                restricted: Some(acp_core::model::SessionFileRestrictedReason::Unreadable),
            });
        }
        if metadata.is_dir() && self.reject_cycle(rel, &canonical).is_err() {
            return Ok(ChildInspection {
                is_link: true,
                target_is_dir: true,
                openable: false,
                restricted: Some(acp_core::model::SessionFileRestrictedReason::Unreadable),
            });
        }
        Ok(ChildInspection {
            is_link: true,
            target_is_dir: metadata.is_dir(),
            openable: true,
            restricted: None,
        })
    }
}

/// 列表扫描时一个子项的边界判定。
#[derive(Debug, Clone, Copy)]
pub(crate) struct ChildInspection {
    pub(crate) is_link: bool,
    pub(crate) target_is_dir: bool,
    pub(crate) openable: bool,
    pub(crate) restricted: Option<acp_core::model::SessionFileRestrictedReason>,
}

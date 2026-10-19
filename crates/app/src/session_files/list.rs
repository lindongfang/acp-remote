//! 分层枚举、确定排序、Git 忽略过滤与有界分页（design D1/D3）。
//!
//! 默认显示隐藏项、系统属性项与 Git 忽略项，但 `.git` 目录与 worktree 的 `.git` 管理文件永不出现。
//! `hideIgnored` 只在 Git 仓库内生效，且过滤发生在分页之前；非仓库与 Git 缺失是两种不同的显式
//! `GitState`，都照常列出全部文件。翻页位置只承载「上一次列表的签名 + 偏移」：目录在两次请求之间
//! 变化时不会静默拼接两个列表，而是要求客户端重试。

use std::collections::HashSet;
use std::path::Path;

use acp_core::model::{
    ConflictKind, GitState, PortError, SESSION_FILE_PAGE_LIMIT, SessionFileEntry,
    SessionFileEntryKind, SessionFilePage, SessionFilePath, SessionFileRestrictedReason,
    SessionListPath, SessionPageToken,
};
use sha2::{Digest, Sha256};

use super::git::GitRunner;
use super::root::{SecureRoot, is_git_management_name, unavailable};

/// 扫描得到的一条目录项（尚未按 Git 忽略规则过滤）。
pub(crate) struct ScannedEntry {
    pub(crate) rel: SessionFilePath,
    pub(crate) name: String,
    pub(crate) kind: SessionFileEntryKind,
    pub(crate) is_directory: bool,
    pub(crate) openable: bool,
    pub(crate) restricted: Option<SessionFileRestrictedReason>,
}

/// 单次目录扫描的条目上限：排序需要看到全部条目，超过上限明确失败而不是悄悄截断。
pub(crate) const MAX_SCAN_ENTRIES: usize = 20_000;

/// 扫描一个目录的直接子项，按「目录优先、名称码点、路径打破并列」排序。
///
/// 无法安全表达的条目（非 UTF-8 名称、控制字符、超长路径）被跳过并计入返回值；不猜一个近似名字，
/// 也不把它们伪装成可打开条目。
pub(crate) fn scan(
    root: &str,
    path: Option<&SessionFilePath>,
) -> Result<Vec<ScannedEntry>, PortError> {
    let secure = SecureRoot::open(root)?;
    let resolved = secure.resolve_dir(path)?;
    let entries = resolved.open_dir()?;
    let parent = path.map(SessionFilePath::as_str);
    let mut scanned = Vec::new();
    let mut skipped = 0usize;
    for entry in entries {
        if scanned.len() >= MAX_SCAN_ENTRIES {
            return Err(PortError::InvalidRequest(acp_core::model::RESULT_TOO_LARGE));
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            skipped += 1;
            continue;
        };
        if name.is_empty() || name.chars().count() > 256 {
            // `SessionFileEntry` 的名称上限是 256 字符；超出时跳过而不是让整页失败。
            skipped += 1;
            continue;
        }
        if is_git_management_name(&name) {
            continue;
        }
        let rel_text = match parent {
            Some(parent) => format!("{parent}/{name}"),
            None => name.clone(),
        };
        let Ok(rel) = SessionFilePath::parse(&rel_text) else {
            // 控制字符、冒号、超长路径等无法在 wire 上无歧义表达：明确降级（跳过并计数）。
            skipped += 1;
            continue;
        };
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        if !file_type.is_symlink() && secure.child_of(&resolved, &rel).is_none() {
            // 枚举期间目录对象可能被替换成根外 junction 再换回：无法证明子项属于已校验目录，
            // 不把根外名称当成本目录条目返回。
            skipped += 1;
            continue;
        }
        let inspection = if file_type.is_symlink() {
            match secure.inspect_child(&rel) {
                Ok(inspection) => inspection,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            }
        } else if file_type.is_dir() {
            super::root::ChildInspection {
                is_link: false,
                target_is_dir: true,
                openable: true,
                restricted: None,
            }
        } else if file_type.is_file() {
            super::root::ChildInspection {
                is_link: false,
                target_is_dir: false,
                openable: true,
                restricted: None,
            }
        } else {
            super::root::ChildInspection {
                is_link: false,
                target_is_dir: false,
                openable: false,
                restricted: Some(SessionFileRestrictedReason::Unsupported),
            }
        };
        let kind = if inspection.is_link {
            SessionFileEntryKind::Link
        } else if inspection.target_is_dir {
            SessionFileEntryKind::Directory
        } else if file_type.is_file() {
            SessionFileEntryKind::File
        } else {
            SessionFileEntryKind::Other
        };
        scanned.push(ScannedEntry {
            rel,
            name,
            kind,
            is_directory: inspection.target_is_dir,
            openable: inspection.openable,
            restricted: inspection.restricted,
        });
    }
    if skipped > 0 {
        // 只记录数量，不记录名称/路径（`SECURITY_DESIGN.md` §14）。
        tracing::warn!(
            event = "session_files.entries_skipped",
            skipped,
            "部分目录项无法在协议中无歧义表达，已跳过"
        );
    }
    scaled_sort(&mut scanned);
    Ok(scanned)
}

fn scaled_sort(scanned: &mut [ScannedEntry]) {
    scanned.sort_by(|left, right| {
        right
            .is_directory
            .cmp(&left.is_directory)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.rel.as_str().cmp(right.rel.as_str()))
    });
}

/// 列表页的签名：目录内容、过滤开关与路径任一变化都会改变签名。
pub(crate) fn list_signature(
    path: Option<&SessionFilePath>,
    hide_ignored: bool,
    entries: &[ScannedEntry],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"session-files-list\0");
    hasher.update(path.map_or("", SessionFilePath::as_str).as_bytes());
    hasher.update([0, u8::from(hide_ignored)]);
    for entry in entries {
        hasher.update(entry.name.as_bytes());
        hasher.update([0]);
        hasher.update(entry.rel.as_str().as_bytes());
        hasher.update([0]);
        hasher.update([kind_tag(entry.kind)]);
        hasher.update([u8::from(entry.is_directory)]);
        hasher.update([u8::from(entry.openable)]);
        hasher.update([entry.restricted.map_or(0, restricted_tag)]);
    }
    hex_prefix(&hasher.finalize())
}

fn kind_tag(kind: SessionFileEntryKind) -> u8 {
    match kind {
        SessionFileEntryKind::File => 1,
        SessionFileEntryKind::Directory => 2,
        SessionFileEntryKind::Link => 3,
        SessionFileEntryKind::Other => 4,
    }
}

fn restricted_tag(reason: SessionFileRestrictedReason) -> u8 {
    match reason {
        SessionFileRestrictedReason::OutsideRoot => 1,
        SessionFileRestrictedReason::Unreadable => 2,
        SessionFileRestrictedReason::Unsupported => 3,
    }
}

pub(crate) fn hex_prefix(digest: &[u8]) -> String {
    let mut text = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        use std::fmt::Write;
        let _ = write!(text, "{byte:02x}");
    }
    text
}

/// 编码翻页位置：`<签名16 hex>-<偏移十进制>`，只含 `SessionPageToken` 允许的字符。
pub(crate) fn encode_token(signature: &str, offset: usize) -> Result<SessionPageToken, PortError> {
    SessionPageToken::parse(&format!("{signature}-{offset}")).map_err(|_| unavailable())
}

/// 解码并校验翻页位置。签名不匹配说明两次请求之间列表变了：明确要求重试，不拼接旧偏移。
pub(crate) fn decode_token(token: &SessionPageToken, signature: &str) -> Result<usize, PortError> {
    let (found, offset) = token
        .as_str()
        .split_once('-')
        .ok_or_else(|| PortError::InvalidRequest("session file page token is invalid"))?;
    if found != signature {
        return Err(PortError::Conflict(ConflictKind::VersionMismatch));
    }
    offset
        .parse::<usize>()
        .map_err(|_| PortError::InvalidRequest("session file page token is invalid"))
}

/// 把一个已过滤、已排序的扫描列表切成有界的一页。
pub(crate) fn page(
    path: SessionListPath,
    git_state: GitState,
    entries: &[ScannedEntry],
    signature: &str,
    offset: usize,
) -> Result<SessionFilePage, PortError> {
    let start = offset.min(entries.len());
    let end = (start + SESSION_FILE_PAGE_LIMIT).min(entries.len());
    let mut page_entries = Vec::with_capacity(end - start);
    for entry in &entries[start..end] {
        page_entries.push(
            SessionFileEntry::try_new(
                entry.rel.clone(),
                &entry.name,
                entry.kind,
                entry.openable,
                entry.restricted,
            )
            .map_err(|_| unavailable())?,
        );
    }
    let next = if end < entries.len() {
        Some(encode_token(signature, end)?)
    } else {
        None
    };
    SessionFilePage::try_new(path, git_state, page_entries, next).map_err(|_| unavailable())
}

/// 在分页之前按 Git 有效忽略规则过滤。返回被过滤后的条目；过滤失败时调用方必须显式降级，
/// 不能声称「已过滤」却返回全部文件。
pub(crate) fn apply_ignored(
    entries: Vec<ScannedEntry>,
    ignored: &HashSet<String>,
) -> Vec<ScannedEntry> {
    entries
        .into_iter()
        .filter(|entry| !ignored.contains(&entry.rel.as_str().replace('\\', "/")))
        .collect()
}

/// 列出目录时的 Git 可用性：只探测，不改变仓库。
pub(crate) async fn git_state(git: &GitRunner, root: &str) -> GitState {
    let root = Path::new(root);
    git.state(root).await
}

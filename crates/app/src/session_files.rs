//! 会话文件与 Git 的只读原生适配器（`acp_core::ports::SessionFiles` 的实现）。
//!
//! 唯一权威：`openspec/changes/session-file-browser/design.md` D2–D5 与
//! `docs/SECURITY_DESIGN.md` §12.3。本模块只做 I/O：文件句柄、目录枚举、有界内存快照、受管理的
//! `git` 子进程与取消；授权、会话选择、来源判定和限额语义由 `core::use_cases` 拥有。
//!
//! 边界纪律：
//!
//! - **根边界按实际对象校验**（[`root`]）：规范化路径 + 打开句柄身份比较，拒绝根外链接、循环链接、
//!   `.git` 管理路径与 Windows 设备保留名；无法证明时不猜测放行。
//! - **只读**：枚举不改目录，Git 查询不写索引/工作树（`--no-optional-locks`），不调用 checkout/clean/
//!   add/reset；正文快照只在内存，空闲 60 秒（可配置）与显式释放都会清空，不落临时正文文件。
//! - **有界**：每页 128 项、单块 32 KiB、文本预览 256 KiB、图片预览 4 MiB、下载 64 MiB、Git 差异
//!   400 行/64 KiB/单行 4096 字符，stdout/stderr 与快照总量都有上限。
//! - **降级显式**：非仓库返回 `GitState::NotRepository`，Git 缺失返回 `Unavailable`，两者都照常列出
//!   文件；不会用空列表冒充干净仓库或已过滤。
//!
//! 不记录文件正文、版本令牌、Git stderr 原文或主机绝对路径。

mod content;
mod git;
mod list;
mod root;

#[cfg(test)]
mod tests;

use std::ffi::OsString;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use acp_core::model::{
    ContentSnapshot, ContentVersion, FileReadPurpose, GitFileStatus, GitState, GitStatusEntry,
    GitStatusPage, PortError, SessionFileChunk, SessionFilePage, SessionFilePath, SessionListPath,
    SessionPageToken,
};
use acp_core::ports::SessionFiles;
use async_trait::async_trait;
use sha2::{Digest, Sha256};

use content::SnapshotStore;
use git::{GitRunner, StatusView};
use root::{SecureRoot, unavailable};

/// 原生适配器的可调参数。生产默认值来自 design D5 与 `SYNC_PROTOCOL.md` §14。
#[derive(Debug, Clone)]
pub struct SessionFilesConfig {
    /// 本机 Git 可执行文件名或路径（默认 `git`）。缺失时 Git 视图显式不可用。
    pub git_program: OsString,
    /// 单次 Git 调用超时（默认 10 秒）。
    pub git_timeout: Duration,
    /// 内容快照空闲 TTL（默认 60 秒）。
    pub idle_ttl: Duration,
    /// 测试用：完全替代 Git 参数（验证超时/取消后的进程树清理）。
    pub(crate) test_git_args: Option<Vec<OsString>>,
}

impl Default for SessionFilesConfig {
    fn default() -> Self {
        Self {
            git_program: OsString::from("git"),
            git_timeout: git::GIT_TIMEOUT,
            idle_ttl: content::DEFAULT_IDLE_TTL,
            test_git_args: None,
        }
    }
}

/// `SessionFiles` 的原生实现。由组合根注入 `UseCases::with_session_files`。
pub struct NativeSessionFiles {
    git: GitRunner,
    snapshots: Arc<SnapshotStore>,
}

impl std::fmt::Debug for NativeSessionFiles {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeSessionFiles")
            .finish_non_exhaustive()
    }
}

impl Default for NativeSessionFiles {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeSessionFiles {
    pub fn new() -> Self {
        Self::with_config(SessionFilesConfig::default())
    }

    pub fn with_config(config: SessionFilesConfig) -> Self {
        Self {
            git: GitRunner::new(config.git_program, config.git_timeout, config.test_git_args),
            snapshots: Arc::new(SnapshotStore::new(config.idle_ttl)),
        }
    }

    /// 释放全部内容快照（Daemon 关闭路径）。连接断开/取消由接线层对每个版本调用
    /// `release_content`，与空闲到期走同一入口。
    pub fn release_all(&self) {
        self.snapshots.release_all();
    }

    #[cfg(test)]
    pub(crate) fn snapshot_count(&self) -> usize {
        self.snapshots.count()
    }

    #[cfg(test)]
    pub(crate) fn snapshot_bytes(&self) -> usize {
        self.snapshots.total_bytes()
    }
}

#[async_trait]
impl SessionFiles for NativeSessionFiles {
    async fn list(
        &self,
        root: &str,
        path: Option<&SessionFilePath>,
        hide_ignored: bool,
        after: Option<&SessionPageToken>,
    ) -> Result<SessionFilePage, PortError> {
        let root_text = root.to_owned();
        let path_owned = path.cloned();
        let entries = {
            let root_text = root_text.clone();
            let path_owned = path_owned.clone();
            tokio::task::spawn_blocking(move || list::scan(&root_text, path_owned.as_ref()))
                .await
                .map_err(|_| unavailable())??
        };

        let mut git_state = list::git_state(&self.git, &root_text).await;
        let mut entries = entries;
        if hide_ignored && git_state == GitState::Available {
            let paths: Vec<String> = entries
                .iter()
                .map(|entry| entry.rel.as_str().to_owned())
                .collect();
            match self.git.check_ignored(Path::new(&root_text), &paths).await {
                Ok(ignored) => entries = list::apply_ignored(entries, &ignored),
                // 过滤不可用时不假装「已过滤」：保留全部文件并显式报告 Git 不可用。
                Err(_) => git_state = GitState::Unavailable,
            }
        }

        let signature = list::list_signature(path_owned.as_ref(), hide_ignored, &entries);
        let offset = match after {
            Some(token) => list::decode_token(token, &signature)?,
            None => 0,
        };
        let list_path =
            SessionListPath::parse(path_owned.as_ref().map_or("", SessionFilePath::as_str))
                .map_err(|_| unavailable())?;
        list::page(list_path, git_state, &entries, &signature, offset)
    }

    async fn open_content(
        &self,
        root: &str,
        path: &SessionFilePath,
        purpose: FileReadPurpose,
    ) -> Result<ContentSnapshot, PortError> {
        let store = Arc::clone(&self.snapshots);
        let root = root.to_owned();
        let path = path.clone();
        tokio::task::spawn_blocking(move || store.open(&root, &path, purpose))
            .await
            .map_err(|_| unavailable())?
    }

    async fn read_content(
        &self,
        version: &ContentVersion,
        offset: u64,
    ) -> Result<SessionFileChunk, PortError> {
        let store = Arc::clone(&self.snapshots);
        let version = version.clone();
        let chunk = tokio::task::spawn_blocking(move || store.read(&version, offset))
            .await
            .map_err(|_| unavailable())??;
        chunk.into_chunk()
    }

    async fn release_content(&self, version: &ContentVersion) -> Result<(), PortError> {
        self.snapshots.release(version);
        Ok(())
    }

    async fn git_status(
        &self,
        root: &str,
        after: Option<&SessionPageToken>,
    ) -> Result<GitStatusPage, PortError> {
        let root_text = root.to_owned();
        {
            let root_text = root_text.clone();
            tokio::task::spawn_blocking(move || SecureRoot::open(&root_text).map(|_| ()))
                .await
                .map_err(|_| unavailable())??;
        }
        let view = self.git.status(Path::new(&root_text)).await?;
        let entries = match view {
            StatusView::NotRepository => {
                return GitStatusPage::try_new(GitState::NotRepository, Vec::new(), None)
                    .map_err(|_| unavailable());
            }
            StatusView::Unavailable => {
                return GitStatusPage::try_new(GitState::Unavailable, Vec::new(), None)
                    .map_err(|_| unavailable());
            }
            StatusView::Entries(entries) => entries,
        };
        // Git 的 `-z` 列表可能穿过根内 junction/symlink 列出根外文件；任何下发路径都必须先按
        // 实际对象确认仍在会话根内。删除的文件不存在，按删除语义保留（diff 会返回删除行）。
        let root_for_filter = root_text.clone();
        let entries = tokio::task::spawn_blocking(move || {
            let secure = SecureRoot::open(&root_for_filter)?;
            Ok::<_, PortError>(
                entries
                    .into_iter()
                    .filter(|entry| {
                        if entry.status == GitFileStatus::Deleted {
                            return true;
                        }
                        secure.resolve(Some(&entry.path)).is_ok()
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .await
        .map_err(|_| unavailable())??;
        let signature = status_signature(&entries);
        let offset = match after {
            Some(token) => list::decode_token(token, &signature)?,
            None => 0,
        };
        let start = offset.min(entries.len());
        let end = (start + acp_core::model::SESSION_FILE_PAGE_LIMIT).min(entries.len());
        let selected: Vec<git::GitEntry> = entries[start..end].to_vec();
        let next = if end < entries.len() {
            Some(list::encode_token(&signature, end)?)
        } else {
            None
        };
        let stats = self.git.line_counts(Path::new(&root_text), &selected).await;
        let page_entries = {
            let root_text = root_text.clone();
            tokio::task::spawn_blocking(move || -> Result<Vec<GitStatusEntry>, PortError> {
                let secure = SecureRoot::open(&root_text)?;
                let mut read_budget = 8 * 1024 * 1024;
                selected
                    .into_iter()
                    .map(|entry| {
                        let binary = content::looks_binary(&secure, &entry.path);
                        let counts = if binary || entry.conflicted {
                            None
                        } else if let Some((has_head, counts)) = &stats {
                            if !has_head
                                || entry.status == acp_core::model::GitFileStatus::Untracked
                            {
                                count_added_lines(&secure, &entry.path, &mut read_budget)
                            } else {
                                counts.get(entry.path.as_str()).copied()
                            }
                        } else {
                            None
                        };
                        git::status_entry(entry, binary).map(|entry| entry.with_line_counts(counts))
                    })
                    .collect()
            })
            .await
            .map_err(|_| unavailable())??
        };
        GitStatusPage::try_new(GitState::Available, page_entries, next).map_err(|_| unavailable())
    }

    async fn git_diff(
        &self,
        root: &str,
        path: &SessionFilePath,
    ) -> Result<acp_core::model::GitDiff, PortError> {
        let root_text = root.to_owned();
        let path_owned = path.clone();
        {
            let root_text = root_text.clone();
            let path_owned = path_owned.clone();
            tokio::task::spawn_blocking(move || ensure_safe_diff_path(&root_text, &path_owned))
                .await
                .map_err(|_| unavailable())??;
        }
        self.git.diff(Path::new(&root_text), path).await
    }
}

/// 差异请求的路径必须指向根内对象；文件已删除时允许继续（只会显示删除），但链接指向根外时拒绝，
/// 避免 Git 顺着链接读到根外内容。
fn ensure_safe_diff_path(root: &str, path: &SessionFilePath) -> Result<(), PortError> {
    root::validate_relative(path)?;
    let secure = SecureRoot::open(root)?;
    let joined = secure.join(path);
    match std::fs::symlink_metadata(&joined) {
        Ok(_) => {
            let resolved = secure.resolve(Some(path))?;
            // 只接受普通文件：FIFO/设备/套接字/目录在打开时不阻塞、也不产生差异。
            if !resolved.metadata().is_file() {
                return Err(unavailable());
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(unavailable()),
    }
}

/// Git status 分页签名：完整列表（路径、状态、旧路径、冲突）参与哈希。
fn status_signature(entries: &[git::GitEntry]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"session-git-status\0");
    for entry in entries {
        hasher.update(entry.path.as_str().as_bytes());
        hasher.update([0]);
        hasher.update(entry.status.as_str().as_bytes());
        hasher.update([0]);
        hasher.update(
            entry
                .previous
                .as_ref()
                .map_or("", SessionFilePath::as_str)
                .as_bytes(),
        );
        hasher.update([0, u8::from(entry.conflicted)]);
    }
    list::hex_prefix(&hasher.finalize())
}

/// Bound empty-base reads across the page; never cache file content for statistics.
fn count_added_lines(
    secure: &SecureRoot,
    path: &SessionFilePath,
    budget: &mut usize,
) -> Option<(u32, u32)> {
    let resolved = secure.resolve(Some(path)).ok()?;
    let size = usize::try_from(resolved.metadata().len()).ok()?;
    if size > *budget {
        return None;
    }
    let bytes = resolved.read_head(size.checked_add(1)?).ok()?;
    *budget = budget.saturating_sub(bytes.len());
    if bytes.len() != size || bytes.contains(&0) || std::str::from_utf8(&bytes).is_err() {
        return None;
    }
    let lines = bytes.iter().filter(|byte| **byte == b'\n').count()
        + usize::from(!bytes.is_empty() && !bytes.ends_with(b"\n"));
    Some((u32::try_from(lines).ok()?, 0))
}

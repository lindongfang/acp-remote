//! 会话文件与 Git 只读视图（`docs/CORE_PORTS_AND_STORAGE.md` §3.8）。
//!
//! 这一组值对象只承载「当前会话工作目录内的只读文件事实」：根内相对路径、一次枚举页、一个字节
//! 块与内容版本、Git 状态页与差异视图，以及读取用途。它们不包含绝对路径、文件句柄、进程、wire
//! DTO 或任何 JSON 容器：绝对根只作为 [`crate::ports::SessionFiles`] 的入参由用例层从持久化的
//! `workspace_cwd` 取得，**从不**进入这些值对象，也不随结果下发。
//!
//! 路径形状在构造期判定（`SessionFilePath::parse`）：拒绝绝对路径、盘符、UNC、父引用、NUL、Windows
//! alternate data stream 冒号与 `.git` 管理路径。长度上限 4096 是 §14 的固定 v1 常量。适配器在真正
//! 打开对象时仍必须以实际句柄验证根边界（D3）；本层只拒绝「不可能是根内相对路径」的输入，不能替代
//! 适配器的安全打开。

use std::fmt;
use std::str::FromStr;

use super::error::InvalidValue;

/// 枚举页、Git 状态页的条目数上限（`SYNC_PROTOCOL.md` §14 的固定 v1 常量：128）。
pub const SESSION_FILE_PAGE_LIMIT: usize = 128;
/// 根内相对路径的字符数上限（`SYNC_PROTOCOL.md` §14 的固定 v1 常量：4096）。
pub const SESSION_FILE_PATH_MAX_CHARS: usize = 4096;
/// 单个内容块的原始字节上限（32 KiB；base64 后仍远小于 1 MiB 消息预算）。
pub const SESSION_FILE_CHUNK_MAX_BYTES: usize = 32 * 1024;
/// 文本预览的原始字节上限（256 KiB）。
pub const SESSION_FILE_TEXT_PREVIEW_MAX_BYTES: u64 = 256 * 1024;
/// 栅格图片预览的原始字节上限（4 MiB）。
pub const SESSION_FILE_IMAGE_PREVIEW_MAX_BYTES: u64 = 4 * 1024 * 1024;
/// 一次完整下载的原始字节上限（64 MiB）。
pub const SESSION_FILE_DOWNLOAD_MAX_BYTES: u64 = 64 * 1024 * 1024;
/// 下载超出 [`SESSION_FILE_DOWNLOAD_MAX_BYTES`] 时端口返回的**可识别**失败文本。
///
/// 与 wire 错误码 `resource.result_too_large` 逐字相同（`docs/SYNC_PROTOCOL.md` §11.5）。适配器 MUST
/// 返回 `PortError::InvalidRequest(RESULT_TOO_LARGE)`，服务端据此映射为同一个对端可见错误码；落进
/// 通用 `InvalidRequest` 通配臂会变成 `protocol.schema_invalid`，把「文件太大」误报成「请求形状错误」。
pub const RESULT_TOO_LARGE: &str = "resource.result_too_large";
/// Git 文本差异的行数上限（400 行）。
pub const GIT_DIFF_MAX_LINES: usize = 400;
/// Git 文本差异的合计 UTF-8 字节上限（64 KiB）。
pub const GIT_DIFF_MAX_BYTES: usize = 64 * 1024;
/// Git 文本差异单行的字符数上限（4096）。
pub const GIT_DIFF_LINE_MAX_CHARS: usize = 4096;
/// 翻页位置标识的字符数上限。
pub const SESSION_PAGE_TOKEN_MAX_CHARS: usize = 256;
/// 连接期内容版本的字符数上限。
pub const CONTENT_VERSION_MAX_CHARS: usize = 128;

newtype!(
    /// 根内相对路径（`SYNC_PROTOCOL.md` §11.5）。空串表示根目录，只有目录枚举使用它；文件读取与
    /// Git 差异一律要求非空路径，因此本类型拒绝空串，根由 [`SessionListPath`] 显式表达。
    ///
    /// 拒绝：绝对路径（前导 `/` 或 `\`）、盘符/UNC、父引用（`..`）与当前目录引用（`.`）、NUL、
    /// 任意 `:`（Windows alternate data stream 与盘符都经过它）、空路径分量（重复/尾随分隔符）以及
    /// 任一分量为 `.git`（大小写不敏感，隐藏 Git 管理目录与 worktree 的 `.git` 管理文件）。
    SessionFilePath,
    check_session_relative_path
);

/// 目录枚举的路径：`None` = 会话根，`Some` = 根内相对目录。`text()` 返回 wire 上的原文（根为空串）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionListPath(Option<SessionFilePath>);

impl SessionListPath {
    /// 会话根（wire 上的空字符串）。
    pub fn root() -> Self {
        Self(None)
    }

    /// 解析：空串是根，其余必须满足 [`SessionFilePath`] 的形状。
    pub fn parse(text: &str) -> Result<Self, InvalidValue> {
        if text.is_empty() {
            Ok(Self::root())
        } else {
            SessionFilePath::parse(text).map(|path| Self(Some(path)))
        }
    }

    /// 根内相对目录；根返回 `None`。
    pub fn file(&self) -> Option<&SessionFilePath> {
        self.0.as_ref()
    }

    /// wire 上的原文：根为空串。
    pub fn as_wire(&self) -> &str {
        self.0.as_ref().map_or("", SessionFilePath::as_str)
    }
}

fn check_session_relative_path(text: &str) -> Result<(), InvalidValue> {
    if text.is_empty() {
        return Err(InvalidValue::Empty);
    }
    if text.chars().count() > SESSION_FILE_PATH_MAX_CHARS {
        return Err(InvalidValue::TooLong {
            max: SESSION_FILE_PATH_MAX_CHARS,
        });
    }
    let bytes = text.as_bytes();
    // 绝对路径与 UNC：前导分隔符一律拒绝（Windows 的 `\\server\share` 与 Unix 的 `/etc`）。
    if matches!(bytes.first(), Some(&b'/') | Some(&b'\\')) {
        return Err(InvalidValue::Field);
    }
    // 盘符与 alternate data stream 都走冒号；一次性全部拒绝，不按平台猜测。
    if text.contains(':') {
        return Err(InvalidValue::Field);
    }
    if text.chars().any(|c| c == '\0' || c.is_control()) {
        return Err(InvalidValue::Field);
    }
    let mut components = 0usize;
    for component in text.split(['/', '\\']) {
        // 空分量 = 重复、尾随或混合分隔符：不是唯一的根内相对表示。
        if component.is_empty() {
            return Err(InvalidValue::Field);
        }
        if component == "." || component == ".." {
            return Err(InvalidValue::Field);
        }
        if component.eq_ignore_ascii_case(".git") {
            return Err(InvalidValue::Field);
        }
        components += 1;
    }
    if components == 0 {
        return Err(InvalidValue::Field);
    }
    Ok(())
}

newtype!(
    /// 翻页位置标识（`after`/`next`）。它只是不透明的短期位置，不授予任何读取权限；字符集与长度
    /// 有界，避免把任意客户端文本原样回显。
    SessionPageToken,
    check_page_token
);

fn check_page_token(text: &str) -> Result<(), InvalidValue> {
    if text.is_empty() {
        return Err(InvalidValue::Empty);
    }
    if text.chars().count() > SESSION_PAGE_TOKEN_MAX_CHARS {
        return Err(InvalidValue::TooLong {
            max: SESSION_PAGE_TOKEN_MAX_CHARS,
        });
    }
    if !text
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(InvalidValue::Field);
    }
    Ok(())
}

newtype!(
    /// 连接期内容版本（`SYNC_PROTOCOL.md` §11.5 的不透明标识）。客户端不解析其结构；值只在建立它的
    /// 连接/设备/会话/路径组合内有效，不得当作跨连接的 bearer 凭据。
    ContentVersion,
    check_content_version
);

fn check_content_version(text: &str) -> Result<(), InvalidValue> {
    if text.is_empty() {
        return Err(InvalidValue::Empty);
    }
    if text.chars().count() > CONTENT_VERSION_MAX_CHARS {
        return Err(InvalidValue::TooLong {
            max: CONTENT_VERSION_MAX_CHARS,
        });
    }
    if !text
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(InvalidValue::Field);
    }
    Ok(())
}

token_enum!(
    /// 文件枚举条目的类型（`session.files.list` 的 `entries[].kind`）。
    SessionFileEntryKind {
        File => "file",
        Directory => "directory",
        Link => "link",
        Other => "other",
    }
);

token_enum!(
    /// 条目不可打开的原因（`entries[].restricted`）。可打开条目不带该字段。
    SessionFileRestrictedReason {
        OutsideRoot => "outside_root",
        Unreadable => "unreadable",
        Unsupported => "unsupported",
    }
);

token_enum!(
    /// 目录的 Git 状态（`gitState`）：已可查询、不是仓库、本机 Git 不可用。非仓库与 Git 缺失是两种
    /// 不同的成功状态，都不得用空列表冒充干净仓库。
    GitState {
        Available => "available",
        NotRepository => "not_repository",
        Unavailable => "unavailable",
    }
);

token_enum!(
    /// Git status 中一个文件的合并状态（相对 HEAD 与工作树）。
    GitFileStatus {
        Added => "added",
        Modified => "modified",
        Deleted => "deleted",
        Renamed => "renamed",
        Untracked => "untracked",
    }
);

token_enum!(
    /// Git 差异的比较基准：HEAD 与当前工作树，或空基准（仓库尚无提交）。
    GitDiffBase {
        Head => "head",
        Empty => "empty",
    }
);

token_enum!(
    /// Git 差异的内容类别：文本、二进制、冲突。二进制与冲突不得伪造成文本差异。
    GitDiffKind {
        Text => "text",
        Binary => "binary",
        Conflict => "conflict",
    }
);

token_enum!(
    /// Git 差异不可用的显式原因（成功结果里的状态，不是协议错误）。
    GitDiffUnavailableReason {
        NotRepository => "not_repository",
        GitUnavailable => "git_unavailable",
        TooLarge => "too_large",
    }
);

token_enum!(
    /// 读取结果的字节内容类别：文本、栅格图片、二进制。客户端据此选择预览方式。
    SessionFileContentKind {
        Text => "text",
        Image => "image",
        Binary => "binary",
    }
);

token_enum!(
    /// 读取用途：预览或用户主动下载。预览只取该用途的预算，下载受完整字节预算约束。
    FileReadPurpose {
        Preview => "preview",
        Download => "download",
    }
);

/// 文件枚举页的一条条目。
///
/// `openable` 与 `restricted` 是同一事实的两面：可打开 ⇔ 没有受限原因。根外链接仍列出名称与根内
/// 路径，但 `openable = false`，且不下发目标绝对路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFileEntry {
    path: SessionFilePath,
    name: String,
    kind: SessionFileEntryKind,
    openable: bool,
    restricted: Option<SessionFileRestrictedReason>,
}

impl SessionFileEntry {
    pub fn try_new(
        path: SessionFilePath,
        name: &str,
        kind: SessionFileEntryKind,
        openable: bool,
        restricted: Option<SessionFileRestrictedReason>,
    ) -> Result<Self, InvalidValue> {
        let name_length = name.chars().count();
        if name_length == 0 || name_length > 256 || name.contains('\0') {
            return Err(InvalidValue::Field);
        }
        if openable == restricted.is_some() {
            return Err(InvalidValue::Field);
        }
        Ok(Self {
            path,
            name: name.to_owned(),
            kind,
            openable,
            restricted,
        })
    }

    pub fn path(&self) -> &SessionFilePath {
        &self.path
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> SessionFileEntryKind {
        self.kind
    }

    pub fn openable(&self) -> bool {
        self.openable
    }

    pub fn restricted(&self) -> Option<SessionFileRestrictedReason> {
        self.restricted
    }
}

/// `session.files.list` 的完成页：回显的路径、目录 Git 状态、有界条目与可选的续读位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFilePage {
    path: SessionListPath,
    git_state: GitState,
    entries: Vec<SessionFileEntry>,
    next: Option<SessionPageToken>,
}

impl SessionFilePage {
    pub fn try_new(
        path: SessionListPath,
        git_state: GitState,
        entries: Vec<SessionFileEntry>,
        next: Option<SessionPageToken>,
    ) -> Result<Self, InvalidValue> {
        if entries.len() > SESSION_FILE_PAGE_LIMIT {
            return Err(InvalidValue::TooLarge {
                max: SESSION_FILE_PAGE_LIMIT,
            });
        }
        Ok(Self {
            path,
            git_state,
            entries,
            next,
        })
    }

    pub fn path(&self) -> &SessionListPath {
        &self.path
    }

    pub fn git_state(&self) -> GitState {
        self.git_state
    }

    pub fn entries(&self) -> &[SessionFileEntry] {
        &self.entries
    }

    pub fn next(&self) -> Option<&SessionPageToken> {
        self.next.as_ref()
    }
}

/// 一次内容快照的元数据（`SessionFiles::open_content` 的返回值）。
///
/// `preview_truncated` 只对 `purpose = preview` 有意义：为真表示预览预算之外的字节没有被取得；原始
/// 大小仍由 `size_bytes` 如实给出。**预览不设原始大小上限**——预览只取该用途的预算（文本 256 KiB /
/// 图片 4 MiB）并置截断标记，因此比下载预算更大的文件仍可预览。`purpose = download` 才要求
/// `size_bytes` 不超过 64 MiB，超过即构造失败。适配器 MUST 把这次构造失败翻译成
/// `PortError::InvalidRequest(RESULT_TOO_LARGE)`（而不是让通用 `InvalidValue::TooLarge` 落进
/// `protocol.schema_invalid`），不截断后冒充完整内容。快照本身只在内存中、绑定连接与空闲 TTL，
/// 不落临时正文文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentSnapshot {
    path: SessionFilePath,
    kind: SessionFileContentKind,
    purpose: FileReadPurpose,
    size_bytes: u64,
    version: ContentVersion,
    preview_truncated: bool,
}

impl ContentSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        path: SessionFilePath,
        kind: SessionFileContentKind,
        purpose: FileReadPurpose,
        size_bytes: u64,
        version: ContentVersion,
        preview_truncated: bool,
    ) -> Result<Self, InvalidValue> {
        if purpose == FileReadPurpose::Download && size_bytes > SESSION_FILE_DOWNLOAD_MAX_BYTES {
            return Err(InvalidValue::TooLarge {
                max: SESSION_FILE_DOWNLOAD_MAX_BYTES as usize,
            });
        }
        Ok(Self {
            path,
            kind,
            purpose,
            size_bytes,
            version,
            preview_truncated,
        })
    }

    pub fn path(&self) -> &SessionFilePath {
        &self.path
    }

    pub fn kind(&self) -> SessionFileContentKind {
        self.kind
    }

    pub fn purpose(&self) -> FileReadPurpose {
        self.purpose
    }

    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    pub fn version(&self) -> &ContentVersion {
        &self.version
    }

    pub fn preview_truncated(&self) -> bool {
        self.preview_truncated
    }
}

/// 一个内容块（`session.files.read` 的完成结果）。
///
/// 数据块的原始字节数与偏移由构造期校验：单块不超过 32 KiB，且 `offset + data.len()` 不超过声明的
/// 文件大小。版本必须与建立快照时的版本一致（适配器判定），core 不在块里重新解释它。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFileChunk {
    path: SessionFilePath,
    kind: SessionFileContentKind,
    size_bytes: u64,
    version: ContentVersion,
    offset: u64,
    data: Vec<u8>,
    eof: bool,
    preview_truncated: bool,
}

impl SessionFileChunk {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        path: SessionFilePath,
        kind: SessionFileContentKind,
        size_bytes: u64,
        version: ContentVersion,
        offset: u64,
        data: Vec<u8>,
        eof: bool,
        preview_truncated: bool,
    ) -> Result<Self, InvalidValue> {
        if data.len() > SESSION_FILE_CHUNK_MAX_BYTES {
            return Err(InvalidValue::TooLarge {
                max: SESSION_FILE_CHUNK_MAX_BYTES,
            });
        }
        let end = offset
            .checked_add(data.len() as u64)
            .ok_or(InvalidValue::Field)?;
        if end > size_bytes {
            return Err(InvalidValue::Field);
        }
        Ok(Self {
            path,
            kind,
            size_bytes,
            version,
            offset,
            data,
            eof,
            preview_truncated,
        })
    }

    pub fn path(&self) -> &SessionFilePath {
        &self.path
    }

    pub fn kind(&self) -> SessionFileContentKind {
        self.kind
    }

    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    pub fn version(&self) -> &ContentVersion {
        &self.version
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn eof(&self) -> bool {
        self.eof
    }

    pub fn preview_truncated(&self) -> bool {
        self.preview_truncated
    }
}

/// Git status 的一个文件条目（相对 HEAD 与工作树合并后只出现一次）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatusEntry {
    path: SessionFilePath,
    status: GitFileStatus,
    previous_path: Option<SessionFilePath>,
    conflicted: bool,
    binary: bool,
    line_counts: Option<(u32, u32)>,
}

impl GitStatusEntry {
    pub fn try_new(
        path: SessionFilePath,
        status: GitFileStatus,
        previous_path: Option<SessionFilePath>,
        conflicted: bool,
        binary: bool,
    ) -> Result<Self, InvalidValue> {
        // 旧路径只在改名且能安全确认时出现；其它状态携带旧路径会让客户端把它当成另一个文件。
        if previous_path.is_some() && status != GitFileStatus::Renamed {
            return Err(InvalidValue::Field);
        }
        Ok(Self {
            path,
            status,
            previous_path,
            conflicted,
            binary,
            line_counts: None,
        })
    }

    /// Counts are available only for complete, non-conflicting text comparisons.
    pub fn with_line_counts(mut self, counts: Option<(u32, u32)>) -> Self {
        self.line_counts = if self.binary || self.conflicted {
            None
        } else {
            counts
        };
        self
    }

    pub fn line_counts(&self) -> Option<(u32, u32)> {
        self.line_counts
    }

    pub fn path(&self) -> &SessionFilePath {
        &self.path
    }

    pub fn status(&self) -> GitFileStatus {
        self.status
    }

    pub fn previous_path(&self) -> Option<&SessionFilePath> {
        self.previous_path.as_ref()
    }

    pub fn conflicted(&self) -> bool {
        self.conflicted
    }

    pub fn binary(&self) -> bool {
        self.binary
    }
}

/// `session.git.status` 的完成页：Git 状态、有界条目与可选的续读位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatusPage {
    git_state: GitState,
    entries: Vec<GitStatusEntry>,
    next: Option<SessionPageToken>,
}

impl GitStatusPage {
    pub fn try_new(
        git_state: GitState,
        entries: Vec<GitStatusEntry>,
        next: Option<SessionPageToken>,
    ) -> Result<Self, InvalidValue> {
        if entries.len() > SESSION_FILE_PAGE_LIMIT {
            return Err(InvalidValue::TooLarge {
                max: SESSION_FILE_PAGE_LIMIT,
            });
        }
        Ok(Self {
            git_state,
            entries,
            next,
        })
    }

    pub fn git_state(&self) -> GitState {
        self.git_state
    }

    pub fn entries(&self) -> &[GitStatusEntry] {
        &self.entries
    }

    pub fn next(&self) -> Option<&SessionPageToken> {
        self.next.as_ref()
    }
}

/// `session.git.diff` 的完成结果：路径、基准、内容类别与有界文本差异。
///
/// `lines` 与 `unavailable` 互斥：不可用时不带任何文本行，也不下发改写后的片段。`truncated` 表示文本
/// 差异超过 400 行 / 64 KiB / 单行 4096 字符的上限，客户端据此明确提示；整体仍按编码后消息预算检查。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitDiff {
    path: SessionFilePath,
    base: GitDiffBase,
    kind: GitDiffKind,
    lines: Vec<String>,
    unavailable: Option<GitDiffUnavailableReason>,
    truncated: bool,
    comparison: Option<(String, String)>,
}

impl GitDiff {
    pub fn try_new(
        path: SessionFilePath,
        base: GitDiffBase,
        kind: GitDiffKind,
        lines: Vec<String>,
        unavailable: Option<GitDiffUnavailableReason>,
        truncated: bool,
    ) -> Result<Self, InvalidValue> {
        if unavailable.is_some() && !lines.is_empty() {
            return Err(InvalidValue::Field);
        }
        if lines.len() > GIT_DIFF_MAX_LINES {
            return Err(InvalidValue::TooLarge {
                max: GIT_DIFF_MAX_LINES,
            });
        }
        let mut total = 0usize;
        for line in &lines {
            if line.chars().count() > GIT_DIFF_LINE_MAX_CHARS {
                return Err(InvalidValue::TooLong {
                    max: GIT_DIFF_LINE_MAX_CHARS,
                });
            }
            total = total.saturating_add(line.len());
        }
        if total > GIT_DIFF_MAX_BYTES {
            return Err(InvalidValue::TooLarge {
                max: GIT_DIFF_MAX_BYTES,
            });
        }
        Ok(Self {
            path,
            base,
            kind,
            lines,
            unavailable,
            truncated,
            comparison: None,
        })
    }

    pub fn with_comparison(mut self, before: String, after: String) -> Self {
        if self.kind == GitDiffKind::Text
            && self.unavailable.is_none()
            && before.len() <= GIT_DIFF_MAX_BYTES
            && after.len() <= GIT_DIFF_MAX_BYTES
        {
            self.comparison = Some((before, after));
        }
        self
    }

    pub fn comparison(&self) -> Option<(&str, &str)> {
        self.comparison
            .as_ref()
            .map(|(before, after)| (before.as_str(), after.as_str()))
    }

    pub fn path(&self) -> &SessionFilePath {
        &self.path
    }

    pub fn base(&self) -> GitDiffBase {
        self.base
    }

    pub fn kind(&self) -> GitDiffKind {
        self.kind
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn unavailable(&self) -> Option<GitDiffUnavailableReason> {
        self.unavailable
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }
}

impl fmt::Display for SessionListPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_wire())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_file_path_rejects_unsafe_shapes() {
        for accepted in [
            "src/main.rs",
            "nested/dir/file.txt",
            "deep\\windows\\style.txt",
            "a..b",
            ".gitignore",
        ] {
            assert!(
                SessionFilePath::parse(accepted).is_ok(),
                "根内相对路径必须接受：{accepted}"
            );
        }
        for rejected in [
            "",
            "/etc/passwd",
            "\\\\server\\share\\file",
            "C:\\repo\\file",
            "C:file",
            "../outside",
            "dir/../outside",
            "./file",
            "dir//file",
            "dir/file/",
            "dir\\file.txt:stream",
            "dir/\0file",
            ".git/config",
            "dir/.git/HEAD",
            "DIR/.GIT/HEAD",
        ] {
            assert!(
                SessionFilePath::parse(rejected).is_err(),
                "不安全的相对路径必须拒绝：{rejected:?}"
            );
        }
        assert!(
            SessionFilePath::parse(&"a".repeat(SESSION_FILE_PATH_MAX_CHARS)).is_ok(),
            "4096 字符是允许的上界"
        );
        assert!(SessionFilePath::parse(&"a".repeat(SESSION_FILE_PATH_MAX_CHARS + 1)).is_err());
    }

    #[test]
    fn list_path_represents_the_root_as_an_empty_string() {
        let root = SessionListPath::parse("").expect("空串是会话根");
        assert!(root.file().is_none());
        assert_eq!(root.as_wire(), "");
        let nested = SessionListPath::parse("src").expect("根内目录");
        assert_eq!(nested.file().map(SessionFilePath::as_str), Some("src"));
        assert_eq!(nested.as_wire(), "src");
    }

    #[test]
    fn content_chunk_and_pages_are_bounded() {
        let path = SessionFilePath::parse("src/main.rs").expect("路径");
        let version = ContentVersion::parse("v1-abc").expect("版本");
        assert!(
            SessionFileChunk::try_new(
                path.clone(),
                SessionFileContentKind::Text,
                10,
                version.clone(),
                0,
                vec![0_u8; 10],
                true,
                false,
            )
            .is_ok()
        );
        assert!(
            SessionFileChunk::try_new(
                path.clone(),
                SessionFileContentKind::Text,
                10,
                version.clone(),
                0,
                vec![0_u8; SESSION_FILE_CHUNK_MAX_BYTES + 1],
                false,
                false,
            )
            .is_err(),
            "单块不得超过 32 KiB"
        );
        assert!(
            SessionFileChunk::try_new(
                path.clone(),
                SessionFileContentKind::Text,
                10,
                version,
                5,
                vec![0_u8; 6],
                true,
                false,
            )
            .is_err(),
            "offset + data 不得超过文件大小"
        );

        let entry = SessionFileEntry::try_new(
            path.clone(),
            "main.rs",
            SessionFileEntryKind::File,
            true,
            None,
        )
        .expect("可打开条目");
        let too_many = vec![entry; SESSION_FILE_PAGE_LIMIT + 1];
        assert!(
            SessionFilePage::try_new(SessionListPath::root(), GitState::Available, too_many, None,)
                .is_err(),
            "枚举页不得超过 128 项"
        );
    }

    #[test]
    fn preview_snapshots_are_not_bounded_by_the_download_budget() {
        let path = SessionFilePath::parse("media/clip.bin").expect("路径");
        let version = ContentVersion::parse("v1-abc").expect("版本");
        let beyond = SESSION_FILE_DOWNLOAD_MAX_BYTES + 1;

        let preview = ContentSnapshot::try_new(
            path.clone(),
            SessionFileContentKind::Binary,
            FileReadPurpose::Preview,
            beyond,
            version.clone(),
            true,
        )
        .expect("预览不受下载预算约束：超过 64 MiB 只截断");
        assert_eq!(preview.size_bytes(), beyond, "原始大小如实给出");
        assert!(preview.preview_truncated());
        assert_eq!(preview.purpose(), FileReadPurpose::Preview);

        assert!(
            ContentSnapshot::try_new(
                path.clone(),
                SessionFileContentKind::Binary,
                FileReadPurpose::Download,
                beyond,
                version.clone(),
                false,
            )
            .is_err(),
            "下载超过 64 MiB 必须明确失败，不得截断冒充完整"
        );
        assert!(
            ContentSnapshot::try_new(
                path,
                SessionFileContentKind::Binary,
                FileReadPurpose::Download,
                SESSION_FILE_DOWNLOAD_MAX_BYTES,
                version,
                false,
            )
            .is_ok(),
            "64 MiB 是允许的下载上界"
        );
    }

    #[test]
    fn entries_and_diffs_enforce_cross_field_invariants() {
        let path = SessionFilePath::parse("src/main.rs").expect("路径");
        assert!(
            SessionFileEntry::try_new(
                path.clone(),
                "main.rs",
                SessionFileEntryKind::File,
                true,
                Some(SessionFileRestrictedReason::Unreadable),
            )
            .is_err(),
            "可打开条目不得带受限原因"
        );
        assert!(
            SessionFileEntry::try_new(
                path.clone(),
                "main.rs",
                SessionFileEntryKind::File,
                false,
                None,
            )
            .is_err(),
            "不可打开条目必须说明原因"
        );
        assert!(
            GitStatusEntry::try_new(
                path.clone(),
                GitFileStatus::Modified,
                Some(SessionFilePath::parse("src/old.rs").expect("旧路径")),
                false,
                false,
            )
            .is_err(),
            "非改名条目不得携带旧路径"
        );
        assert!(
            GitDiff::try_new(
                path.clone(),
                GitDiffBase::Head,
                GitDiffKind::Text,
                vec!["+line".to_owned()],
                Some(GitDiffUnavailableReason::TooLarge),
                false,
            )
            .is_err(),
            "不可用时不得同时下发差异行"
        );
        assert!(
            GitDiff::try_new(
                path,
                GitDiffBase::Empty,
                GitDiffKind::Binary,
                Vec::new(),
                None,
                false,
            )
            .is_ok()
        );
    }
}

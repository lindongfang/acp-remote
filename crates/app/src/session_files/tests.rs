//! `session_files` 适配器测试：临时目录/仓库、根边界、内容快照、Git 只读查询与进程树清理。
//!
//! Windows 链接用例使用目录 junction（无需管理员权限，真实 reparse point）；Unix 用例使用 symlink、
//! FIFO 与 Unix socket。超时/取消用例用受控的假 Git 程序验证 `ProcessTree` 对所有平台真实生效。

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use acp_core::model::{
    FileReadPurpose, GitDiffBase, GitDiffKind, GitDiffUnavailableReason, GitFileStatus, GitState,
    PortError, RESULT_TOO_LARGE, SESSION_FILE_CHUNK_MAX_BYTES, SESSION_FILE_DOWNLOAD_MAX_BYTES,
    SESSION_FILE_TEXT_PREVIEW_MAX_BYTES, SessionFileContentKind, SessionFileEntryKind,
    SessionFilePath, SessionFileRestrictedReason, SessionListPath,
};
use acp_core::ports::SessionFiles;

use super::content;
use super::root::{SecureRoot, validate_relative};
use super::{NativeSessionFiles, SessionFilesConfig};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "acpr-session-files-{label}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&path).expect("创建临时目录");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn root(&self) -> String {
        self.0
            .canonicalize()
            .expect("规范化临时根")
            .to_str()
            .expect("临时路径是 UTF-8")
            .to_owned()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write(dir: &Path, rel: &str, bytes: &[u8]) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("创建父目录");
    }
    fs::write(path, bytes).expect("写文件");
}

fn file_path(text: &str) -> SessionFilePath {
    SessionFilePath::parse(text).expect("合法根内路径")
}

fn adapter() -> NativeSessionFiles {
    NativeSessionFiles::new()
}

fn adapter_with(config: SessionFilesConfig) -> NativeSessionFiles {
    NativeSessionFiles::with_config(config)
}

async fn list_page(
    adapter: &NativeSessionFiles,
    root: &str,
    path: Option<&str>,
    hide: bool,
) -> acp_core::model::SessionFilePage {
    let owned = path.map(file_path);
    adapter
        .list(root, owned.as_ref(), hide, None)
        .await
        .expect("枚举成功")
}

fn entry_names(page: &acp_core::model::SessionFilePage) -> Vec<String> {
    page.entries()
        .iter()
        .map(|entry| entry.name().to_owned())
        .collect()
}

fn entry<'a>(
    page: &'a acp_core::model::SessionFilePage,
    name: &str,
) -> &'a acp_core::model::SessionFileEntry {
    page.entries()
        .iter()
        .find(|entry| entry.name() == name)
        .unwrap_or_else(|| panic!("缺少条目 {name}：{:?}", entry_names(page)))
}

async fn read_all(
    adapter: &NativeSessionFiles,
    snapshot: &acp_core::model::ContentSnapshot,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut offset = 0u64;
    loop {
        let chunk = adapter
            .read_content(snapshot.version(), offset)
            .await
            .expect("读取块");
        bytes.extend_from_slice(chunk.data());
        offset += chunk.data().len() as u64;
        if chunk.eof() {
            break;
        }
        assert!(!chunk.data().is_empty(), "非 EOF 块必须带数据");
    }
    bytes
}

// ---------------------------------------------------------------------------
// Git 测试脚手架
// ---------------------------------------------------------------------------

fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("启动本机 git")
}

fn git_ok(dir: &Path, args: &[&str]) {
    let output = git(dir, args);
    assert!(
        output.status.success(),
        "git {args:?} 失败：{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Git 在配置值里按反斜杠转义；给 filter 命令与脚本内嵌路径统一用正斜杠，
/// 这样同一份 `.sh` 脚本在 Unix 与 Git for Windows 上都能直接执行。
fn posix_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// 写一个会被 Git 当作 clean filter 执行的脚本，返回可直接写进配置的命令。
fn filter_marker_script(dir: &Path, marker: &Path) -> String {
    let scripts = dir.join("scripts");
    fs::create_dir_all(&scripts).expect("scripts");
    let script = scripts.join("filter.sh");
    fs::write(
        &script,
        format!("#!/bin/sh\necho ran >> \"{}\"\ncat\n", posix_path(marker)),
    )
    .expect("script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    posix_path(&script)
}

fn commit(dir: &Path, message: &str) {
    git_ok(dir, &["add", "-A"]);
    git_ok(
        dir,
        &[
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "user.name=Test",
            "commit",
            "-q",
            "-m",
            message,
        ],
    );
}

fn init_repo(dir: &Path) {
    git_ok(dir, &["init", "-q", "-b", "main", "."]);
    // 合并需要提交身份，而 CI 没有全局 `user.*`；把身份写进仓库本地配置，
    // 让测试不依赖宿主机 Git 配置。
    git_ok(dir, &["config", "user.email", "test@example.invalid"]);
    git_ok(dir, &["config", "user.name", "Test"]);
}

// ---------------------------------------------------------------------------
// 2.1 枚举、过滤、分页与特殊名称
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_shows_hidden_and_ignored_by_default_but_never_git() {
    let temp = TempDir::new("list-default");
    init_repo(temp.path());
    write(temp.path(), ".gitignore", b"ignored.txt\n");
    write(temp.path(), ".hidden", b"hidden");
    write(temp.path(), "normal.txt", b"normal");
    write(temp.path(), "ignored.txt", b"ignored");
    write(temp.path(), ".git/config", b"must-not-be-listed");
    #[cfg(windows)]
    {
        let system = temp.path().join("system.txt");
        write(temp.path(), "system.txt", b"system");
        let _ = std::process::Command::new("attrib")
            .args(["+s", "+h"])
            .arg(&system)
            .status();
    }

    let adapter = adapter();
    let root = temp.root();
    let page = list_page(&adapter, &root, None, false).await;
    assert_eq!(page.git_state(), GitState::Available);
    let names = entry_names(&page);
    assert!(names.contains(&".gitignore".to_owned()));
    assert!(names.contains(&".hidden".to_owned()));
    assert!(names.contains(&"ignored.txt".to_owned()));
    assert!(names.contains(&"normal.txt".to_owned()));
    #[cfg(windows)]
    assert!(
        names.contains(&"system.txt".to_owned()),
        "系统属性文件默认可见"
    );
    assert!(!names.contains(&".git".to_owned()), ".git 永不可见");

    let filtered = list_page(&adapter, &root, None, true).await;
    assert_eq!(filtered.git_state(), GitState::Available);
    let names = entry_names(&filtered);
    assert!(!names.contains(&"ignored.txt".to_owned()), "忽略项被过滤");
    assert!(
        names.contains(&".hidden".to_owned()),
        "隐藏项不受忽略开关影响"
    );
    assert!(!names.contains(&".git".to_owned()));

    // 直接读取 `.git` 管理路径在 core 的类型层就不可能构造；适配器也会复核。
    assert!(SessionFilePath::parse(".git/config").is_err());
    assert!(SessionFilePath::parse(".GIT/HEAD").is_err());
}

#[tokio::test]
async fn worktree_git_management_file_is_hidden() {
    let temp = TempDir::new("git-file");
    write(temp.path(), ".git", b"gitdir: /somewhere/else\n");
    write(temp.path(), "tracked.txt", b"data");
    let adapter = adapter();
    let root = temp.root();
    let page = list_page(&adapter, &root, None, false).await;
    assert_eq!(page.git_state(), GitState::NotRepository);
    let names = entry_names(&page);
    assert!(!names.contains(&".git".to_owned()));
    assert!(names.contains(&"tracked.txt".to_owned()));
}

#[tokio::test]
async fn uppercase_git_directory_is_hidden() {
    let temp = TempDir::new("upper-git");
    write(temp.path(), ".GIT/config", b"must-not-be-listed");
    write(temp.path(), "visible.txt", b"x");
    let adapter = adapter();
    let root = temp.root();
    let page = list_page(&adapter, &root, None, false).await;
    let names = entry_names(&page);
    assert!(!names.contains(&".GIT".to_owned()) && !names.contains(&".git".to_owned()));
    assert!(names.contains(&"visible.txt".to_owned()));
}

#[tokio::test]
async fn non_repository_lists_all_files_and_reports_not_repository() {
    let temp = TempDir::new("non-repo");
    write(temp.path(), "ignored.txt", b"x");
    write(temp.path(), ".gitignore", b"ignored.txt\n");
    let adapter = adapter();
    let root = temp.root();
    let page = list_page(&adapter, &root, None, true).await;
    assert_eq!(page.git_state(), GitState::NotRepository);
    assert!(entry_names(&page).contains(&"ignored.txt".to_owned()));
}

#[tokio::test]
async fn missing_git_degrades_to_unavailable_but_still_lists_files() {
    let temp = TempDir::new("no-git");
    write(temp.path(), "a.txt", b"a");
    let adapter = adapter_with(SessionFilesConfig {
        git_program: OsString::from("acpr-remote-git-does-not-exist"),
        ..SessionFilesConfig::default()
    });
    let root = temp.root();
    let page = list_page(&adapter, &root, None, true).await;
    assert_eq!(page.git_state(), GitState::Unavailable);
    assert_eq!(entry_names(&page), vec!["a.txt".to_owned()]);
}

#[tokio::test]
async fn pagination_filters_before_paging_and_sorts_deterministically() {
    let temp = TempDir::new("paging");
    init_repo(temp.path());
    fs::create_dir_all(temp.path().join("zdir")).expect("zdir");
    fs::create_dir_all(temp.path().join("adir")).expect("adir");
    write(temp.path(), "a.txt", b"a");
    write(temp.path(), "b.txt", b"b");
    write(temp.path(), "z-ignored.txt", b"z");
    write(temp.path(), ".gitignore", b"z-ignored.txt\n");
    for index in 0..130 {
        write(temp.path(), &format!("f{index:03}.txt"), b"x");
    }

    let adapter = adapter();
    let root = temp.root();
    let first = list_page(&adapter, &root, None, true).await;
    assert_eq!(first.entries().len(), 128);
    let next = first.next().cloned().expect("还有下一页");
    assert_eq!(
        entry_names(&first)[..2].to_vec(),
        vec!["adir".to_owned(), "zdir".to_owned()]
    );
    assert!(!entry_names(&first).contains(&"z-ignored.txt".to_owned()));
    let second = adapter
        .list(&root, None, true, Some(&next))
        .await
        .expect("第二页");
    assert!(second.next().is_none());
    let mut all = entry_names(&first);
    all.extend(entry_names(&second));
    assert_eq!(
        all.len(),
        2 + 2 + 1 + 130,
        "两个目录 + 两个文件 + .gitignore + 130 项"
    );
    assert!(!all.contains(&"z-ignored.txt".to_owned()));
    let mut unique = all.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), all.len(), "分页不重复");
}

#[tokio::test]
async fn page_token_detects_a_changed_listing() {
    let temp = TempDir::new("token");
    for index in 0..130 {
        write(temp.path(), &format!("f{index:03}.txt"), b"x");
    }
    let adapter = adapter();
    let root = temp.root();
    let first = list_page(&adapter, &root, None, false).await;
    let next = first.next().cloned().expect("下一页");
    write(temp.path(), "f000-new.txt", b"x");
    let error = adapter
        .list(&root, None, false, Some(&next))
        .await
        .expect_err("列表变化必须要求重试");
    assert!(matches!(
        error,
        PortError::Conflict(acp_core::model::ConflictKind::VersionMismatch)
    ));
}

#[test]
fn list_signature_includes_directory_target() {
    let make = |is_directory: bool| super::list::ScannedEntry {
        rel: file_path("link"),
        name: "link".to_owned(),
        kind: SessionFileEntryKind::Link,
        is_directory,
        openable: true,
        restricted: None,
    };
    let as_directory = super::list::list_signature(None, false, &[make(true)]);
    let as_file = super::list::list_signature(None, false, &[make(false)]);
    assert_ne!(
        as_directory, as_file,
        "链接目标从目录变为文件必须改变分页签名，否则第二页会重复/漏项"
    );
}

#[tokio::test]
async fn special_names_are_listed_verbatim() {
    let temp = TempDir::new("names");
    let long_name = format!("long-{}.txt", "x".repeat(80));
    write(temp.path(), "with space.txt", b"x");
    write(temp.path(), "unicode-文件.txt", b"x");
    write(temp.path(), &long_name, b"x");
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::OsStr::from_bytes(b"control-\x01.txt");
        fs::write(temp.path().join(name), b"x").expect("创建控制字符文件名");
    }
    let adapter = adapter();
    let root = temp.root();
    let page = list_page(&adapter, &root, None, false).await;
    let names = entry_names(&page);
    assert!(names.contains(&"with space.txt".to_owned()));
    assert!(names.contains(&"unicode-文件.txt".to_owned()));
    assert!(names.contains(&long_name));
    #[cfg(unix)]
    assert!(
        !names.iter().any(|name| name.starts_with("control-")),
        "无法无歧义表达的名称必须跳过"
    );
    assert!(SessionFilePath::parse("bad\u{1}.txt").is_err());
}

// ---------------------------------------------------------------------------
// 2.2 根边界、链接、循环与特殊文件
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[tokio::test]
async fn unix_links_cannot_escape_root_and_cycles_are_rejected() {
    use std::os::unix::fs::symlink;

    let outside = TempDir::new("unix-outside");
    write(outside.path(), "secret.txt", b"secret");
    let temp = TempDir::new("unix-links");
    fs::create_dir_all(temp.path().join("inside_dir")).expect("inside dir");
    write(temp.path(), "inside_dir/file.txt", b"inside");
    write(temp.path(), "plain.txt", b"plain");
    symlink(
        outside.path().join("secret.txt"),
        temp.path().join("file_link"),
    )
    .expect("symlink");
    symlink(outside.path(), temp.path().join("dir_link")).expect("symlink");
    symlink(temp.path(), temp.path().join("loop")).expect("symlink loop");
    symlink("inside_dir", temp.path().join("internal_link")).expect("symlink");
    symlink("inside_dir/file.txt", temp.path().join("internal_file")).expect("symlink");

    let adapter = adapter();
    let root = temp.root();
    let page = list_page(&adapter, &root, None, false).await;
    let file_link = entry(&page, "file_link");
    assert_eq!(file_link.kind(), SessionFileEntryKind::Link);
    assert!(!file_link.openable());
    assert_eq!(
        file_link.restricted(),
        Some(SessionFileRestrictedReason::OutsideRoot)
    );
    let dir_link = entry(&page, "dir_link");
    assert!(!dir_link.openable());
    assert_eq!(
        dir_link.restricted(),
        Some(SessionFileRestrictedReason::OutsideRoot)
    );
    let loop_entry = entry(&page, "loop");
    assert!(!loop_entry.openable(), "循环链接不可打开");
    let internal = entry(&page, "internal_link");
    assert!(internal.openable(), "根内目录链接可浏览");

    for path in ["file_link", "dir_link", "loop"] {
        let error = adapter
            .open_content(&root, &file_path(path), FileReadPurpose::Preview)
            .await
            .expect_err("根外/循环链接不可读取");
        assert!(matches!(error, PortError::Unavailable(_)));
    }
    let inside_page = adapter
        .list(&root, Some(&file_path("internal_link")), false, None)
        .await
        .expect("根内链接可逐层浏览");
    assert_eq!(entry_names(&inside_page), vec!["file.txt".to_owned()]);
    let snapshot = adapter
        .open_content(
            &root,
            &file_path("internal_file"),
            FileReadPurpose::Download,
        )
        .await
        .expect("根内文件链接可读");
    assert_eq!(read_all(&adapter, &snapshot).await, b"inside");

    // 循环链接：直接请求也不会无限深入。
    let error = adapter
        .list(&root, Some(&file_path("loop")), false, None)
        .await
        .expect_err("循环目录链接不可枚举");
    assert!(matches!(error, PortError::Unavailable(_)));

    // 命名管道与 Unix socket：明确不可打开，不阻塞。
    let fifo = temp.path().join("fifo");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo");
    assert!(status.success());
    let socket_path = temp.path().join("sock");
    let _listener = std::os::unix::net::UnixListener::bind(&socket_path).expect("unix socket");
    let page = list_page(&adapter, &root, None, false).await;
    for name in ["fifo", "sock"] {
        let item = entry(&page, name);
        assert!(!item.openable(), "{name} 不可打开");
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            adapter.open_content(&root, &file_path(name), FileReadPurpose::Preview),
        )
        .await
        .expect("不得挂起");
        assert!(result.is_err());
    }
}

#[cfg(unix)]
#[tokio::test]
async fn git_diff_rejects_fifo_without_blocking() {
    let temp = TempDir::new("diff-fifo");
    let fifo = temp.path().join("pipe");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo");
    assert!(status.success());
    let adapter = adapter();
    let root = temp.root();
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        adapter.git_diff(&root, &file_path("pipe")),
    )
    .await
    .expect("FIFO 差异请求不得阻塞到 Git 超时");
    assert!(result.is_err(), "FIFO 必须立即报告不可用");
}

#[cfg(windows)]
fn junction(link: &Path, target: &Path) {
    let status = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .status()
        .expect("启动 cmd");
    assert!(status.success(), "创建 junction 失败：{}", link.display());
}

#[cfg(windows)]
#[tokio::test]
async fn windows_junctions_cannot_escape_root() {
    let outside = TempDir::new("win-outside");
    write(outside.path(), "secret.txt", b"secret");
    let temp = TempDir::new("win-junction");
    fs::create_dir_all(temp.path().join("inside_dir")).expect("inside dir");
    write(temp.path(), "inside_dir/file.txt", b"inside");
    junction(&temp.path().join("junction_out"), outside.path());
    junction(
        &temp.path().join("junction_in"),
        &temp.path().join("inside_dir"),
    );
    junction(&temp.path().join("loop"), temp.path());

    let adapter = adapter();
    let root = temp.root();
    let page = list_page(&adapter, &root, None, false).await;
    let outside_entry = entry(&page, "junction_out");
    assert_eq!(outside_entry.kind(), SessionFileEntryKind::Link);
    assert!(!outside_entry.openable());
    assert_eq!(
        outside_entry.restricted(),
        Some(SessionFileRestrictedReason::OutsideRoot)
    );
    let inside_entry = entry(&page, "junction_in");
    assert!(inside_entry.openable(), "根内 junction 可浏览");
    assert!(
        !entry(&page, "loop").openable(),
        "指回根的 junction 不可打开"
    );

    let error = adapter
        .open_content(
            &root,
            &file_path("junction_out/secret.txt"),
            FileReadPurpose::Preview,
        )
        .await
        .expect_err("根外 junction 下的文件不可读");
    assert!(matches!(error, PortError::Unavailable(_)));
    let error = adapter
        .list(&root, Some(&file_path("junction_out")), false, None)
        .await
        .expect_err("根外 junction 不可枚举");
    assert!(matches!(error, PortError::Unavailable(_)));
    let error = adapter
        .list(&root, Some(&file_path("loop")), false, None)
        .await
        .expect_err("循环 junction 不可枚举");
    assert!(matches!(error, PortError::Unavailable(_)));

    let inside_page = adapter
        .list(&root, Some(&file_path("junction_in")), false, None)
        .await
        .expect("根内 junction 可浏览");
    assert_eq!(entry_names(&inside_page), vec!["file.txt".to_owned()]);

    // 文件 symlink 在未启用开发者模式的主机上需要管理员权限；能创建时也必须被同样拒绝。
    if std::os::windows::fs::symlink_file(
        outside.path().join("secret.txt"),
        temp.path().join("file_link"),
    )
    .is_ok()
    {
        let page = list_page(&adapter, &root, None, false).await;
        let link = entry(&page, "file_link");
        assert!(!link.openable());
        assert_eq!(
            link.restricted(),
            Some(SessionFileRestrictedReason::OutsideRoot)
        );
        assert!(
            adapter
                .open_content(&root, &file_path("file_link"), FileReadPurpose::Preview)
                .await
                .is_err()
        );
    }
}

#[cfg(windows)]
#[tokio::test]
async fn junction_replacement_cannot_smuggle_children() {
    let outside = TempDir::new("smuggle-outside");
    write(outside.path(), "secret.txt", b"secret");
    let temp = TempDir::new("smuggle");
    fs::create_dir_all(temp.path().join("dir")).expect("dir");
    write(temp.path(), "dir/inside.txt", b"inside");

    let root = temp.root();
    let secure = SecureRoot::open(&root).expect("根");
    let dir = secure
        .resolve_dir(Some(&file_path("dir")))
        .expect("已校验目录");
    assert!(
        secure
            .child_of(&dir, &file_path("dir/inside.txt"))
            .is_some(),
        "真实子项应绑定到已校验目录"
    );

    // 枚举打开后把目录替换成根外 junction：根外子项名称不得被当作目录内条目。
    fs::remove_dir_all(temp.path().join("dir")).expect("删除目录");
    junction(&temp.path().join("dir"), outside.path());
    assert!(
        secure
            .child_of(&dir, &file_path("dir/secret.txt"))
            .is_none(),
        "根外 junction 的子项必须被拒绝"
    );

    // 换回根内真实目录后，只有确实存在于该目录的名称才能通过。
    fs::remove_dir(temp.path().join("dir")).expect("删除 junction");
    fs::create_dir_all(temp.path().join("dir")).expect("重建目录");
    write(temp.path(), "dir/secret.txt", b"local");
    assert!(
        secure
            .child_of(&dir, &file_path("dir/secret.txt"))
            .is_some(),
        "根内真实子项仍应被接受"
    );
}

#[tokio::test]
async fn replace_after_validation_is_rejected() {
    let outside = TempDir::new("race-outside");
    write(outside.path(), "secret.txt", b"secret");
    let temp = TempDir::new("race");
    write(temp.path(), "target.txt", b"original");

    let root = temp.root();
    let secure = SecureRoot::open(&root).expect("根");
    let path = file_path("target.txt");
    let resolved = secure.resolve(Some(&path)).expect("解析目标");

    // 换成另一个普通文件：身份不同，必须拒绝。
    fs::remove_file(temp.path().join("target.txt")).expect("删除");
    write(temp.path(), "target.txt", b"replacement-content");
    assert!(
        resolved.open_file().is_err(),
        "校验后替换的普通文件必须被身份比较拒绝"
    );

    // 换成指向根外的目录链接：即使打开路径仍然存在，也必须拒绝。
    let resolved = secure.resolve(Some(&path)).expect("重新解析");
    fs::remove_file(temp.path().join("target.txt")).expect("删除");
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        outside.path().join("secret.txt"),
        temp.path().join("target.txt"),
    )
    .expect("symlink");
    #[cfg(windows)]
    junction(&temp.path().join("target.txt"), outside.path());
    assert!(
        resolved.open_file().is_err(),
        "校验后被替换为根外链接必须拒绝"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn windows_reserved_device_names_are_rejected_without_opening() {
    let temp = TempDir::new("devices");
    let adapter = adapter();
    let root = temp.root();
    for name in ["CON", "con.txt", "NUL", "sub/AUX", "COM1", "CONOUT$"] {
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            adapter.open_content(&root, &file_path(name), FileReadPurpose::Preview),
        )
        .await
        .expect("设备保留名不得挂起");
        assert!(
            matches!(result, Err(PortError::InvalidRequest(_))),
            "{name} 必须被拒绝"
        );
    }
}

#[test]
fn adapter_revalidates_relative_shapes() {
    for bad in ["", "/etc/passwd", "../x", "a//b", "a/.git/b"] {
        let parsed = SessionFilePath::parse(bad);
        if let Ok(path) = parsed {
            assert!(validate_relative(&path).is_err(), "{bad} 必须拒绝");
        }
    }
    assert!(validate_relative(&file_path("dir/file.txt")).is_ok());
}

// ---------------------------------------------------------------------------
// 2.3 内容快照
// ---------------------------------------------------------------------------

#[tokio::test]
async fn download_chunks_are_consistent_and_version_bound() {
    let temp = TempDir::new("download");
    let payload: Vec<u8> = (0..100 * 1024).map(|index| (index % 251) as u8).collect();
    write(temp.path(), "data.bin", &payload);
    let adapter = adapter();
    let root = temp.root();
    let snapshot = adapter
        .open_content(&root, &file_path("data.bin"), FileReadPurpose::Download)
        .await
        .expect("打开下载");
    assert_eq!(snapshot.kind(), SessionFileContentKind::Binary);
    assert_eq!(snapshot.size_bytes(), payload.len() as u64);
    assert_eq!(snapshot.purpose(), FileReadPurpose::Download);
    assert!(!snapshot.preview_truncated());

    let bytes = read_all(&adapter, &snapshot).await;
    assert_eq!(bytes, payload);

    // 未知版本、释放后的版本、越界 offset 都明确失败。
    let unknown =
        acp_core::model::ContentVersion::parse("00000000000000000000000000000000").expect("版本");
    assert!(adapter.read_content(&unknown, 0).await.is_err());
    assert!(
        adapter
            .read_content(snapshot.version(), payload.len() as u64 + 1)
            .await
            .is_err()
    );
    adapter
        .release_content(snapshot.version())
        .await
        .expect("释放");
    adapter
        .release_content(snapshot.version())
        .await
        .expect("释放幂等");
    assert!(adapter.read_content(snapshot.version(), 0).await.is_err());
    assert_eq!(adapter.snapshot_count(), 0);
}

#[tokio::test]
async fn changed_file_invalidates_the_snapshot() {
    let temp = TempDir::new("changed");
    write(temp.path(), "file.txt", b"first version\n");
    let adapter = adapter();
    let root = temp.root();
    let snapshot = adapter
        .open_content(&root, &file_path("file.txt"), FileReadPurpose::Download)
        .await
        .expect("打开");
    assert_eq!(read_all(&adapter, &snapshot).await, b"first version\n");

    write(
        temp.path(),
        "file.txt",
        b"second version with different length\n",
    );
    assert!(
        adapter.read_content(snapshot.version(), 0).await.is_err(),
        "同一版本不得拼接两个时刻的内容"
    );
    assert_eq!(adapter.snapshot_count(), 0, "变化后快照被废弃");

    let reopened = adapter
        .open_content(&root, &file_path("file.txt"), FileReadPurpose::Download)
        .await
        .expect("重新打开");
    assert_eq!(
        read_all(&adapter, &reopened).await,
        b"second version with different length\n"
    );
}

#[tokio::test]
async fn preview_respects_budgets_and_reports_original_size() {
    let temp = TempDir::new("preview");
    let text = "a".repeat(300 * 1024);
    write(temp.path(), "large.txt", text.as_bytes());
    let adapter = adapter();
    let root = temp.root();
    let snapshot = adapter
        .open_content(&root, &file_path("large.txt"), FileReadPurpose::Preview)
        .await
        .expect("预览");
    assert_eq!(snapshot.kind(), SessionFileContentKind::Text);
    assert_eq!(snapshot.size_bytes(), text.len() as u64);
    assert!(snapshot.preview_truncated(), "超过文本预览预算必须标记截断");
    let bytes = read_all(&adapter, &snapshot).await;
    assert_eq!(bytes.len(), SESSION_FILE_TEXT_PREVIEW_MAX_BYTES as usize);
    assert!(bytes.starts_with(b"aaa"));

    // 图片 magic 决定 Image，并按 4 MiB 预算截断。
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    png.extend(std::iter::repeat_n(0_u8, 5 * 1024 * 1024));
    write(temp.path(), "image.png", &png);
    let image = adapter
        .open_content(&root, &file_path("image.png"), FileReadPurpose::Preview)
        .await
        .expect("图片预览");
    assert_eq!(image.kind(), SessionFileContentKind::Image);
    assert!(image.preview_truncated());
    let bytes = read_all(&adapter, &image).await;
    assert!(bytes.len() as u64 <= 4 * 1024 * 1024);
    assert_eq!(bytes[..8], png[..8]);

    // 扩展名不参与判断：文本内容叫 .png 仍然是 Text。
    write(temp.path(), "fake.png", b"plain text");
    let fake = adapter
        .open_content(&root, &file_path("fake.png"), FileReadPurpose::Preview)
        .await
        .expect("按内容判定");
    assert_eq!(fake.kind(), SessionFileContentKind::Text);
}

#[tokio::test]
async fn preview_of_binary_has_no_body_and_download_limit_is_explicit() {
    let temp = TempDir::new("budgets");
    write(temp.path(), "blob.bin", b"abc\0def");
    let adapter = adapter();
    let root = temp.root();
    let preview = adapter
        .open_content(&root, &file_path("blob.bin"), FileReadPurpose::Preview)
        .await
        .expect("二进制预览");
    assert_eq!(preview.kind(), SessionFileContentKind::Binary);
    assert_eq!(preview.size_bytes(), 7);
    assert!(preview.preview_truncated());
    let chunk = adapter
        .read_content(preview.version(), 0)
        .await
        .expect("空预览块");
    assert!(chunk.data().is_empty());
    assert!(chunk.eof());

    let big = std::fs::File::create(temp.path().join("huge.bin")).expect("创建大文件");
    big.set_len(SESSION_FILE_DOWNLOAD_MAX_BYTES + 1)
        .expect("设置长度");
    let error = adapter
        .open_content(&root, &file_path("huge.bin"), FileReadPurpose::Download)
        .await
        .expect_err("超过下载预算必须失败");
    match error {
        PortError::InvalidRequest(text) => assert_eq!(text, RESULT_TOO_LARGE),
        other => panic!("错误类型不符：{other:?}"),
    }
    // 预览不受下载预算限制，只截断。
    let preview = adapter
        .open_content(&root, &file_path("huge.bin"), FileReadPurpose::Preview)
        .await
        .expect("超大文件仍可预览");
    assert_eq!(preview.size_bytes(), SESSION_FILE_DOWNLOAD_MAX_BYTES + 1);
    assert!(preview.preview_truncated());
}

#[tokio::test]
async fn snapshots_are_bounded_expire_and_leave_no_temp_files() {
    let temp = TempDir::new("ttl");
    write(temp.path(), "a.txt", b"alpha");
    write(temp.path(), "b.txt", b"beta");
    let before: Vec<_> = fs::read_dir(temp.path())
        .expect("read_dir")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    let adapter = adapter_with(SessionFilesConfig {
        idle_ttl: Duration::from_millis(60),
        ..SessionFilesConfig::default()
    });
    let root = temp.root();
    let snapshot = adapter
        .open_content(&root, &file_path("a.txt"), FileReadPurpose::Download)
        .await
        .expect("打开");
    assert!(adapter.snapshot_bytes() > 0);
    assert_eq!(
        fs::read_dir(temp.path()).expect("read_dir").count(),
        before.len(),
        "快照不写临时正文文件"
    );
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert!(adapter.read_content(snapshot.version(), 0).await.is_err());
    assert_eq!(adapter.snapshot_count(), 0, "空闲到期释放内存");

    // 总量有界：连续打开多个小文件，数量不超过上限。
    for index in 0..10 {
        write(temp.path(), &format!("file{index}.txt"), b"x");
        let _ = adapter
            .open_content(
                &root,
                &file_path(&format!("file{index}.txt")),
                FileReadPurpose::Download,
            )
            .await
            .expect("打开");
    }
    assert!(adapter.snapshot_count() <= content::MAX_SNAPSHOTS);
    adapter.release_all();
    assert_eq!(adapter.snapshot_bytes(), 0);
}

#[cfg(unix)]
#[tokio::test]
async fn unix_socket_is_not_opened_as_content() {
    let temp = TempDir::new("content-socket");
    let socket = temp.path().join("s.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&socket).expect("socket");
    let adapter = adapter();
    let root = temp.root();
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        adapter.open_content(&root, &file_path("s.sock"), FileReadPurpose::Download),
    )
    .await
    .expect("不得挂起");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// 2.4 Git status / diff / 进程树
// ---------------------------------------------------------------------------

#[tokio::test]
async fn git_status_merges_staged_and_worktree_and_respects_root() {
    let temp = TempDir::new("status");
    init_repo(temp.path());
    write(temp.path(), "staged-then-modified.txt", b"one\n");
    write(temp.path(), "deleted.txt", b"bye\n");
    write(temp.path(), "binary.dat", b"a\0b");
    commit(temp.path(), "base");

    write(temp.path(), "staged-then-modified.txt", b"staged\n");
    git_ok(temp.path(), &["add", "staged-then-modified.txt"]);
    write(temp.path(), "staged-then-modified.txt", b"worktree\n");
    write(temp.path(), "added.txt", b"new\n");
    git_ok(temp.path(), &["add", "added.txt"]);
    fs::remove_file(temp.path().join("deleted.txt")).expect("删除");
    write(temp.path(), "untracked.txt", b"untracked\n");
    write(temp.path(), "binary.dat", b"c\0d");
    git_ok(temp.path(), &["add", "binary.dat"]);

    let index_before = fs::read(temp.path().join(".git/index")).expect("index");
    let worktree_before = fs::read(temp.path().join("staged-then-modified.txt")).expect("worktree");
    let adapter = adapter();
    let root = temp.root();
    let page = adapter.git_status(&root, None).await.expect("Git 状态");
    assert_eq!(page.git_state(), GitState::Available);
    let find = |name: &str| {
        page.entries()
            .iter()
            .find(|entry| entry.path().as_str() == name)
            .unwrap_or_else(|| panic!("缺少 {name}"))
    };
    assert_eq!(
        find("staged-then-modified.txt").status(),
        GitFileStatus::Modified
    );
    assert_eq!(find("staged-then-modified.txt").line_counts(), Some((1, 1)));
    assert_eq!(find("added.txt").line_counts(), Some((1, 0)));
    assert_eq!(find("deleted.txt").line_counts(), Some((0, 1)));
    assert_eq!(find("untracked.txt").line_counts(), Some((1, 0)));
    assert_eq!(find("binary.dat").line_counts(), None);
    assert_eq!(find("added.txt").status(), GitFileStatus::Added);
    assert_eq!(find("deleted.txt").status(), GitFileStatus::Deleted);
    assert_eq!(find("untracked.txt").status(), GitFileStatus::Untracked);
    assert!(find("binary.dat").binary(), "二进制状态显式标识");
    for name in ["staged-then-modified.txt", "added.txt", "binary.dat"] {
        let only = page
            .entries()
            .iter()
            .filter(|entry| entry.path().as_str() == name)
            .count();
        assert_eq!(only, 1, "同一文件只出现一次");
    }
    let index_after = fs::read(temp.path().join(".git/index")).expect("index");
    assert_eq!(index_before, index_after, "查询不得写索引");
    assert_eq!(
        fs::read(temp.path().join("staged-then-modified.txt")).expect("worktree"),
        worktree_before,
        "查询不得改写工作树"
    );
    assert!(
        !temp.path().join("deleted.txt").exists(),
        "查询不得恢复已删除文件"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn git_status_does_not_list_outside_root_junction_children() {
    let outside = TempDir::new("status-outside");
    write(outside.path(), "secret.txt", b"secret");
    let temp = TempDir::new("status-junction");
    init_repo(temp.path());
    write(temp.path(), "tracked.txt", b"ok\n");
    commit(temp.path(), "base");
    junction(&temp.path().join("escape"), outside.path());

    let adapter = adapter();
    let root = temp.root();
    let page = adapter.git_status(&root, None).await.expect("Git 状态");
    assert_eq!(page.git_state(), GitState::Available);
    assert!(
        page.entries()
            .iter()
            .all(|entry| !entry.path().as_str().starts_with("escape/")),
        "Git status 不得穿过根内 junction 下发根外文件：{:?}",
        page.entries()
            .iter()
            .map(|entry| entry.path().as_str())
            .collect::<Vec<_>>()
    );
    let outside_path = file_path("escape/secret.txt");
    assert!(
        adapter.git_diff(&root, &outside_path).await.is_err(),
        "根外 junction 的差异请求必须被拒绝"
    );
}

#[tokio::test]
async fn git_status_handles_empty_head_conflicts_and_subdirectories() {
    // 无 HEAD：以空基准显示新增。
    let empty = TempDir::new("empty-head");
    init_repo(empty.path());
    write(empty.path(), "new.txt", b"hi\n");
    let adapter = adapter();
    let root = empty.root();
    let page = adapter.git_status(&root, None).await.expect("status");
    let entry = &page.entries()[0];
    assert_eq!(entry.path().as_str(), "new.txt");
    assert_eq!(entry.status(), GitFileStatus::Untracked);
    assert_eq!(entry.line_counts(), Some((1, 0)));
    let diff = adapter
        .git_diff(&root, &file_path("new.txt"))
        .await
        .expect("diff");
    assert_eq!(diff.base(), GitDiffBase::Empty);
    assert_eq!(diff.kind(), GitDiffKind::Text);
    assert_eq!(diff.comparison(), Some(("", "hi\n")));
    assert!(diff.lines().iter().any(|line| line == "+hi"));

    // 冲突：保留冲突标识，不伪造文本差异。
    let conflict = TempDir::new("conflict");
    init_repo(conflict.path());
    write(conflict.path(), "conflict.txt", b"base\n");
    commit(conflict.path(), "base");
    git_ok(conflict.path(), &["checkout", "-q", "-b", "other"]);
    write(conflict.path(), "conflict.txt", b"other\n");
    commit(conflict.path(), "other");
    git_ok(conflict.path(), &["checkout", "-q", "main"]);
    write(conflict.path(), "conflict.txt", b"main\n");
    commit(conflict.path(), "main");
    let merge = git(conflict.path(), &["merge", "other"]);
    assert!(!merge.status.success(), "合并必须产生冲突");
    let root = conflict.root();
    let page = adapter.git_status(&root, None).await.expect("status");
    let entry = page
        .entries()
        .iter()
        .find(|entry| entry.path().as_str() == "conflict.txt")
        .expect("冲突条目");
    assert!(entry.conflicted());
    let diff = adapter
        .git_diff(&root, &file_path("conflict.txt"))
        .await
        .expect("diff");
    assert_eq!(diff.kind(), GitDiffKind::Conflict);
    assert!(diff.lines().is_empty());

    // 子目录会话：只返回根内改动；根外 rename 表达为根内新增/删除。
    let subrepo = TempDir::new("subdir");
    init_repo(subrepo.path());
    fs::create_dir_all(subrepo.path().join("sub")).expect("sub");
    write(subrepo.path(), "outside.txt", b"outside\n");
    write(subrepo.path(), "sub/inside.txt", b"inside\n");
    commit(subrepo.path(), "base");
    write(subrepo.path(), "outside.txt", b"modified outside\n");
    write(subrepo.path(), "sub/inside.txt", b"modified inside\n");
    git_ok(subrepo.path(), &["mv", "outside.txt", "sub/moved.txt"]);
    git_ok(subrepo.path(), &["mv", "sub/inside.txt", "out-moved.txt"]);
    let root = subrepo
        .path()
        .join("sub")
        .canonicalize()
        .expect("sub root")
        .to_str()
        .expect("utf8")
        .to_owned();
    let page = adapter.git_status(&root, None).await.expect("status");
    let paths: Vec<&str> = page
        .entries()
        .iter()
        .map(|entry| entry.path().as_str())
        .collect();
    assert_eq!(paths, vec!["inside.txt", "moved.txt"]);
    let moved = page
        .entries()
        .iter()
        .find(|entry| entry.path().as_str() == "moved.txt")
        .expect("根外 → 根内的改名");
    assert_eq!(moved.status(), GitFileStatus::Added);
    assert!(moved.previous_path().is_none(), "根外旧路径不下发");
    // 根内 → 根外：作为根内删除出现，不泄漏根外新路径。
    let deleted = page
        .entries()
        .iter()
        .find(|entry| entry.path().as_str() == "inside.txt")
        .expect("根内删除");
    assert_eq!(deleted.status(), GitFileStatus::Deleted);
    assert!(deleted.previous_path().is_none());
}

#[tokio::test]
async fn git_diff_is_text_binary_and_bounded() {
    let temp = TempDir::new("diff");
    init_repo(temp.path());
    write(temp.path(), "text.txt", b"one\ntwo\n");
    write(temp.path(), "bin.dat", b"a\0b");
    write(temp.path(), "huge.txt", b"line\n");
    commit(temp.path(), "base");
    write(temp.path(), "text.txt", b"one\nchanged\n");
    write(temp.path(), "bin.dat", b"c\0d");
    write(
        temp.path(),
        "huge.txt",
        (0..500)
            .map(|index| format!("line {index}\n"))
            .collect::<String>()
            .as_bytes(),
    );
    write(
        temp.path(),
        "long-line.txt",
        format!("{}\n", "x".repeat(5000)).as_bytes(),
    );
    git_ok(temp.path(), &["add", "long-line.txt"]);
    let adapter = adapter();
    let root = temp.root();
    let diff = adapter
        .git_diff(&root, &file_path("text.txt"))
        .await
        .expect("text diff");
    assert_eq!(diff.comparison(), Some(("one\ntwo\n", "one\nchanged\n")));
    assert_eq!(diff.base(), GitDiffBase::Head);
    assert_eq!(diff.kind(), GitDiffKind::Text);
    assert!(diff.lines().iter().any(|line| line == "+changed"));

    let binary = adapter
        .git_diff(&root, &file_path("bin.dat"))
        .await
        .expect("binary diff");
    assert_eq!(binary.kind(), GitDiffKind::Binary);
    assert!(binary.lines().is_empty());

    let huge = adapter
        .git_diff(&root, &file_path("huge.txt"))
        .await
        .expect("huge diff");
    assert!(huge.lines().len() <= 400);
    assert!(huge.truncated());

    let long = adapter
        .git_diff(&root, &file_path("long-line.txt"))
        .await
        .expect("long diff");
    assert!(long.truncated());
    assert!(long.lines().iter().all(|line| line.chars().count() <= 4096));

    // 已暂存后又恢复成 HEAD：状态仍可见，但详情相对 HEAD 无净差异。
    git_ok(temp.path(), &["add", "text.txt"]);
    write(temp.path(), "text.txt", b"one\ntwo\n");
    let diff = adapter
        .git_diff(&root, &file_path("text.txt"))
        .await
        .expect("no-net diff");
    assert_eq!(diff.kind(), GitDiffKind::Text);
    assert!(diff.lines().is_empty(), "不得拼接 staged 与 worktree 补丁");
}

#[test]
fn truncated_utf8_diff_falls_back_to_character_boundary() {
    let mut bytes = b"line\n".to_vec();
    bytes.extend_from_slice("中".as_bytes());
    let cut = &bytes[..bytes.len() - 1];
    assert_eq!(
        super::git::decode_diff_output(cut, true),
        Some("line\n"),
        "截断落在多字节字符中间时应回退到字符边界"
    );
    assert_eq!(
        super::git::decode_diff_output(cut, false),
        None,
        "未截断却无法解码仍是 binary"
    );
    let mut invalid = b"line\n".to_vec();
    invalid.push(0xff);
    assert_eq!(
        super::git::decode_diff_output(&invalid, true),
        None,
        "内容本身含非法字节时仍判 binary"
    );
}

#[tokio::test]
async fn git_diff_truncation_keeps_utf8_text() {
    let temp = TempDir::new("diff-utf8");
    init_repo(temp.path());
    write(temp.path(), "multi.txt", b"base\n");
    commit(temp.path(), "base");
    let content = "中文中文中文中文中文\n".repeat(6000);
    write(temp.path(), "multi.txt", content.as_bytes());
    let adapter = adapter();
    let root = temp.root();
    let diff = adapter
        .git_diff(&root, &file_path("multi.txt"))
        .await
        .expect("utf8 diff");
    assert_eq!(
        diff.kind(),
        GitDiffKind::Text,
        "UTF-8 输出在 64 KiB 边界被截断时不得误判 binary"
    );
    assert!(diff.truncated());
    assert!(!diff.lines().is_empty());
}

#[tokio::test]
async fn git_diff_is_unavailable_for_non_repository_and_missing_git() {
    let plain = TempDir::new("plain");
    write(plain.path(), "a.txt", b"a");
    let adapter = adapter();
    let root = plain.root();
    let status = adapter.git_status(&root, None).await.expect("status");
    assert_eq!(status.git_state(), GitState::NotRepository);
    let diff = adapter
        .git_diff(&root, &file_path("a.txt"))
        .await
        .expect("diff");
    assert_eq!(
        diff.unavailable(),
        Some(GitDiffUnavailableReason::NotRepository)
    );

    let missing = adapter_with(SessionFilesConfig {
        git_program: OsString::from("acpr-remote-git-does-not-exist"),
        ..SessionFilesConfig::default()
    });
    let status = missing.git_status(&root, None).await.expect("status");
    assert_eq!(status.git_state(), GitState::Unavailable);
    let diff = missing
        .git_diff(&root, &file_path("a.txt"))
        .await
        .expect("diff");
    assert_eq!(
        diff.unavailable(),
        Some(GitDiffUnavailableReason::GitUnavailable)
    );
}

#[tokio::test]
async fn malicious_repository_configuration_is_not_executed() {
    let temp = TempDir::new("malicious");
    init_repo(temp.path());
    write(temp.path(), "tracked.txt", b"base\n");
    write(temp.path(), "tracked.dat", b"base\n");
    write(
        temp.path(),
        ".gitattributes",
        b"*.txt diff=evil\n*.dat filter=evil\n",
    );
    commit(temp.path(), "base");

    let scripts = temp.path().join("scripts");
    fs::create_dir_all(&scripts).expect("scripts");
    let marker = temp.path().join("ran.marker");
    let hook_marker = temp.path().join("hook.marker");
    #[cfg(windows)]
    {
        let script = scripts.join("evil.cmd");
        fs::write(
            &script,
            format!(
                "@echo off\r\necho ran>> \"{}\"\r\nmore\r\n",
                marker.display()
            ),
        )
        .expect("script");
        let hook = scripts.join("post-index-change.cmd");
        fs::write(
            &hook,
            format!("@echo off\r\necho ran>> \"{}\"\r\n", hook_marker.display()),
        )
        .expect("hook");
        git_ok(
            temp.path(),
            &["config", "core.hooksPath", scripts.to_str().unwrap()],
        );
        git_ok(
            temp.path(),
            &[
                "config",
                "filter.evil.clean",
                &format!("cmd /c {}", script.display()),
            ],
        );
        git_ok(
            temp.path(),
            &[
                "config",
                "filter.evil.smudge",
                &format!("cmd /c {}", script.display()),
            ],
        );
        git_ok(
            temp.path(),
            &[
                "config",
                "diff.external",
                &format!("cmd /c {}", script.display()),
            ],
        );
        git_ok(
            temp.path(),
            &[
                "config",
                "diff.evil.textconv",
                &format!("cmd /c {}", script.display()),
            ],
        );
        git_ok(
            temp.path(),
            &[
                "config",
                "core.pager",
                &format!("cmd /c {}", script.display()),
            ],
        );
        git_ok(
            temp.path(),
            &["config", "core.fsmonitor", script.to_str().unwrap()],
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script = scripts.join("evil.sh");
        fs::write(
            &script,
            format!("#!/bin/sh\necho ran >> \"{}\"\ncat\n", marker.display()),
        )
        .expect("script");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
        let hook = scripts.join("post-index-change");
        fs::write(
            &hook,
            format!("#!/bin/sh\necho ran >> \"{}\"\n", hook_marker.display()),
        )
        .expect("hook");
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).expect("chmod");
        git_ok(
            temp.path(),
            &["config", "core.hooksPath", scripts.to_str().unwrap()],
        );
        git_ok(
            temp.path(),
            &["config", "filter.evil.clean", script.to_str().unwrap()],
        );
        git_ok(
            temp.path(),
            &["config", "filter.evil.smudge", script.to_str().unwrap()],
        );
        git_ok(
            temp.path(),
            &["config", "diff.external", script.to_str().unwrap()],
        );
        git_ok(
            temp.path(),
            &["config", "diff.evil.textconv", script.to_str().unwrap()],
        );
        git_ok(
            temp.path(),
            &["config", "core.pager", script.to_str().unwrap()],
        );
        git_ok(
            temp.path(),
            &["config", "core.fsmonitor", script.to_str().unwrap()],
        );
    }

    write(temp.path(), "tracked.txt", b"changed\n");
    write(temp.path(), "tracked.dat", b"changed\n");
    let adapter = adapter();
    let root = temp.root();
    let page = adapter.git_status(&root, None).await.expect("status");
    assert_eq!(page.git_state(), GitState::Available);
    let diff = adapter
        .git_diff(&root, &file_path("tracked.txt"))
        .await
        .expect("diff");
    assert_eq!(diff.kind(), GitDiffKind::Text);
    let dat = adapter
        .git_diff(&root, &file_path("tracked.dat"))
        .await
        .expect("filtered diff");
    assert_eq!(dat.kind(), GitDiffKind::Text);
    assert!(!marker.exists(), "仓库配置的外部命令不得被执行");
    assert!(!hook_marker.exists(), "仓库 hooks 不得被执行");
}

#[tokio::test]
async fn worktree_and_included_filter_configuration_is_neutralized() {
    let temp = TempDir::new("config-layers");
    init_repo(temp.path());
    write(
        temp.path(),
        ".gitattributes",
        b"*.dat filter=evil\n*.txt filter=included\n",
    );
    write(temp.path(), "tracked.dat", b"base\n");
    write(temp.path(), "tracked.txt", b"base\n");
    commit(temp.path(), "base");

    let scripts = temp.path().join("scripts");
    fs::create_dir_all(&scripts).expect("scripts");
    let marker = temp.path().join("ran.marker");
    #[cfg(windows)]
    let command = {
        let script = scripts.join("evil.cmd");
        fs::write(
            &script,
            format!(
                "@echo off\r\necho ran>> \"{}\"\r\nmore\r\n",
                marker.display()
            ),
        )
        .expect("script");
        format!("cmd /c {}", script.display())
    };
    #[cfg(unix)]
    let command = {
        use std::os::unix::fs::PermissionsExt;
        let script = scripts.join("evil.sh");
        fs::write(
            &script,
            format!("#!/bin/sh\necho ran >> \"{}\"\ncat\n", marker.display()),
        )
        .expect("script");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
        script.display().to_string()
    };

    // include.path 引入的 filter 驱动。
    let include = temp.path().join("include.cfg");
    for attribute in ["clean", "smudge", "process"] {
        git_ok(
            temp.path(),
            &[
                "config",
                "--file",
                include.to_str().unwrap(),
                &format!("filter.included.{attribute}"),
                &command,
            ],
        );
    }
    git_ok(
        temp.path(),
        &[
            "config",
            "include.path",
            &include.to_str().unwrap().replace('\\', "/"),
        ],
    );

    // extensions.worktreeConfig 启用的 `.git/config.worktree` 中的 filter 驱动。
    git_ok(
        temp.path(),
        &["config", "extensions.worktreeConfig", "true"],
    );
    let worktree_config = temp.path().join(".git/config.worktree");
    for attribute in ["clean", "smudge", "process"] {
        git_ok(
            temp.path(),
            &[
                "config",
                "--file",
                worktree_config.to_str().unwrap(),
                &format!("filter.evil.{attribute}"),
                &command,
            ],
        );
    }

    write(temp.path(), "tracked.dat", b"changed\n");
    write(temp.path(), "tracked.txt", b"changed\n");
    let index_before = fs::read(temp.path().join(".git/index")).expect("index");
    let dat_before = fs::read(temp.path().join("tracked.dat")).expect("dat");
    let txt_before = fs::read(temp.path().join("tracked.txt")).expect("txt");

    let adapter = adapter();
    let root = temp.root();
    let page = adapter.git_status(&root, None).await.expect("status");
    assert_eq!(page.git_state(), GitState::Available);
    let dat = adapter
        .git_diff(&root, &file_path("tracked.dat"))
        .await
        .expect("worktree filter diff");
    assert_eq!(dat.kind(), GitDiffKind::Text);
    let txt = adapter
        .git_diff(&root, &file_path("tracked.txt"))
        .await
        .expect("included filter diff");
    assert_eq!(txt.kind(), GitDiffKind::Text);

    assert!(
        !marker.exists(),
        "worktree 与 include 层声明的 filter 命令不得被执行"
    );
    assert_eq!(
        fs::read(temp.path().join(".git/index")).expect("index"),
        index_before,
        "查询不得写入索引"
    );
    assert_eq!(
        fs::read(temp.path().join("tracked.dat")).expect("dat"),
        dat_before,
        "查询不得改写工作树"
    );
    assert_eq!(
        fs::read(temp.path().join("tracked.txt")).expect("txt"),
        txt_before,
        "查询不得改写工作树"
    );
}

/// 空驱动名（`.gitattributes` 的 `filter=` + `[filter ""]`）同样必须被中和：
/// 覆盖键是 `filter..required=false` 与 `filter..clean=` 这类带空子段的形式。
#[tokio::test]
async fn empty_filter_driver_is_neutralized() {
    let temp = TempDir::new("empty-filter");
    init_repo(temp.path());
    write(temp.path(), "tracked.dat", b"base\n");
    commit(temp.path(), "base");

    // filter 属性与配置都放在提交之后，避免测试自身的 `git add` 触发脚本。
    write(temp.path(), ".gitattributes", b"*.dat filter=\n");
    let marker = temp.path().join("ran.marker");
    let command = filter_marker_script(temp.path(), &marker);
    for attribute in ["clean", "smudge", "process"] {
        git_ok(
            temp.path(),
            &["config", &format!("filter..{attribute}"), &command],
        );
    }

    write(temp.path(), "tracked.dat", b"changed\n");
    let index_before = fs::read(temp.path().join(".git/index")).expect("index");
    let dat_before = fs::read(temp.path().join("tracked.dat")).expect("dat");

    let adapter = adapter();
    let root = temp.root();
    let page = adapter.git_status(&root, None).await.expect("status");
    assert_eq!(page.git_state(), GitState::Available);
    let diff = adapter
        .git_diff(&root, &file_path("tracked.dat"))
        .await
        .expect("filtered diff");
    assert_eq!(diff.kind(), GitDiffKind::Text);

    assert!(!marker.exists(), "空 filter 驱动名不得绕过中和执行仓库命令");
    assert_eq!(
        fs::read(temp.path().join(".git/index")).expect("index"),
        index_before,
        "查询不得写入索引"
    );
    assert_eq!(
        fs::read(temp.path().join("tracked.dat")).expect("dat"),
        dat_before,
        "查询不得改写工作树"
    );
}

/// 驱动名无法按 UTF-8 无损解码时，覆盖键无法与真实驱动名精确匹配；
/// 此时必须整体失败关闭（Git 不可用），绝不生成近似键后继续。
#[tokio::test]
async fn non_utf8_filter_driver_fails_closed() {
    let temp = TempDir::new("non-utf8-filter");
    init_repo(temp.path());
    write(temp.path(), "tracked.dat", b"base\n");
    commit(temp.path(), "base");

    write(temp.path(), ".gitattributes", b"*.dat filter=\xff\n");
    let marker = temp.path().join("ran.marker");
    let command = filter_marker_script(temp.path(), &marker);

    // 驱动名含 0xFF：只能以原始字节写入配置，Git 会原样输出该键名。
    let mut config = fs::read(temp.path().join(".git/config")).expect("config");
    config.extend_from_slice(b"[filter \"\xff\"]\n\tclean = ");
    config.extend_from_slice(command.as_bytes());
    config.extend_from_slice(b"\n");
    fs::write(temp.path().join(".git/config"), config).expect("config");

    write(temp.path(), "tracked.dat", b"changed\n");
    let index_before = fs::read(temp.path().join(".git/index")).expect("index");
    let dat_before = fs::read(temp.path().join("tracked.dat")).expect("dat");

    let adapter = adapter();
    let root = temp.root();
    let page = adapter.git_status(&root, None).await.expect("status");
    assert_eq!(
        page.git_state(),
        GitState::Unavailable,
        "非 UTF-8 驱动名必须失败关闭"
    );
    let diff = adapter
        .git_diff(&root, &file_path("tracked.dat"))
        .await
        .expect("diff");
    assert_eq!(
        diff.unavailable(),
        Some(GitDiffUnavailableReason::GitUnavailable)
    );

    assert!(!marker.exists(), "不可解码的驱动名不得被近似键绕过");
    assert_eq!(
        fs::read(temp.path().join(".git/index")).expect("index"),
        index_before,
        "查询不得写入索引"
    );
    assert_eq!(
        fs::read(temp.path().join("tracked.dat")).expect("dat"),
        dat_before,
        "查询不得改写工作树"
    );
}

/// 驱动名含 `=` 时不能生成覆盖键：Git 按第一个 `=` 拆分 `-c key=value`，
/// `filter.a=b.clean=` 实际覆盖的是键 `filter.a`，真实驱动 `a=b` 的命令仍会执行。
/// 此时必须失败关闭（Git 不可用），不得生成近似键。
#[tokio::test]
async fn filter_driver_with_equals_fails_closed() {
    let temp = TempDir::new("equals-filter");
    init_repo(temp.path());
    write(temp.path(), "tracked.dat", b"base\n");
    commit(temp.path(), "base");

    write(temp.path(), ".gitattributes", b"*.dat filter=a=b\n");
    let marker = temp.path().join("ran.marker");
    let command = filter_marker_script(temp.path(), &marker);

    // `git config filter.a=b.clean <cmd>` 会被当作无效键拒绝；驱动名只能原样写入配置。
    let mut config = fs::read(temp.path().join(".git/config")).expect("config");
    config.extend_from_slice(b"[filter \"a=b\"]\n\tclean = ");
    config.extend_from_slice(command.as_bytes());
    config.extend_from_slice(b"\nsmudge = ");
    config.extend_from_slice(command.as_bytes());
    config.extend_from_slice(b"\nprocess = ");
    config.extend_from_slice(command.as_bytes());
    config.extend_from_slice(b"\n");
    fs::write(temp.path().join(".git/config"), config).expect("config");

    write(temp.path(), "tracked.dat", b"changed\n");
    let index_before = fs::read(temp.path().join(".git/index")).expect("index");
    let dat_before = fs::read(temp.path().join("tracked.dat")).expect("dat");

    let adapter = adapter();
    let root = temp.root();
    let page = adapter.git_status(&root, None).await.expect("status");
    assert_eq!(
        page.git_state(),
        GitState::Unavailable,
        "含 `=` 的驱动名必须失败关闭"
    );
    let diff = adapter
        .git_diff(&root, &file_path("tracked.dat"))
        .await
        .expect("diff");
    assert_eq!(
        diff.unavailable(),
        Some(GitDiffUnavailableReason::GitUnavailable)
    );

    assert!(!marker.exists(), "含 `=` 的驱动名不得被近似键绕过");
    assert_eq!(
        fs::read(temp.path().join(".git/index")).expect("index"),
        index_before,
        "查询不得写入索引"
    );
    assert_eq!(
        fs::read(temp.path().join("tracked.dat")).expect("dat"),
        dat_before,
        "查询不得改写工作树"
    );
}

/// 未跟踪文件（以及无 HEAD 的新增文件）走 `git diff --no-index`；该分支同样会按
/// `.gitattributes` 执行 clean filter，因此必须和其它 diff 一样先中和仓库声明的驱动。
/// 驱动名含 `=` 时无法生成精确覆盖键：必须与 HEAD 分支一致地失败关闭。
#[tokio::test]
async fn untracked_no_index_diff_fails_closed_for_unrepresentable_filter_driver() {
    let temp = TempDir::new("no-index-equals-filter");
    init_repo(temp.path());
    write(temp.path(), "tracked.txt", b"base\n");
    commit(temp.path(), "base");

    write(temp.path(), ".gitattributes", b"*.dat filter=a=b\n");
    let marker = temp.path().join("ran.marker");
    let command = filter_marker_script(temp.path(), &marker);

    // `git config filter.a=b.clean <cmd>` 会被当作无效键拒绝；驱动名只能原样写入配置。
    let mut config = fs::read(temp.path().join(".git/config")).expect("config");
    config.extend_from_slice(b"[filter \"a=b\"]\n");
    for attribute in ["clean", "smudge", "process"] {
        config.extend_from_slice(b"\t");
        config.extend_from_slice(attribute.as_bytes());
        config.extend_from_slice(b" = ");
        config.extend_from_slice(command.as_bytes());
        config.extend_from_slice(b"\n");
    }
    fs::write(temp.path().join(".git/config"), config).expect("config");

    write(temp.path(), "untracked.dat", b"changed\n");
    let index_before = fs::read(temp.path().join(".git/index")).expect("index");
    let dat_before = fs::read(temp.path().join("untracked.dat")).expect("dat");

    let adapter = adapter();
    let root = temp.root();
    let diff = adapter
        .git_diff(&root, &file_path("untracked.dat"))
        .await
        .expect("diff");
    assert_eq!(
        diff.unavailable(),
        Some(GitDiffUnavailableReason::GitUnavailable),
        "no-index 分支同样必须失败关闭"
    );

    assert!(
        !marker.exists(),
        "含 `=` 的驱动名不得经由 no-index 分支绕过中和"
    );
    assert_eq!(
        fs::read(temp.path().join(".git/index")).expect("index"),
        index_before,
        "查询不得写入索引"
    );
    assert_eq!(
        fs::read(temp.path().join("untracked.dat")).expect("dat"),
        dat_before,
        "查询不得改写工作树"
    );
}

/// 未跟踪文件走 `--no-index` 时，仓库声明的普通 clean filter 也必须被置空：
/// 若 guard 未打开，Git 会对工作树内容执行仓库命令。
#[tokio::test]
async fn untracked_no_index_diff_neutralizes_clean_filter() {
    let temp = TempDir::new("no-index-filter");
    init_repo(temp.path());
    write(temp.path(), "tracked.txt", b"base\n");
    commit(temp.path(), "base");

    write(temp.path(), ".gitattributes", b"*.dat filter=evil\n");
    let marker = temp.path().join("ran.marker");
    let command = filter_marker_script(temp.path(), &marker);
    for attribute in ["clean", "smudge", "process"] {
        git_ok(
            temp.path(),
            &["config", &format!("filter.evil.{attribute}"), &command],
        );
    }

    write(temp.path(), "untracked.dat", b"changed\n");
    let index_before = fs::read(temp.path().join(".git/index")).expect("index");
    let dat_before = fs::read(temp.path().join("untracked.dat")).expect("dat");

    let adapter = adapter();
    let root = temp.root();
    let diff = adapter
        .git_diff(&root, &file_path("untracked.dat"))
        .await
        .expect("diff");
    assert_eq!(diff.kind(), GitDiffKind::Text);
    assert!(diff.lines().iter().any(|line| line == "+changed"));

    assert!(
        !marker.exists(),
        "no-index 分支不得执行仓库声明的 clean filter"
    );
    assert_eq!(
        fs::read(temp.path().join(".git/index")).expect("index"),
        index_before,
        "查询不得写入索引"
    );
    assert_eq!(
        fs::read(temp.path().join("untracked.dat")).expect("dat"),
        dat_before,
        "查询不得改写工作树"
    );
}

#[tokio::test]
async fn large_git_output_is_read_after_child_exit() {
    let temp = TempDir::new("large-output");
    let script = temp.path().join(if cfg!(windows) {
        "large.cmd"
    } else {
        "large.sh"
    });
    #[cfg(windows)]
    fs::write(
        &script,
        "@echo off\r\nfor /L %%i in (1,1,4000) do @echo 0123456789abcdef\r\n",
    )
    .expect("script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::write(
            &script,
            "#!/bin/sh\ni=0\nwhile [ \"$i\" -lt 4000 ]; do echo 0123456789abcdef; i=$((i+1)); done\n",
        )
        .expect("script");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let (program, args) = if cfg!(windows) {
        (
            "cmd",
            vec![OsString::from("/c"), script.as_os_str().to_os_string()],
        )
    } else {
        ("/bin/sh", vec![script.as_os_str().to_os_string()])
    };
    let runner = super::git::GitRunner::new(OsString::from(program), Duration::from_secs(10), None);
    let (stdout, code, truncated) = match runner
        .run_raw_for_test(temp.path(), &args, 1024 * 1024)
        .await
    {
        Ok(value) => value,
        Err(_) => panic!("假 Git 调用失败"),
    };
    assert_eq!(code, Some(0));
    assert!(!truncated);
    assert!(
        stdout.len() > 8 * 1024,
        "输出必须跨越多次 8 KiB 读取，实际 {} 字节",
        stdout.len()
    );
    let text = std::str::from_utf8(&stdout).expect("ASCII 输出");
    assert_eq!(text.lines().count(), 4000, "子进程退出后管道必须读到 EOF");
    assert_eq!(text.lines().last(), Some("0123456789abcdef"));
}

#[cfg(windows)]
#[tokio::test]
async fn git_timeout_kills_the_whole_process_tree_on_windows() {
    let temp = TempDir::new("tree-win");
    let marker = temp.path().join("late.marker");
    let child = temp.path().join("child.cmd");
    fs::write(
        &child,
        format!(
            "@echo off\r\nping -n 4 127.0.0.1 >nul\r\necho late> \"{}\"\r\n",
            marker.display()
        ),
    )
    .expect("child script");
    let fake = temp.path().join("fake.cmd");
    fs::write(
        &fake,
        format!(
            "@echo off\r\nstart \"\" /b \"{}\"\r\nping -n 30 127.0.0.1 >nul\r\n",
            child.display()
        ),
    )
    .expect("fake script");
    let adapter = adapter_with(SessionFilesConfig {
        git_program: OsString::from("cmd"),
        git_timeout: Duration::from_millis(300),
        test_git_args: Some(vec![OsString::from("/c"), fake.as_os_str().to_os_string()]),
        ..SessionFilesConfig::default()
    });
    let root = temp.root();
    let status = tokio::time::timeout(Duration::from_secs(8), adapter.git_status(&root, None))
        .await
        .expect("超时路径必须在有界时间内返回")
        .expect("status");
    assert_eq!(status.git_state(), GitState::Unavailable);
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(!marker.exists(), "Job Object 必须结束孙进程");
}

#[cfg(unix)]
#[tokio::test]
async fn git_timeout_kills_the_whole_process_group() {
    let temp = TempDir::new("tree-unix");
    let marker = temp.path().join("late.marker");
    let script = temp.path().join("fake.sh");
    fs::write(
        &script,
        "#!/bin/sh\n( sleep 2; echo late > \"$1\" ) &\nsleep 30\n",
    )
    .expect("script");
    let adapter = adapter_with(SessionFilesConfig {
        git_program: OsString::from("/bin/sh"),
        git_timeout: Duration::from_millis(300),
        test_git_args: Some(vec![
            script.as_os_str().to_os_string(),
            marker.as_os_str().to_os_string(),
        ]),
        ..SessionFilesConfig::default()
    });
    let root = temp.root();
    let status = tokio::time::timeout(Duration::from_secs(8), adapter.git_status(&root, None))
        .await
        .expect("超时路径必须在有界时间内返回")
        .expect("status");
    assert_eq!(status.git_state(), GitState::Unavailable);
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(!marker.exists(), "进程组必须连同孙进程一起结束");
}

#[cfg(windows)]
#[tokio::test]
async fn dropping_a_git_query_kills_the_process_tree() {
    let temp = TempDir::new("cancel-win");
    let marker = temp.path().join("cancelled.marker");
    let child = temp.path().join("child.cmd");
    fs::write(
        &child,
        format!(
            "@echo off\r\nping -n 4 127.0.0.1 >nul\r\necho late> \"{}\"\r\n",
            marker.display()
        ),
    )
    .expect("child script");
    let fake = temp.path().join("fake.cmd");
    fs::write(
        &fake,
        format!(
            "@echo off\r\nstart \"\" /b \"{}\"\r\nping -n 30 127.0.0.1 >nul\r\n",
            child.display()
        ),
    )
    .expect("fake script");
    let adapter = adapter_with(SessionFilesConfig {
        git_program: OsString::from("cmd"),
        git_timeout: Duration::from_secs(30),
        test_git_args: Some(vec![OsString::from("/c"), fake.as_os_str().to_os_string()]),
        ..SessionFilesConfig::default()
    });
    let root = temp.root();
    let mut query = Box::pin(adapter.git_status(&root, None));
    tokio::select! {
        _ = query.as_mut() => panic!("查询不应在取消前完成"),
        _ = tokio::time::sleep(Duration::from_millis(200)) => {}
    }
    drop(query);
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(!marker.exists(), "future 被丢弃后必须结束整棵进程树");
}

#[cfg(unix)]
#[tokio::test]
async fn dropping_a_git_query_kills_the_process_group() {
    let temp = TempDir::new("cancel-unix");
    let marker = temp.path().join("cancelled.marker");
    let script = temp.path().join("fake.sh");
    fs::write(
        &script,
        "#!/bin/sh\n( sleep 2; echo late > \"$1\" ) &\nsleep 30\n",
    )
    .expect("script");
    let adapter = adapter_with(SessionFilesConfig {
        git_program: OsString::from("/bin/sh"),
        git_timeout: Duration::from_secs(30),
        test_git_args: Some(vec![
            script.as_os_str().to_os_string(),
            marker.as_os_str().to_os_string(),
        ]),
        ..SessionFilesConfig::default()
    });
    let root = temp.root();
    let mut query = Box::pin(adapter.git_status(&root, None));
    tokio::select! {
        _ = query.as_mut() => panic!("查询不应在取消前完成"),
        _ = tokio::time::sleep(Duration::from_millis(200)) => {}
    }
    drop(query);
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(!marker.exists(), "future 被丢弃后必须结束进程组");
}

#[tokio::test]
async fn direct_git_diff_on_outside_link_is_rejected_before_git_runs() {
    let outside = TempDir::new("diff-outside");
    write(outside.path(), "secret.txt", b"secret");
    let temp = TempDir::new("diff-link");
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        outside.path().join("secret.txt"),
        temp.path().join("link.txt"),
    )
    .expect("symlink");
    #[cfg(windows)]
    junction(&temp.path().join("link.txt"), outside.path());
    let adapter = adapter();
    let root = temp.root();
    let result = adapter.git_diff(&root, &file_path("link.txt")).await;
    assert!(result.is_err(), "根外链接的差异请求必须在启动 Git 前拒绝");
}

#[test]
fn session_list_path_round_trips() {
    assert_eq!(SessionListPath::root().as_wire(), "");
    assert_eq!(
        SessionListPath::parse("src").expect("path").as_wire(),
        "src"
    );
}

// 适配器是 `Send + Sync`，可注入 `Arc<dyn SessionFiles>`。
fn _assert_send_sync() {
    fn check<T: Send + Sync + 'static>() {}
    check::<NativeSessionFiles>();
    let _: Arc<dyn SessionFiles> = Arc::new(NativeSessionFiles::new());
}

// 避免未使用导入告警：这些常量是端口预算的一部分，测试通过断言使用。
const _: usize = SESSION_FILE_CHUNK_MAX_BYTES;

//! 受管理的本机 Git 子进程（design D4）。
//!
//! 只读、固定参数、不经 shell：参数以数组直接传给 `git`，用户提供的路径一律作为 `:(literal)` pathspec。
//! 每次调用都在 `agent_host::platform::ProcessTree` 下运行（Windows Job Object / Unix 进程组），单次
//! 10 秒超时；超时、取消或 future 被丢弃时结束整棵进程树。stdout/stderr 都有上限，超限即终止进程。
//!
//! 安全开关：`--no-optional-locks` 与 `GIT_OPTIONAL_LOCKS=0`（查询不写索引）、`--no-ext-diff`、
//! `--no-textconv`、`core.fsmonitor=false`、`core.hooksPath=`、禁用 pager，并在运行 status/diff 前把
//! 仓库本地配置里声明的 `filter.*.clean/smudge/process` 全部置空——仓库指定的命令不会因为「看一眼
//! 文件」而被执行。Git 缺失只影响 Git 视图与忽略过滤，文件枚举照常工作。

use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use acp_core::model::{
    GIT_DIFF_LINE_MAX_CHARS, GIT_DIFF_MAX_BYTES, GIT_DIFF_MAX_LINES, GitDiff, GitDiffBase,
    GitDiffKind, GitDiffUnavailableReason, GitFileStatus, GitState, GitStatusEntry, PortError,
    RESULT_TOO_LARGE, SessionFilePath, UnavailableKind,
};
use agent_host::platform::ProcessTree;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

/// 单次 Git 调用的超时（design D5：10 秒）。
pub(crate) const GIT_TIMEOUT: Duration = Duration::from_secs(10);
/// status 输出的上限；超限明确失败而不是截断成看似完整的列表。
const STATUS_MAX_BYTES: usize = 2 * 1024 * 1024;
/// 单条 status/diff 辅助调用的 stdout 上限。
const AUX_MAX_BYTES: usize = 1024 * 1024;
/// stderr 只用于区分失败类别，绝不进入协议错误或日志正文。
const STDERR_MAX_BYTES: usize = 8 * 1024;
/// `check-ignore` 输入上限。
const IGNORE_INPUT_MAX_BYTES: usize = 1024 * 1024;

/// Git 子进程的运行器。路径与超时由组合根配置注入（测试可替换）。
pub(crate) struct GitRunner {
    program: OsString,
    timeout: Duration,
    /// 测试用：以这些参数完全替代固定前缀与调用参数（验证超时与进程树清理）。
    test_args: Option<Vec<OsString>>,
}

/// 一次 Git 调用的有界结果。
struct GitOutput {
    stdout: Vec<u8>,
    code: Option<i32>,
    truncated: bool,
}

impl GitOutput {
    fn success(&self) -> bool {
        self.code == Some(0)
    }
}

/// Git 子进程层面的失败（与「仓库说没有」不同）。
pub(crate) enum GitFailure {
    Missing,
    Timeout,
    Failed,
    /// 无法枚举/精确解码仓库声明的 filter 驱动：无法证明命令已被中和，必须失败关闭。
    FiltersUnreadable,
}

impl GitRunner {
    pub(crate) fn new(
        program: OsString,
        timeout: Duration,
        test_args: Option<Vec<OsString>>,
    ) -> Self {
        Self {
            program,
            timeout,
            test_args,
        }
    }

    /// Git 查询的可用性。非仓库与 Git 缺失是两种显式状态。
    pub(crate) async fn state(&self, root: &Path) -> GitState {
        match self.probe(root).await {
            Probe::Repo(_) => GitState::Available,
            Probe::NotRepository => GitState::NotRepository,
            Probe::Unavailable => GitState::Unavailable,
        }
    }

    async fn probe(&self, root: &Path) -> Probe {
        let args = os_args(&[
            "rev-parse",
            "--is-inside-work-tree",
            "--show-prefix",
            "--show-toplevel",
        ]);
        match self.run(root, &args, None, AUX_MAX_BYTES, false).await {
            Err(_) => Probe::Unavailable,
            Ok(output) => {
                if !output.success() {
                    // `rev-parse --is-inside-work-tree` 失败只说明这个目录不能作为工作树使用
                    // （非仓库、损坏的 `.git` 文件等）；Git 缺失与超时由上面的 spawn 错误区分。
                    return Probe::NotRepository;
                }
                let text = String::from_utf8_lossy(&output.stdout);
                let mut lines = text.lines();
                let inside = lines.next().unwrap_or("").trim();
                if inside != "true" {
                    return Probe::NotRepository;
                }
                let prefix = lines.next().unwrap_or("").trim_end_matches(['\r', '\n']);
                let prefix = if prefix.is_empty() {
                    String::new()
                } else if prefix.ends_with('/') {
                    prefix.to_owned()
                } else {
                    format!("{prefix}/")
                };
                Probe::Repo(RepoInfo { prefix })
            }
        }
    }

    /// `git check-ignore --stdin -z`：返回被有效忽略规则命中的根内相对路径。
    /// 失败时调用方必须显式降级（保留全部文件并报告 gitState 不可用），不能假装已过滤。
    pub(crate) async fn check_ignored(
        &self,
        root: &Path,
        paths: &[String],
    ) -> Result<std::collections::HashSet<String>, GitFailure> {
        let mut ignored = std::collections::HashSet::new();
        if paths.is_empty() {
            return Ok(ignored);
        }
        let mut input = Vec::new();
        for path in paths {
            let path = path.replace('\\', "/");
            if input.len() + path.len() + 1 > IGNORE_INPUT_MAX_BYTES {
                return Err(GitFailure::Failed);
            }
            input.extend_from_slice(path.as_bytes());
            input.push(0);
        }
        let args = os_args(&["check-ignore", "--stdin", "-z"]);
        let output = self
            .run(root, &args, Some(&input), AUX_MAX_BYTES, false)
            .await?;
        // exit 0 = 至少一个被忽略；exit 1 = 一个都没有。
        match output.code {
            Some(0) => {}
            Some(1) => return Ok(ignored),
            _ => return Err(GitFailure::Failed),
        }
        for field in output.stdout.split(|byte| *byte == 0) {
            if field.is_empty() {
                continue;
            }
            if let Ok(text) = std::str::from_utf8(field) {
                ignored.insert(text.replace('\\', "/"));
            }
        }
        Ok(ignored)
    }

    /// 合并后的 `HEAD` 与工作树状态，只覆盖会话根内的文件。
    pub(crate) async fn status(&self, root: &Path) -> Result<StatusView, PortError> {
        let prefix = match self.probe(root).await {
            Probe::Repo(info) => info.prefix,
            Probe::NotRepository => return Ok(StatusView::NotRepository),
            Probe::Unavailable => return Ok(StatusView::Unavailable),
        };
        let args = os_args(&[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=all",
            "--find-renames",
            "--",
            ".",
        ]);
        let output = match self.run(root, &args, None, STATUS_MAX_BYTES, true).await {
            Ok(output) => output,
            Err(_) => return Ok(StatusView::Unavailable),
        };
        if output.truncated {
            return Err(PortError::InvalidRequest(RESULT_TOO_LARGE));
        }
        if !output.success() {
            return Ok(StatusView::Unavailable);
        }
        let mut entries = match parse_status(&output.stdout, &prefix) {
            Ok(entries) => entries,
            Err(_) => return Ok(StatusView::Unavailable),
        };
        entries.sort_by(|left, right| left.path.as_str().cmp(right.path.as_str()));
        Ok(StatusView::Entries(entries))
    }

    /// `HEAD` 与当前工作树的单文件差异。无 HEAD 时以空基准比较。
    pub(crate) async fn diff(
        &self,
        root: &Path,
        rel: &SessionFilePath,
    ) -> Result<GitDiff, PortError> {
        let (prefix, has_head) = match self.probe(root).await {
            Probe::Repo(info) => {
                let has_head = matches!(
                    self.run(
                        root,
                        &os_args(&["rev-parse", "--verify", "-q", "HEAD"]),
                        None,
                        AUX_MAX_BYTES,
                        false,
                    )
                    .await,
                    Ok(output) if output.success()
                );
                (info.prefix, has_head)
            }
            Probe::NotRepository => {
                return GitDiff::try_new(
                    rel.clone(),
                    GitDiffBase::Head,
                    GitDiffKind::Text,
                    Vec::new(),
                    Some(GitDiffUnavailableReason::NotRepository),
                    false,
                )
                .map_err(|_| unavailable());
            }
            Probe::Unavailable => {
                return GitDiff::try_new(
                    rel.clone(),
                    GitDiffBase::Head,
                    GitDiffKind::Text,
                    Vec::new(),
                    Some(GitDiffUnavailableReason::GitUnavailable),
                    false,
                )
                .map_err(|_| unavailable());
            }
        };
        let base = if has_head {
            GitDiffBase::Head
        } else {
            GitDiffBase::Empty
        };

        // 冲突与改名来自单路径 status：冲突必须显式标识，改名只在旧路径也在根内时下发展示。
        let detail = self.path_status(root, prefix.as_str(), rel).await;
        let (conflicted, mut untracked, previous) = match &detail {
            Some(record) => (
                record.conflicted,
                record.status == GitFileStatus::Untracked,
                record.previous.clone(),
            ),
            None => (false, false, None),
        };
        if conflicted {
            return GitDiff::try_new(
                rel.clone(),
                base,
                GitDiffKind::Conflict,
                Vec::new(),
                None,
                false,
            )
            .map_err(|_| unavailable());
        }
        if detail.is_none() {
            // status 里没有这条路径：未跟踪（含被忽略）文件按空基准的全量新增展示；
            // 已在索引中的文件继续走 HEAD 比较。
            untracked = self.is_untracked(root, rel).await;
        }
        if crate::session_files::content::looks_binary_root(&root.to_string_lossy(), rel) {
            return GitDiff::try_new(
                rel.clone(),
                base,
                GitDiffKind::Binary,
                Vec::new(),
                None,
                false,
            )
            .map_err(|_| unavailable());
        }

        let literal = |path: &SessionFilePath| format!(":(literal){}", path.as_str());
        let literal_rel = literal(rel);
        let use_empty_base = !has_head || untracked;
        let output = if use_empty_base {
            let mut args = os_args(&[
                "diff",
                "--no-index",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "--",
                null_device(),
            ]);
            args.push(OsString::from(rel.as_str()));
            match self.run(root, &args, None, GIT_DIFF_MAX_BYTES, true).await {
                Ok(output) => output,
                // `--no-index` 同样会按 `.gitattributes` 执行 clean filter：无法证明命令已被
                // 中和时必须失败关闭，而不是回退到未加 guard 的直接调用。
                Err(GitFailure::FiltersUnreadable) => {
                    return GitDiff::try_new(
                        rel.clone(),
                        base,
                        GitDiffKind::Text,
                        Vec::new(),
                        Some(GitDiffUnavailableReason::GitUnavailable),
                        false,
                    )
                    .map_err(|_| unavailable());
                }
                Err(_) => return Err(unavailable()),
            }
        } else {
            let mut args = os_args(&[
                "diff",
                "--relative",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "--unified=3",
                "--find-renames",
                "HEAD",
                "--",
                literal_rel.as_str(),
            ]);
            if let Some(previous) = &previous {
                args.insert(args.len() - 1, OsString::from(literal(previous)));
            }
            match self.run(root, &args, None, GIT_DIFF_MAX_BYTES, true).await {
                Ok(output) => output,
                // filter 驱动无法枚举/解码：差异查询明确报告 Git 不可用（失败关闭）。
                Err(GitFailure::FiltersUnreadable) => {
                    return GitDiff::try_new(
                        rel.clone(),
                        base,
                        GitDiffKind::Text,
                        Vec::new(),
                        Some(GitDiffUnavailableReason::GitUnavailable),
                        false,
                    )
                    .map_err(|_| unavailable());
                }
                Err(_) => return Err(unavailable()),
            }
        };
        // `--no-index` 在「有差异」时以 1 退出；常规 diff 非零才是错误。输出超限时进程被主动终止，
        // 此时没有可信退出码，但已捕获的前缀必须作为显式 truncated 差异返回。
        let allowed_codes: &[Option<i32>] = if use_empty_base {
            &[Some(0), Some(1)]
        } else {
            &[Some(0)]
        };
        if !output.truncated && !allowed_codes.contains(&output.code) {
            return GitDiff::try_new(
                rel.clone(),
                base,
                GitDiffKind::Text,
                Vec::new(),
                Some(GitDiffUnavailableReason::GitUnavailable),
                false,
            )
            .map_err(|_| unavailable());
        }
        let text = match decode_diff_output(&output.stdout, output.truncated) {
            Some(text) => text,
            None => {
                return GitDiff::try_new(
                    rel.clone(),
                    base,
                    GitDiffKind::Binary,
                    Vec::new(),
                    None,
                    false,
                )
                .map_err(|_| unavailable());
            }
        };
        if text
            .lines()
            .any(|line| line.starts_with("Binary files ") || line.starts_with("GIT binary patch"))
        {
            return GitDiff::try_new(
                rel.clone(),
                base,
                GitDiffKind::Binary,
                Vec::new(),
                None,
                false,
            )
            .map_err(|_| unavailable());
        }
        let (lines, over_limit) = bounded_lines(text);
        GitDiff::try_new(
            rel.clone(),
            base,
            GitDiffKind::Text,
            lines,
            None,
            output.truncated || over_limit,
        )
        .map_err(|_| unavailable())
    }

    /// 路径不在索引中（未跟踪或被忽略）→ 差异按空基准的全量新增表达。
    async fn is_untracked(&self, root: &Path, rel: &SessionFilePath) -> bool {
        let pathspec = format!(":(literal){}", rel.as_str());
        let args = os_args(&["ls-files", "-z", "--", pathspec.as_str()]);
        match self.run(root, &args, None, AUX_MAX_BYTES, false).await {
            Ok(output) if output.success() && !output.truncated => output.stdout.is_empty(),
            _ => false,
        }
    }

    async fn path_status(
        &self,
        root: &Path,
        prefix: &str,
        rel: &SessionFilePath,
    ) -> Option<GitEntry> {
        let pathspec = format!(":(literal){}", rel.as_str());
        let args = os_args(&[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=all",
            "--find-renames",
            "--",
            pathspec.as_str(),
        ]);
        let output = self
            .run(root, &args, None, AUX_MAX_BYTES, true)
            .await
            .ok()?;
        if !output.success() || output.truncated {
            return None;
        }
        parse_status(&output.stdout, prefix)
            .ok()?
            .into_iter()
            .find(|entry| entry.path == *rel)
    }

    async fn run(
        &self,
        root: &Path,
        args: &[OsString],
        stdin: Option<&[u8]>,
        max_stdout: usize,
        guard_filters: bool,
    ) -> Result<GitOutput, GitFailure> {
        let mut all: Vec<OsString> = Vec::new();
        if let Some(override_args) = &self.test_args {
            all.extend(override_args.iter().cloned());
        } else {
            all.extend(prefix_args());
            if guard_filters {
                all.extend(self.filter_overrides(root).await?);
            }
            all.extend(args.iter().cloned());
        }
        self.run_once(root, &all, stdin, max_stdout).await
    }

    /// 仓库本地配置声明的 filter 命令全部置空：status/diff 不会因为读取工作树而执行它们。
    /// 无法枚举时失败关闭（宁可报告 Git 不可用，也不执行未识别的仓库命令）。
    ///
    /// 这里刻意不限定 scope：`--local` 会漏掉 `extensions.worktreeConfig` 启用的
    /// `.git/config.worktree`，且给定具体 scope 时 Git 默认不展开 `include.path`。system/global
    /// 已由环境变量关闭，因此「全部配置层 + includes」恰好覆盖仓库能让 status/diff 看到的命令。
    async fn filter_overrides(&self, root: &Path) -> Result<Vec<OsString>, GitFailure> {
        let args = os_args(&[
            "config",
            "--name-only",
            "--includes",
            "--get-regexp",
            r"^filter\..*\.(clean|smudge|process)$",
        ]);
        let output = self.run_once(root, &args, None, 64 * 1024).await?;
        if output.truncated || !matches!(output.code, Some(0) | Some(1)) {
            return Err(GitFailure::FiltersUnreadable);
        }
        // 配置键按原始字节输出：驱动名必须无损解码，否则生成的覆盖键与真实驱动名不匹配，
        // Git 仍会执行仓库命令。无法解码时整体失败关闭（报告 Git 不可用）。
        let text =
            std::str::from_utf8(&output.stdout).map_err(|_| GitFailure::FiltersUnreadable)?;
        let mut overrides = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for line in text.lines() {
            let Some(rest) = line.trim().strip_prefix("filter.") else {
                continue;
            };
            let Some((driver, attribute)) = rest.rsplit_once('.') else {
                continue;
            };
            // 空驱动名同样是合法（且危险）的：`.gitattributes` 的 `filter=` 与 `[filter ""]`
            // 会匹配到它，覆盖键因此是 `filter..clean=` 这样带空子段的形式。
            if !matches!(attribute, "clean" | "smudge" | "process") {
                continue;
            }
            // Git 按键名里第一个 `=` 拆分 `-c key=value`：驱动名本身含 `=` 时，生成的
            // `filter.<driver>.<attribute>=` 只会覆盖另一个键，真实驱动的命令仍然生效。
            // 控制字符同样无法与配置键精确对应。两种情况都绝不生成近似键，直接失败关闭。
            if driver.contains('=') || driver.chars().any(char::is_control) {
                return Err(GitFailure::FiltersUnreadable);
            }
            if !seen.insert(driver.to_owned()) {
                continue;
            }
            overrides.push(OsString::from("-c"));
            overrides.push(OsString::from(format!("filter.{driver}.required=false")));
            for attribute in ["clean", "smudge", "process"] {
                overrides.push(OsString::from("-c"));
                overrides.push(OsString::from(format!("filter.{driver}.{attribute}=")));
            }
        }
        Ok(overrides)
    }

    async fn run_once(
        &self,
        root: &Path,
        args: &[OsString],
        stdin: Option<&[u8]>,
        max_stdout: usize,
    ) -> Result<GitOutput, GitFailure> {
        let mut command = Command::new(&self.program);
        command.args(args);
        command.current_dir(root);
        configure_environment(&mut command);
        command.stdin(stdin.map_or(std::process::Stdio::null(), |_| {
            std::process::Stdio::piped()
        }));
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        command.kill_on_drop(true);

        let tree = ProcessTree::prepare(&mut command).map_err(|_| GitFailure::Failed)?;
        let mut child = command.spawn().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                GitFailure::Missing
            } else {
                GitFailure::Failed
            }
        })?;
        if tree.attach(&child).is_err() {
            tree.terminate();
            let _ = child.start_kill();
            return Err(GitFailure::Failed);
        }
        let guard = TreeGuard(tree);

        let deadline = tokio::time::Instant::now() + self.timeout;
        if let Some(input) = stdin {
            let Some(mut pipe) = child.stdin.take() else {
                return Err(GitFailure::Failed);
            };
            let input = input.to_vec();
            match tokio::time::timeout_at(deadline, async move {
                pipe.write_all(&input).await?;
                pipe.shutdown().await
            })
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(_)) => {
                    guard.terminate();
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    return Err(GitFailure::Failed);
                }
                Err(_) => {
                    guard.terminate();
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    return Err(GitFailure::Timeout);
                }
            }
        }

        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut out_done = stdout.is_none();
        let mut err_done = stderr.is_none();
        let mut truncated = false;
        let mut status = None;
        loop {
            if status.is_some() && out_done && err_done {
                break;
            }
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => {
                    guard.terminate();
                    let _ = child.start_kill();
                    let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
                    return Err(GitFailure::Timeout);
                }
                result = read_chunk(&mut stdout, &mut out, max_stdout), if !out_done => {
                    match result {
                        Ok((0, _)) => out_done = true,
                        Ok((_, overflow)) => {
                            if overflow {
                                truncated = true;
                                out_done = true;
                                guard.terminate();
                                let _ = child.start_kill();
                            }
                        }
                        Err(_) => out_done = true,
                    }
                }
                result = read_chunk(&mut stderr, &mut err, STDERR_MAX_BYTES), if !err_done => {
                    match result {
                        Ok((0, _)) => err_done = true,
                        Ok((_, overflow)) => {
                            if overflow {
                                err_done = true;
                            }
                        }
                        Err(_) => err_done = true,
                    }
                }
                result = child.wait(), if status.is_none() => {
                    // 进程退出不意味着管道已读空：继续读 stdout/stderr 到 EOF。管道里可能仍有
                    // 最多一个管道缓冲区的数据，提前标记 done 会把非空输出读成空。
                    status = Some(result.ok().and_then(|status| status.code()));
                }
            }
        }
        Ok(GitOutput {
            stdout: out,
            code: status.flatten(),
            truncated,
        })
    }
}

#[cfg(test)]
impl GitRunner {
    /// 测试钩子：执行一次有界调用并暴露原始 stdout，用来验证读循环在子进程退出后仍读到 EOF。
    pub(crate) async fn run_raw_for_test(
        &self,
        root: &Path,
        args: &[OsString],
        max_stdout: usize,
    ) -> Result<(Vec<u8>, Option<i32>, bool), GitFailure> {
        let output = self.run_once(root, args, None, max_stdout).await?;
        Ok((output.stdout, output.code, output.truncated))
    }
}

/// 进程树句柄的 RAII 守卫：future 被取消/丢弃时同样结束整棵树。
struct TreeGuard(ProcessTree);

impl TreeGuard {
    fn terminate(&self) {
        self.0.terminate();
    }
}

impl Drop for TreeGuard {
    fn drop(&mut self) {
        self.0.terminate();
    }
}

enum Probe {
    Repo(RepoInfo),
    NotRepository,
    Unavailable,
}

struct RepoInfo {
    prefix: String,
}

/// status 查询的三种结果：非仓库、Git 不可用与根内条目（都是成功结果里的显式状态）。
pub(crate) enum StatusView {
    NotRepository,
    Unavailable,
    Entries(Vec<GitEntry>),
}

/// status 的内部条目（二进制标志在分页后按需计算）。
#[derive(Clone)]
pub(crate) struct GitEntry {
    pub(crate) path: SessionFilePath,
    pub(crate) status: GitFileStatus,
    pub(crate) previous: Option<SessionFilePath>,
    pub(crate) conflicted: bool,
}

fn prefix_args() -> Vec<OsString> {
    os_args(&[
        "--no-pager",
        "--no-optional-locks",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.untrackedCache=false",
        "-c",
        "core.hooksPath=",
        "-c",
        "core.pager=cat",
        "-c",
        "gc.auto=0",
        "-c",
        "maintenance.auto=false",
    ])
}

fn configure_environment(command: &mut Command) {
    command.env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    #[cfg(windows)]
    for name in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command.env("GIT_CONFIG_NOSYSTEM", "1");
    command.env("GIT_CONFIG_GLOBAL", null_device());
    command.env("GIT_ATTR_NOSYSTEM", "1");
    command.env("GIT_TERMINAL_PROMPT", "0");
    command.env("GIT_OPTIONAL_LOCKS", "0");
    command.env("GIT_PAGER", "cat");
    command.env("PAGER", "cat");
    command.env("LC_ALL", "C");
    command.env("LANG", "C");
}

fn null_device() -> &'static str {
    if cfg!(windows) { "NUL" } else { "/dev/null" }
}

fn os_args(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

async fn read_chunk<R: AsyncRead + Unpin>(
    reader: &mut Option<R>,
    target: &mut Vec<u8>,
    cap: usize,
) -> std::io::Result<(usize, bool)> {
    let Some(reader) = reader.as_mut() else {
        return Ok((0, false));
    };
    let mut scratch = [0_u8; 8 * 1024];
    let read = reader.read(&mut scratch).await?;
    if read == 0 {
        return Ok((0, false));
    }
    let room = cap.saturating_sub(target.len());
    let take = read.min(room);
    target.extend_from_slice(&scratch[..take]);
    Ok((read, take < read))
}

/// 解析 `--porcelain=v1 -z`。重命名记录在 `-z` 下是「新路径\0旧路径\0」，记录本身以 `XY ` 开头。
fn parse_status(bytes: &[u8], prefix: &str) -> Result<Vec<GitEntry>, ()> {
    let mut entries = Vec::new();
    let mut fields = bytes.split(|byte| *byte == 0).peekable();
    while let Some(field) = fields.next() {
        if field.is_empty() {
            continue;
        }
        if field.len() < 4 || field[2] != b' ' {
            return Err(());
        }
        let x = field[0];
        let y = field[1];
        let path_bytes = &field[3..];
        let Ok(path_text) = std::str::from_utf8(path_bytes) else {
            continue;
        };
        let previous_text = if matches!(x, b'R' | b'C') {
            fields
                .next()
                .and_then(|field| std::str::from_utf8(field).ok())
        } else {
            None
        };
        let new_rel = strip_prefix(path_text, prefix);
        let old_rel = previous_text.and_then(|text| strip_prefix(text, prefix));
        let (rel_text, previous_text) = match (new_rel, old_rel) {
            (Some(new), Some(old)) => (new, Some(old)),
            (Some(new), None) => (new, None),
            // 根外 → 根内的改名按根内新增表达；根内 → 根外按根内删除表达。
            (None, Some(old)) => (old, None),
            (None, None) => continue,
        };
        let Ok(path) = SessionFilePath::parse(rel_text) else {
            continue;
        };
        let previous = previous_text.and_then(|text| SessionFilePath::parse(text).ok());
        let conflicted = matches!((x, y), (b'U', _) | (_, b'U') | (b'A', b'A') | (b'D', b'D'));
        let status = if x == b'?' {
            GitFileStatus::Untracked
        } else if conflicted {
            GitFileStatus::Modified
        } else if x == b'R' || x == b'C' {
            GitFileStatus::Renamed
        } else if x == b'A' || y == b'A' {
            GitFileStatus::Added
        } else if x == b'D' || y == b'D' {
            GitFileStatus::Deleted
        } else {
            GitFileStatus::Modified
        };
        entries.push(GitEntry {
            path,
            status,
            previous: if status == GitFileStatus::Renamed {
                previous
            } else {
                None
            },
            conflicted,
        });
    }
    Ok(entries)
}

fn strip_prefix<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    if prefix.is_empty() {
        return (!path.is_empty()).then_some(path);
    }
    let rest = path.strip_prefix(prefix)?;
    if rest.is_empty() { None } else { Some(rest) }
}

/// 把差异输出截到 400 行 / 64 KiB / 单行 4096 字符；超限只置 truncated，不伪造额外文本。
fn bounded_lines(text: &str) -> (Vec<String>, bool) {
    let mut lines = Vec::new();
    let mut total = 0usize;
    let mut truncated = false;
    let mut iter = text.split('\n').peekable();
    while let Some(line) = iter.next() {
        // 输出通常以换行结尾：`split` 会多出一个空尾项，它不是内容行。
        if line.is_empty() && iter.peek().is_none() {
            break;
        }
        if lines.len() == GIT_DIFF_MAX_LINES {
            truncated = true;
            break;
        }
        let line = line.strip_suffix('\r').unwrap_or(line);
        let mut line = line.to_owned();
        if line.chars().count() > GIT_DIFF_LINE_MAX_CHARS {
            let end = line
                .char_indices()
                .nth(GIT_DIFF_LINE_MAX_CHARS)
                .map_or(line.len(), |(index, _)| index);
            line.truncate(end);
            truncated = true;
        }
        if total + line.len() > GIT_DIFF_MAX_BYTES {
            truncated = true;
            break;
        }
        total += line.len();
        lines.push(line);
    }
    (lines, truncated)
}

fn unavailable() -> PortError {
    PortError::Unavailable(UnavailableKind::IoError)
}

/// 解码有界的 diff 输出。输出达到上限被截断时，末尾可能正好落在 UTF-8 多字节字符中间：
/// 这种「合法前缀 + 不完整序列」是截断伪影，回退到最后一个完整字符边界再解析，仍标记 truncated。
/// 只有内容本身含非法字节（不是截断造成的）才判为 binary。
pub(crate) fn decode_diff_output(bytes: &[u8], truncated: bool) -> Option<&str> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Some(text),
        Err(error) if truncated && error.error_len().is_none() => {
            std::str::from_utf8(&bytes[..error.valid_up_to()]).ok()
        }
        Err(_) => None,
    }
}

/// 构建 Git 状态条目（二进制标志由调用方按页计算后传入）。
pub(crate) fn status_entry(entry: GitEntry, binary: bool) -> Result<GitStatusEntry, PortError> {
    GitStatusEntry::try_new(
        entry.path,
        entry.status,
        entry.previous,
        entry.conflicted,
        binary,
    )
    .map_err(|_| unavailable())
}

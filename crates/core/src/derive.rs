//! core 的派生投影（`docs/SYNC_PROTOCOL.md` §10.3 的三类「合同已登记、core 侧无生产者」的事件）。
//!
//! 本模块只做**纯函数**投影（不读时钟、不落盘、不发布），由 [`crate::broker`] 在组装提交时调用：
//!
//! - `file.changed`（R5–R7）：派生源**只有** ACP 工具调用内容里的类型化 Diff 元素（`design.md` D4/D5）。
//!   判定完全依据 `view` 的 `diff` 键——那个键由适配器（`agent-host`）从 `ToolCallContent::Diff` 逐字节
//!   投影而来；本模块**不读** `rawInput` 等自由形状字段，也不猜 Agent 的意图。
//! - 行数统计：对 `oldText`/`newText` 跑行级 Myers diff（O(ND)、线性空间），判定不出时**省略**两项而不是
//!   填零。行数差是错的且错得隐蔽（10 行换 10 行也可能是删 2 加 2），因此这里绝不使用行数差。
//! - 展示路径：用**规范化后的前缀关系**相对化（`/a/b` 与 `/a/bc` 不会被误判为前缀关系）；越界时只下发
//!   文件名称并置 `outsideWorkspace`，绝不泄漏工作目录根的任何片段或回退层级。
//! - 会话标题（R9）：从 `session.info.changed` 的 ACP 原文读标题意图——`SessionInfoUpdate` 的所有字段都是
//!   可选的（部分更新），因此「键缺席」= 不改、「显式 null」= 清空，必须是**两层可选**语义。

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest as _, Sha256};

use crate::model::{
    InvalidValue, array_items, decode_json_string, encode_json_string, json_object_members,
};

/// 每会话 `file.changed` 去重日志的容量。
///
/// 去重的键是 `(toolCallId, 展示路径)`：同一个工具调用可能在 `tool.call.started` 与
/// `tool.call.updated` 里重复携带同一个 Diff 元素，R5 要求「MUST NOT 为同一处改动重复派生」。
/// 日志按插入顺序做 FIFO 淘汰，容量只是内存上界——被淘汰的键对应的是早已过去的工具调用，
/// 再出现的概率可以忽略，且代价只是多一条提示性事件（行数统计不受影响）。
pub(crate) const FILE_CHANGE_DEDUP_CAPACITY: usize = 4096;

/// 行级 diff 的工作预算（基本步数）。
///
/// Myers 是 O((N+M)·D)：典型编辑的 D 很小，但「整文件重写」这种病态输入的 D 与行数同阶。超过预算时
/// 本模块**省略**行数统计而不是给出可能错误或耗时的结果——`design.md` D4 的风险条目与回滚条件 (b)
/// 正是「病态输入下省略行数统计，不整体回滚」。
const LINE_STAT_WORK_BUDGET: u64 = 8_000_000;

/// Myers 的 `V` 数组随 `N+M` 线性增长，因此 `N+M` 也要有上界（预算通常先被耗尽，这里是硬保护）。
const LINE_STAT_MAX_LINES: usize = 200_000;

/// 单个标题的长度上界（与 `Session::title`/`sessionSummary.title` 一致）。
const TITLE_MAX_CHARS: usize = 512;

// ---------------------------------------------------------------------------------------------
// 行级差异统计（R6）
// ---------------------------------------------------------------------------------------------

/// 行级差异统计：`(added, deleted)`；判定不出或超出工作预算时返回 `None`（**省略**，不是零）。
///
/// `old` 为 `None`（Diff 元素没有 `oldText`）时按**新建文件**处理：全部行计为新增、删除为零；
/// `new` 为 `None`（没有可用的改动后内容）时判定不出，返回 `None`。
pub(crate) fn line_stats(old: Option<&str>, new: Option<&str>) -> Option<(u64, u64)> {
    let new = new?;
    let new_lines = split_lines(new);
    let Some(old) = old else {
        return Some((new_lines.len() as u64, 0));
    };
    let old_lines = split_lines(old);
    if old_lines.is_empty() {
        return Some((new_lines.len() as u64, 0));
    }
    if new_lines.is_empty() {
        return Some((0, old_lines.len() as u64));
    }
    let distance = myers_distance(&old_lines, &new_lines)?;
    // D = (N - lcs) + (M - lcs) ⇒ lcs = (N + M - D) / 2，因此 added/deleted 由 D 唯一确定，
    // 不需要重建编辑脚本（线性空间的理由见 `myers_distance`）。
    let common_twice = old_lines.len() + new_lines.len() - distance;
    let common = common_twice / 2;
    Some((
        (new_lines.len() - common) as u64,
        (old_lines.len() - common) as u64,
    ))
}

/// 文本 → 行。只按 `\n` 切分；末尾的换行只表示**该行以换行结束**，不额外产生一条空行
/// （`"a\nb\n"` 与 `"a\nb"` 都是两行）。行数统计只服务于「有没有改动」的提示，不表达「末尾换行是否
/// 变化」；相反，若把末尾换行算成一条空行，「整文件换成 `""`」这样的删除会被误报成仍有新增行。
fn split_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}

/// 行级 Myers diff 的编辑距离 D（`O((N+M)·D)` 时间、`O(N+M)` 空间）；超出预算返回 `None`。
///
/// 用论文 Algorithm 1 的贪心前向搜索：按 D 递增处理对角线，首个到达 `(N, M)` 的深度即最小编辑距离。
/// 只需要 D（不需要编辑脚本），因此不必做双向搜索或分治；这也是 `line_stats` 能在线性空间里算出
/// `added`/`deleted` 的原因。
fn myers_distance(a: &[&str], b: &[&str]) -> Option<usize> {
    let n = a.len();
    let m = b.len();
    let max = n + m;
    if max > LINE_STAT_MAX_LINES {
        return None;
    }
    let offset = max + 1;
    let mut v = vec![0usize; 2 * max + 3];
    let mut budget = LINE_STAT_WORK_BUDGET;
    for d in 0..=max {
        let d = d as isize;
        for k in (-d..=d).step_by(2) {
            budget = budget.checked_sub(1)?;
            let index = (offset as isize + k) as usize;
            let mut x = if k == -d || (k != d && v[index - 1] < v[index + 1]) {
                v[index + 1]
            } else {
                v[index - 1] + 1
            };
            let mut y = (x as isize - k) as usize;
            while x < n && y < m && a[x] == b[y] {
                x += 1;
                y += 1;
                budget = budget.checked_sub(1)?;
            }
            v[index] = x;
            if x >= n && y >= m {
                return Some(d as usize);
            }
        }
    }
    Some(max)
}

// ---------------------------------------------------------------------------------------------
// 文件改动（R5–R7）
// ---------------------------------------------------------------------------------------------

/// 从 `tool.call.*` 的 view 派生的一项文件改动。
pub(crate) struct DerivedFileChange {
    /// 携带该 Diff 元素的工具调用标识（去重键的一半）。
    pub(crate) tool_call_id: String,
    /// `file.changed.kind`：`added` / `deleted` / `modified`。
    pub(crate) kind: &'static str,
    /// `displayPath`：相对会话工作目录根（越界时只给文件名称）。
    pub(crate) display_path: String,
    /// 是否位于工作目录根之外。
    pub(crate) outside: bool,
    /// 行数统计；`None` = 判定不出（省略两项）。
    pub(crate) stats: Option<(u64, u64)>,
    /// 工具调用给出的**原始 `path` 原文**（`display_path` 相对化/越界收窄**之前**的取值）。
    ///
    /// **不参与投影**：任何 view 字段都不读它。它只作「同一处改动」的稳定标识——展示路径是有损的
    /// （越界时只留 `file_name()`），两个不同目录下的同名越界文件会退化出同一个展示值；去重键与
    /// `changeId` 若取展示路径就会把它们当成同一处改动而吞掉一条。R5 要求「同一处改动只派生一次」，
    /// 判据必须比展示值更精细（`design.md` D5）。
    pub(crate) origin_key: String,
}

impl DerivedFileChange {
    /// 去重键（`toolCallId` + **原始路径原文**）。展示路径经过相对化/越界收窄，是有损的，不能用来判定
    /// 「是不是同一处改动」；工具调用给出的原始路径对同一处改动稳定，且能区分同名但不同目录的文件。
    pub(crate) fn dedup_key(&self) -> String {
        format!("{}\u{1}{}", self.tool_call_id, self.origin_key)
    }

    /// `summary`：摘要字段（`file.changed` 的必填字段）。只描述本次改动本身，不携带 Free-form 原文。
    pub(crate) fn summary(&self) -> String {
        format!("{} {}", self.kind, self.display_path)
    }

    /// 派生的 `changeId`。
    ///
    /// 取 `SHA-256(会话 + toolCallId + **原始路径原文**)` 的前 16 字节并置 UUIDv4 的版本/变体位：
    /// **确定性**且形状合法（`common.schema.json#/$defs/uuid` 只要求规范小写 UUID 文本）。去重日志
    /// 使得同一个 `(工具调用, 路径)` 只派生一条事件，因此确定性不会带来重复的 `changeId`；反过来，它让
    /// 「同一处改动」在任何重放/重试下都得到同一个 id。取原始路径而非展示路径：展示路径越界时被收窄为
    /// `file_name()`，两个不同目录的同名文件会得到同一个 `changeId`（与去重键同一缺陷）。
    pub(crate) fn change_id(&self, session: &str) -> String {
        canonical_uuid(&format!(
            "{session}\u{1}{}\u{1}{}",
            self.tool_call_id, self.origin_key
        ))
    }
}

/// 从一个 `tool.call.*` 事件的 view 派生文件改动（R5–R7）。
///
/// 返回的每一项恰好对应 view 的 `diff` 数组里的一个**类型化 Diff 元素**。`diff` 键缺席（或不是数组）
/// 时返回空 `Vec`——本函数绝不从 `rawInput`/`title` 等自由形状字段推断文件改动。
///
/// 跳过（不派生、不报错）的三种情形，都属「该元素没有可用的类型化取值」：
/// - view 没有 `toolCallId`（去重键不成立；`§10.3` 要求 `tool.call.*` 必带它，缺失即适配器缺陷）；
/// - `diff` 元素不是 object，或 `path` 缺失/不是字符串（无法相对化，也就无法给出展示路径）；
/// - `oldText`/`newText` 出现了非字符串、非 null 的取值（形状不可判定）。
///
/// `workspace_root` 是会话持久化的规范化工作目录（`owned_session.workspace_cwd`）；它为 `None` 时按
/// 「无法证明路径在工作区内」处理（越界 + 只给文件名称），因此绝不静默下发绝对路径。
pub(crate) fn derived_file_changes(
    view: &str,
    workspace_root: Option<&str>,
) -> Result<Vec<DerivedFileChange>, InvalidValue> {
    let tool_call_id = match string_member(view, "toolCallId") {
        Ok(Some(id)) => id,
        // 形状不符（非字符串）或缺席：无法保证「同一处改动只派生一次」，跳过而不是猜。
        _ => return Ok(Vec::new()),
    };
    let Some(raw) = member_raw(view, "diff")? else {
        return Ok(Vec::new());
    };
    // 适配器契约是数组（恒为数组，长度 ≥ 1）；单个 object 也接受，不因此拒绝整次调用。
    let elements = array_items(raw).unwrap_or_else(|| vec![raw]);
    derive_from_elements(&tool_call_id, &elements, workspace_root)
}

fn derive_from_elements(
    tool_call_id: &str,
    elements: &[&str],
    workspace_root: Option<&str>,
) -> Result<Vec<DerivedFileChange>, InvalidValue> {
    let mut derived = Vec::new();
    for element in elements {
        let Ok(members) = json_object_members(element) else {
            continue;
        };
        let Some(path) = optional_string(&members, "path")? else {
            continue;
        };
        let old_text = optional_string(&members, "oldText")?;
        let new_text = optional_string(&members, "newText")?;
        let (display_path, outside) = display_path(workspace_root, &path);
        // R6 的三种口径：`oldText` 缺席 = 新建（全部行计新增、删除为零）；`newText` 缺席 = 判定不出行数
        // （**省略**两项，绝不填零）；两者都在时才跑行级 diff。
        let stats = match (old_text.as_deref(), new_text.as_deref()) {
            (None, Some(new)) => line_stats(None, Some(new)),
            (Some(old), Some(new)) => line_stats(Some(old), Some(new)),
            (_, None) => None,
        };
        let kind = match (old_text.is_some(), new_text.is_some()) {
            (false, _) => "added",
            // `oldText` 在、`newText` 缺席：工具调用没有可用的改动后内容，不猜改动方向。
            (true, false) => "modified",
            (true, true) if new_text.as_deref() == Some("") => "deleted",
            (true, true) => "modified",
        };
        derived.push(DerivedFileChange {
            tool_call_id: tool_call_id.to_owned(),
            kind,
            display_path,
            outside,
            stats,
            // 去重键与 `changeId` 取工具调用给出的原始路径：展示路径在越界时被收窄为文件名，是有损的
            // （两个不同目录下的同名越界文件会碰撞，后者被静默当成重复丢掉）。
            origin_key: path,
        });
    }
    Ok(derived)
}

/// object 成员里的可选字符串：缺席 → `None`，字符串 → `Some`，其它取值 → `InvalidValue`（调用方跳过该元素）。
fn optional_string(members: &[(String, &str)], key: &str) -> Result<Option<String>, InvalidValue> {
    match members.iter().find(|(name, _)| name == key) {
        None => Ok(None),
        Some((_, raw)) => {
            if raw.trim() == "null" {
                return Ok(None);
            }
            decode_json_string(raw).map(Some).ok_or(InvalidValue::Field)
        }
    }
}

/// 事件 view 里某个成员解码后的字符串取值：缺席与显式 `null` 都是 `None`，非字符串是形状错误。
fn string_member(text: &str, key: &str) -> Result<Option<String>, InvalidValue> {
    match member_raw(text, key)? {
        None => Ok(None),
        Some(raw) => {
            if raw.trim() == "null" {
                return Ok(None);
            }
            decode_json_string(raw).map(Some).ok_or(InvalidValue::Field)
        }
    }
}

/// 事件 view 的单个成员原文（不解码）。
fn member_raw<'a>(text: &'a str, key: &str) -> Result<Option<&'a str>, InvalidValue> {
    Ok(json_object_members(text)?
        .into_iter()
        .find(|(name, _)| name == key)
        .map(|(_, raw)| raw))
}

// ---------------------------------------------------------------------------------------------
// 展示路径（R7 / `workspace-resolution`）
// ---------------------------------------------------------------------------------------------

/// 展示路径：`(相对化后的展示值, 是否位于工作区之外)`。
///
/// - 报告路径是**绝对路径**且规范化后位于工作目录根之下 → 下发相对形式（`/` 分隔，不含根的任何片段、
///   不含回退层级）；
/// - 不在其下（含无法证实） → 只下发该文件的名称，且调用方置 `outsideWorkspace = true`；
/// - 报告路径是**相对路径** → 按「相对该工作目录根」解释（ACP 上游是绝对路径，但适配器可能给出相对
///   形式）；规范化后仍带回退层级（逃出根）时同样按越界处理。
///
/// 前缀关系用**规范化后**的组件比较（`Path::starts_with` + `strip_prefix`），因此字符串前缀相同但不在
/// 其下的情况（`/work/api` 与 `/work/api-tools`）不会被误判为工作区内。
pub(crate) fn display_path(workspace_root: Option<&str>, reported: &str) -> (String, bool) {
    let reported_path = Path::new(reported);
    if !reported_path.is_absolute() {
        let normalized = normalize(reported_path);
        if normalized.as_os_str().is_empty()
            || matches!(normalized.components().next(), Some(Component::ParentDir))
        {
            return outside_display(reported_path);
        }
        // 相对形式按「相对该会话工作目录根」解释（ACP 上游是绝对路径，适配器也可能给出相对形式）；
        // 无法与工作目录根比较（根未登记）时同样从严，按越界处理而不是默认信任。
        match workspace_root.map(|root| canonical_path(Path::new(root))) {
            Some(root) if !root.as_os_str().is_empty() => {
                let candidate = normalize(&root.join(&normalized));
                match candidate.strip_prefix(&root) {
                    Ok(rest) => match relative_text(rest) {
                        Some(text) => (text, false),
                        None => outside_display(reported_path),
                    },
                    Err(_) => outside_display(reported_path),
                }
            }
            _ => outside_display(reported_path),
        }
    } else {
        let Some(root) = workspace_root else {
            return outside_display(reported_path);
        };
        let root_path = canonical_path(Path::new(root));
        let reported_canonical = canonical_path(reported_path);
        if reported_canonical == root_path {
            // 「改动对象就是工作目录根本身」不是文件改动，且不能下发根的任何片段。
            return (String::from("."), true);
        }
        match reported_canonical.strip_prefix(&root_path) {
            Ok(rest) => match relative_text(rest) {
                Some(text) => (text, false),
                None => outside_display(reported_path),
            },
            Err(_) => outside_display(reported_path),
        }
    }
}

/// 越界路径的展示值：只给文件名称（不含任何目录片段、不含回退层级）。
fn outside_display(path: &Path) -> (String, bool) {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty());
    (name.unwrap_or_else(|| String::from(".")), true)
}

/// 组件 → `/` 分隔的相对路径文本；没有可用组件时返回 `None`。
fn relative_text(path: &Path) -> Option<String> {
    let mut out = String::new();
    for component in path.components() {
        let Component::Normal(part) = component else {
            continue;
        };
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(&part.to_string_lossy());
    }
    if out.is_empty() { None } else { Some(out) }
}

/// 词法规范化：去掉 `.`、处理 `..`（越出根部时保留前导 `..`）、合并重复分隔符。
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => out.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            Component::Normal(part) => out.push(part),
        }
    }
    out
}

/// 规范化一个**可能尚不存在**的路径：优先 `canonicalize`（解析 symlink/junction/大小写），失败时逐级
/// 上溯到第一个存在的祖先、规范化它之后把剩余组件接回；整条路径都不存在时退回纯词法规范化。
///
/// 上溯这一步是必要的：Agent 报出的新文件路径在磁盘上还不存在（`canonicalize` 会失败），而只做词法
/// 规范化会让它与根系（已 canonical 化）在 symlink/大小写/verbatim 前缀上不可比较，从而被误判为越界。
fn canonical_path(path: &Path) -> PathBuf {
    if let Ok(resolved) = std::fs::canonicalize(path) {
        return strip_verbatim(resolved);
    }
    let normalized = normalize(path);
    let mut tail: Vec<OsString> = Vec::new();
    let mut cursor: &Path = &normalized;
    while let Some(parent) = cursor.parent() {
        if let Some(name) = cursor.file_name() {
            tail.push(name.to_os_string());
        }
        match std::fs::canonicalize(parent) {
            Ok(resolved) => {
                let mut out = strip_verbatim(resolved);
                for part in tail.iter().rev() {
                    out.push(part);
                }
                return out;
            }
            Err(_) => cursor = parent,
        }
    }
    normalized
}

/// Windows 的 `canonicalize` 会带 verbatim 前缀（`\\?\`、`\\?\UNC\`），把它剥掉使「已 canonical 化的
/// 根」与「词法规范化的报告路径」可比较。
#[cfg(windows)]
fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path,
    }
}

#[cfg(not(windows))]
fn strip_verbatim(path: PathBuf) -> PathBuf {
    path
}

/// 确定性 canonical UUID（见 [`DerivedFileChange::change_id`]）。
fn canonical_uuid(seed: &str) -> String {
    let digest = Sha256::digest(seed.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let mut out = String::with_capacity(36);
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            out.push('-');
        }
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

// ---------------------------------------------------------------------------------------------
// 事件 view 组装（`file.changed` 与节点级 `agent.*`）
// ---------------------------------------------------------------------------------------------

/// 派生事件的 view 文本（`file.changed`，`SYNC_PROTOCOL.md` §10.3）。
///
/// 只写合同登记的字段：`addedLines`/`deletedLines` 在 `stats` 为 `None` 时**整个键缺席**（省略而非零），
/// `outsideWorkspace` 只在越界时为 `true`（工作区内时省略，与既有 fixture 的最小集一致）。
pub(crate) fn file_changed_view(change: &DerivedFileChange, session: &str) -> String {
    let mut out = String::from("{\"changeId\":");
    out.push_str(&encode_json_string(&change.change_id(session)));
    out.push_str(",\"kind\":");
    out.push_str(&encode_json_string(change.kind));
    out.push_str(",\"displayPath\":");
    out.push_str(&encode_json_string(&change.display_path));
    out.push_str(",\"summary\":");
    out.push_str(&encode_json_string(&change.summary()));
    if let Some((added, deleted)) = change.stats {
        out.push_str(",\"addedLines\":\"");
        out.push_str(&added.to_string());
        out.push_str("\",\"deletedLines\":\"");
        out.push_str(&deleted.to_string());
        out.push('"');
    }
    if change.outside {
        out.push_str(",\"outsideWorkspace\":true");
    }
    out.push('}');
    out
}

/// `file.changed` 的派生入口事件类型（R5）：适配器的 `tool_call`/`tool_call_update` 投影
/// （`SYNC_PROTOCOL.md` §10.3 的判别子映射）。终态额外生成的 `tool.call.completed` 也带内容。
pub(crate) fn file_change_event(event_type: &str) -> bool {
    matches!(
        event_type,
        "tool.call.started" | "tool.call.updated" | "tool.call.completed"
    )
}

/// 节点级事件 view 的读取面：`(agentId, state)`；任一缺失或不是字符串都为 `None`。
pub(crate) fn node_event_parts(view: &str) -> (Option<String>, Option<String>) {
    (
        string_member(view, "agentId").ok().flatten(),
        string_member(view, "state").ok().flatten(),
    )
}

/// view 里是否存在**类型化 Diff 元素**（R5 的唯一派生判据）：`diff` 键存在、是数组且至少有一个元素
/// （或直接是单个 object）。
///
/// 只说「有没有 Diff 元素」，因此决定是否要为此做一次会话窄读取；具体的逐元素派生见
/// [`derived_file_changes`]。
pub(crate) fn view_carries_diff(view: &str) -> Result<bool, InvalidValue> {
    let Some(raw) = member_raw(view, "diff")? else {
        return Ok(false);
    };
    if raw.trim_start().starts_with('{') {
        return Ok(true);
    }
    Ok(array_items(raw).is_some_and(|items| !items.is_empty()))
}

// ---------------------------------------------------------------------------------------------
// 会话标题（R9 / D7）
// ---------------------------------------------------------------------------------------------

/// `session.info.changed` 携带的标题意图（**两层可选**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TitleIntent {
    /// 通知没有携带标题（只携带更新时间）：既有标题保持不变。
    Unchanged,
    /// 通知显式把标题置空：写入空值（客户端呈现为未命名会话）。
    Clear,
    /// 写入该标题。
    Set(String),
}

/// 从 `session.info.changed` 读出标题意图。
///
/// 权威来源是 **ACP 原文**（`payload.acp.rawJson` 的 `params.update.title`）：ACP 的 `SessionInfoUpdate`
/// 所有字段都是可选的（支持部分更新），因此只有原始文本才能区分「键缺席」（不改）与「显式 null」
/// （清空）——公共 view 的 `title` 是 §10.3 的 required 字段（`string|null`），键永远存在，表达不了这个
/// 区别。
///
/// ACP 原文不可用（缺失/不可解析/结构不符）时退回 view 的 `title`：`null` 按「只有更新时间」处理
/// （保持既有标题不变），字符串则照写。这条降级路径不会把「不改」误判成「清空」。
///
/// 标题超过 [`TITLE_MAX_CHARS`] 时返回 [`InvalidValue::Field`]：那是适配器投影缺陷，失败关闭而不是把
/// 一个违反值对象不变量的标题写进会话行。
pub(crate) fn title_intent(
    acp_json: Option<&str>,
    view: &str,
) -> Result<TitleIntent, InvalidValue> {
    if let Some(raw) = acp_json {
        if let Some(update) = acp_update_members(raw) {
            return match update.iter().find(|(name, _)| name == "title") {
                Some((_, value)) => title_from_raw(value, true),
                None => Ok(TitleIntent::Unchanged),
            };
        }
    }
    match member_raw(view, "title")? {
        // 降级路径：公共 view 的 `title` 是 §10.3 的 required 字段（`string|null`），键永远存在，
        // 因此这里的 `null` 表达不了「显式清空 vs 只有更新时间」。按最保守的解释处理——保持既有标题
        // 不变——绝不把「不改」误判成「清空」。
        Some(raw) => title_from_raw(raw, false),
        None => Ok(TitleIntent::Unchanged),
    }
}

/// `clear_is_explicit` 为真时 `null` 是「显式清空」（ACP 原文里的键存在且为 `null`）；为假时 `null`
/// 按「键缺席」处理（降级路径，见 [`title_intent`]）。
fn title_from_raw(raw: &str, clear_is_explicit: bool) -> Result<TitleIntent, InvalidValue> {
    if raw.trim() == "null" {
        return Ok(if clear_is_explicit {
            TitleIntent::Clear
        } else {
            TitleIntent::Unchanged
        });
    }
    let text = decode_json_string(raw).ok_or(InvalidValue::Field)?;
    if text.chars().count() > TITLE_MAX_CHARS {
        return Err(InvalidValue::Field);
    }
    Ok(TitleIntent::Set(text))
}

/// 走 `session/update` 通知的 ACP 原文：root → `params` → `update` 的成员；任一层不符即 `None`。
fn acp_update_members(raw: &str) -> Option<Vec<(String, &str)>> {
    let root = json_object_members(raw).ok()?;
    let params = root.iter().find(|(name, _)| name == "params")?.1;
    let params = json_object_members(params).ok()?;
    let update = params.iter().find(|(name, _)| name == "update")?.1;
    json_object_members(update).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 暴力 LCS 长度（DP，`O(N·M)`），只用于给 Myers 距离做交叉验证。
    fn brute_force_distance(a: &[&str], b: &[&str]) -> usize {
        let n = a.len();
        let m = b.len();
        let mut table = vec![vec![0_usize; m + 1]; n + 1];
        for i in 0..n {
            for j in 0..m {
                table[i + 1][j + 1] = if a[i] == b[j] {
                    table[i][j] + 1
                } else {
                    table[i][j + 1].max(table[i + 1][j])
                };
            }
        }
        n + m - 2 * table[n][m]
    }

    /// 全部「长度 ≤ 3、字母表大小 2」的行序列对（穷举，含空序列）。
    fn tiny_sequences() -> Vec<Vec<&'static str>> {
        const WORDS: [&str; 2] = ["a", "b"];
        let mut out = vec![Vec::new()];
        for _ in 0..3 {
            let mut next = Vec::new();
            for sequence in &out {
                for word in WORDS {
                    let mut extended = sequence.clone();
                    extended.push(word);
                    next.push(extended);
                }
            }
            out.extend(next);
        }
        out
    }

    /// 确定性伪随机数（不引入依赖，也不依赖系统随机源）。
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self, bound: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            ((self.0 >> 33) as usize) % bound
        }
    }

    #[test]
    fn myers_distance_matches_brute_force_on_all_tiny_pairs() {
        let sequences = tiny_sequences();
        for a in &sequences {
            for b in &sequences {
                let expected = brute_force_distance(a, b);
                let actual = myers_distance(a, b).expect("预算足够");
                assert_eq!(actual, expected, "a={a:?} b={b:?}");
            }
        }
    }

    #[test]
    fn myers_distance_matches_brute_force_on_random_pairs() {
        let mut rng = Lcg(0x5eed_1234_abcd_9f01);
        let alphabet = ["alpha", "beta", "gamma", "delta", "epsilon"];
        for _ in 0..400 {
            let n = rng.next(24);
            let m = rng.next(24);
            let a: Vec<&str> = (0..n).map(|_| alphabet[rng.next(alphabet.len())]).collect();
            let b: Vec<&str> = (0..m).map(|_| alphabet[rng.next(alphabet.len())]).collect();
            let expected = brute_force_distance(&a, &b);
            let actual = myers_distance(&a, &b).expect("预算足够");
            assert_eq!(actual, expected, "a={a:?} b={b:?}");
        }
    }

    #[test]
    fn line_stats_counts_replacement_when_line_counts_are_equal() {
        // R6 场景 1：行数相等但内容不同 → 必须报告非零改动（行数差会得到 0）。
        let (added, deleted) = line_stats(Some("a\nb\nc"), Some("a\nX\nc")).expect("判定得出");
        assert_eq!((added, deleted), (1, 1));
    }

    #[test]
    fn line_stats_treats_missing_old_text_as_a_new_file() {
        // R6 场景 2：新建文件的全部行计为新增、删除为零。
        let (added, deleted) = line_stats(None, Some("a\nb\nc\n")).expect("判定得出");
        assert_eq!((added, deleted), (3, 0));
    }

    #[test]
    fn line_stats_omits_when_new_text_is_unavailable() {
        // R6 场景 3：判定不出时省略（返回 `None`），绝不填零。
        assert_eq!(line_stats(Some("a\nb"), None), None);
    }

    #[test]
    fn line_stats_is_zero_for_identical_text() {
        assert_eq!(line_stats(Some("a\nb"), Some("a\nb")), Some((0, 0)));
    }

    #[test]
    fn line_stats_counts_deletion_of_every_line() {
        assert_eq!(line_stats(Some("a\nb"), Some("")), Some((0, 2)));
    }

    #[test]
    fn display_path_relativizes_inside_the_workspace() {
        let root = std::env::temp_dir();
        let root_text = root.to_string_lossy().into_owned();
        let inside = root.join("sub").join("file.txt");
        let (display, outside) = display_path(Some(&root_text), &inside.to_string_lossy());
        assert!(!outside);
        assert_eq!(display, "sub/file.txt");
    }

    #[test]
    fn display_path_marks_outside_and_keeps_only_the_file_name() {
        let (display, outside) = display_path(Some("/work/api"), "/etc/passwd");
        assert!(outside);
        assert_eq!(display, "passwd");
        assert!(!display.contains(".."), "不得下发回退层级");
        assert!(!display.contains("api"), "不得下发工作目录根的片段");
    }

    #[test]
    fn display_path_does_not_treat_a_string_prefix_as_inside() {
        // R7 场景 3：`/work/api` 与 `/work/api-tools` 字符串前缀相同但不构成目录关系。
        let (display, outside) = display_path(Some("/work/api"), "/work/api-tools/file.txt");
        assert!(outside);
        assert_eq!(display, "file.txt");
    }

    #[test]
    fn display_path_without_a_workspace_root_is_outside() {
        let (display, outside) = display_path(None, "/anywhere/file.txt");
        assert!(outside);
        assert_eq!(display, "file.txt");
    }

    #[test]
    fn display_path_escapes_a_relative_parent_traversal() {
        let (display, outside) = display_path(Some("/work/api"), "../etc/passwd");
        assert!(outside);
        assert_eq!(display, "passwd");
    }

    #[test]
    fn display_path_accepts_a_relative_form_as_workspace_relative() {
        let (display, outside) = display_path(Some("/work/api"), "src/main.rs");
        assert!(!outside);
        assert_eq!(display, "src/main.rs");
    }

    #[test]
    fn canonical_uuid_has_the_declared_shape_and_is_deterministic() {
        let first = canonical_uuid("seed");
        let second = canonical_uuid("seed");
        assert_eq!(first, second);
        assert_eq!(first.len(), 36);
        assert_eq!(first.as_bytes()[14], b'4', "版本位必须是 4");
        assert!(matches!(first.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
        assert!(first.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-'));
    }

    #[test]
    fn derived_changes_only_come_from_typed_diff_elements() {
        let view = r#"{"toolCallId":"tool-1","title":"编辑","state":"in_progress","rawInput":{"path":"/work/api/secret.txt","newText":"x"}}"#;
        assert!(
            derived_file_changes(view, Some("/work/api"))
                .expect("可解析")
                .is_empty(),
            "没有类型化 Diff 元素时不得从 rawInput 推断文件改动"
        );
    }

    #[test]
    fn derived_changes_read_the_typed_diff_element() {
        let view = r#"{"toolCallId":"tool-1","diff":[{"path":"src/main.rs","oldText":"a","newText":"b"}]}"#;
        let derived = derived_file_changes(view, Some("/work/api")).expect("可解析");
        assert_eq!(derived.len(), 1);
        assert_eq!(derived[0].kind, "modified");
        assert_eq!(derived[0].display_path, "src/main.rs");
        assert_eq!(derived[0].stats, Some((1, 1)));
        assert!(!derived[0].outside);
    }

    #[test]
    fn derived_changes_omit_stats_without_new_text() {
        let view = r#"{"toolCallId":"tool-1","diff":[{"path":"src/main.rs","oldText":"a"}]}"#;
        let derived = derived_file_changes(view, None).expect("可解析");
        assert_eq!(derived.len(), 1);
        assert_eq!(derived[0].stats, None, "判定不出时省略而非填零");
        assert_eq!(derived[0].kind, "modified", "没有改动后内容时不猜改动方向");
    }

    #[test]
    fn derived_changes_report_a_new_file_as_added() {
        let view = r#"{"toolCallId":"tool-1","diff":[{"path":"src/new.rs","newText":"a\nb"}]}"#;
        let derived = derived_file_changes(view, None).expect("可解析");
        assert_eq!(derived.len(), 1);
        assert_eq!(derived[0].kind, "added");
        assert_eq!(derived[0].stats, Some((2, 0)));
    }

    #[test]
    fn derived_changes_report_an_emptied_file_as_deleted() {
        let view = r#"{"toolCallId":"tool-1","diff":[{"path":"src/gone.rs","oldText":"a\nb","newText":""}]}"#;
        let derived = derived_file_changes(view, None).expect("可解析");
        assert_eq!(derived.len(), 1);
        assert_eq!(derived[0].kind, "deleted");
        assert_eq!(derived[0].stats, Some((0, 2)));
    }

    #[test]
    fn derived_changes_skip_elements_without_a_path() {
        let view = r#"{"toolCallId":"tool-1","diff":[{"oldText":"a","newText":"b"},{"path":"keep.txt","newText":"b"}]}"#;
        let derived = derived_file_changes(view, None).expect("可解析");
        assert_eq!(derived.len(), 1);
        assert_eq!(derived[0].display_path, "keep.txt");
    }

    #[test]
    fn derived_changes_skip_when_tool_call_id_is_missing() {
        let view = r#"{"diff":[{"path":"a.txt","newText":"b"}]}"#;
        assert!(
            derived_file_changes(view, None).expect("可解析").is_empty(),
            "没有 toolCallId 就无法保证不重复派生"
        );
    }

    /// F2（`review-w3-r1`）：两个**同名但不同目录**的越界文件是两处改动。
    ///
    /// 展示路径在越界时按合同收窄为 `file_name()`（R7），因此两者的 `display_path` 相同；若去重键
    /// 与 `changeId` 取展示路径就会碰撞，第二条被当成「同一处改动」而静默丢掉。去重键与 `changeId`
    /// 必须取工具调用给出的**原始路径**（`origin_key`，不参与投影）。
    #[test]
    fn derived_changes_distinguish_same_named_outside_files() {
        let view = r#"{"toolCallId":"tool-1","diff":[{"path":"/etc/nginx/nginx.conf","oldText":"a","newText":"b"},{"path":"/tmp/nginx.conf","oldText":"c","newText":"d"}]}"#;
        let derived = derived_file_changes(view, Some("/work/api")).expect("可解析");
        assert_eq!(derived.len(), 2, "两处改动都要派生");
        assert_eq!(derived[0].display_path, "nginx.conf");
        assert_eq!(derived[1].display_path, "nginx.conf");
        assert!(derived[0].outside && derived[1].outside);
        assert_eq!(derived[0].origin_key, "/etc/nginx/nginx.conf");
        assert_eq!(derived[1].origin_key, "/tmp/nginx.conf");
        assert_ne!(
            derived[0].dedup_key(),
            derived[1].dedup_key(),
            "同名越界文件不得共用去重键"
        );
        assert_ne!(
            derived[0].change_id("session-1"),
            derived[1].change_id("session-1"),
            "同名越界文件的 changeId 必须互不相同"
        );
        // 展示路径仍然只给文件名（R7 的输出不因去重键的改动而变化）。
        let view_text = file_changed_view(&derived[1], "session-1");
        assert!(view_text.contains(r#""displayPath":"nginx.conf""#));
        assert!(
            !view_text.contains("origin_key") && !view_text.contains("/tmp/"),
            "原始路径不参与投影：{view_text}"
        );
    }

    #[test]
    fn title_intent_distinguishes_absent_null_and_text() {
        let absent = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"session_info_update","updatedAt":"2026-10-04T00:00:00.000Z"}}}"#;
        assert_eq!(
            title_intent(
                Some(absent),
                r#"{"title":null,"updatedAt":"2026-10-04T00:00:00.000Z"}"#
            )
            .expect("可解析"),
            TitleIntent::Unchanged
        );
        let explicit_null =
            r#"{"params":{"update":{"sessionUpdate":"session_info_update","title":null}}}"#;
        assert_eq!(
            title_intent(Some(explicit_null), r#"{"title":null}"#).expect("可解析"),
            TitleIntent::Clear
        );
        let text =
            r#"{"params":{"update":{"sessionUpdate":"session_info_update","title":"标题"}}}"#;
        assert_eq!(
            title_intent(Some(text), r#"{"title":"标题"}"#).expect("可解析"),
            TitleIntent::Set("标题".to_owned())
        );
    }

    #[test]
    fn title_intent_falls_back_to_the_view_when_the_acp_document_is_unusable() {
        assert_eq!(
            title_intent(
                None,
                r#"{"title":null,"updatedAt":"2026-10-04T00:00:00.000Z"}"#
            )
            .expect("可解析"),
            TitleIntent::Unchanged
        );
        assert_eq!(
            title_intent(Some("not json"), r#"{"title":"T"}"#).expect("可解析"),
            TitleIntent::Set("T".to_owned())
        );
    }

    #[test]
    fn title_intent_rejects_an_over_long_title() {
        let long = "x".repeat(TITLE_MAX_CHARS + 1);
        let raw = format!(r#"{{"params":{{"update":{{"title":"{long}"}}}}}}"#);
        assert!(
            title_intent(Some(&raw), r#"{"title":null}"#).is_err(),
            "超过 512 字符的标题必须失败关闭"
        );
    }
}

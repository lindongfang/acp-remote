//! core 的派生投影（`docs/SYNC_PROTOCOL.md` §10.3 的三类「合同已登记、core 侧无生产者」的事件）。
//!
//! 本模块只做**纯函数**投影（不读时钟、不落盘、不发布），由 [`crate::broker`] 在组装提交时调用：
//!
//! - `file.changed`（R5–R7）：派生源**只有** ACP 工具调用内容里的类型化 Diff 元素（`design.md` D4/D5）。
//!   判定完全依据 `view` 的 `diff` 键——那个键由适配器（`agent-host`）从 `ToolCallContent::Diff` 逐字节
//!   投影而来；本模块**不读** `rawInput` 等自由形状字段，也不猜 Agent 的意图。
//! - 行数统计：对 `oldText`/`newText` 跑行级 Myers diff（O(ND)、线性空间），判定不出时**省略**两项而不是
//!   填零。行数差是错的且错得隐蔽（10 行换 10 行也可能是删 2 加 2），因此这里绝不使用行数差。
//! - 结构化差异行（R10、feature `core.file-diff.v1`）：同一份行级 diff 的**逐行**对齐（新增/删除/上下文），
//!   受 §14 的固定行数/字节/单行上限约束；超限时只带统计并把差异行标记为 `size_limit` 未下发，绝不裁剪
//!   行，也绝不改写 ACP 原文的字节、摘要与长度。
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

/// 单处改动的**差异行数**上限（`SYNC_PROTOCOL.md` §14 的固定 v1 常量，`event-views.schema.json` 的
/// `diffLines.maxItems`）。
const DIFF_LINE_LIMIT: usize = 400;

/// 单处改动的**差异行合计文本**上限（§14：64 KiB，按 UTF-8 字节计）。
const DIFF_BYTE_LIMIT: usize = 64 * 1024;

/// **单行**差异行文本的字符上限（§14 与 `event-views.schema.json` 的 `text.maxLength`）。
const DIFF_LINE_MAX_CHARS: usize = 4096;

// ---------------------------------------------------------------------------------------------
// 行级差异（R6 统计 / R10 结构化差异行）
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
// 结构化差异行（R10 / feature `core.file-diff.v1`）
// ---------------------------------------------------------------------------------------------

/// `file.changed.diffLines` 的一行：判别子（`added`/`deleted`/`context`）+ **不带前缀**的行文本。
///
/// 文本不带 `+ `/`- `/`  ` 前缀：前缀是客户端的呈现选择，逐行下发结构化数据才能让客户端自己决定配色与
/// 前缀，而不必解析任何拼好的文本块（`SYNC_PROTOCOL.md` §10.3）。
pub(crate) struct DiffLine {
    /// `diffLines[].kind`：封闭词表。
    pub(crate) kind: &'static str,
    /// `diffLines[].text`：该行的原文（不含行尾换行）。
    pub(crate) text: String,
}

/// 一处改动的行级差异派生结果（R6 的统计与 R10 的差异行**同源**：来自同一次行级 diff）。
///
/// 三态而不是「统计 + 可选的差异行」两个独立字段：`Omitted` 是唯一会同时丢掉统计与差异行的状态，其余
/// 两态都带统计，因此不可能出现「有差异行却没有统计」或「统计与逐行行数不一致」的形状。
pub(crate) enum LineDiff {
    /// 判定不出（`newText` 缺席，或行级 diff 超出工作预算）：统计与差异行**都**省略，绝不填零。
    Omitted,
    /// 判定得出但差异行超出 §14 的固定上限：只保留统计，差异行改由 `diffUnavailable` 标记未下发。
    OverLimit { added: u64, deleted: u64 },
    /// 上限内：统计 + 完整的逐行差异（未做任何裁剪）。
    Complete {
        added: u64,
        deleted: u64,
        lines: Vec<DiffLine>,
    },
}

impl LineDiff {
    /// `(addedLines, deletedLines)`；`None` = 判定不出（view 里两项**整个键缺席**）。
    pub(crate) fn stats(&self) -> Option<(u64, u64)> {
        match self {
            LineDiff::Omitted => None,
            LineDiff::OverLimit { added, deleted } | LineDiff::Complete { added, deleted, .. } => {
                Some((*added, *deleted))
            }
        }
    }

    /// 上限内的完整逐行差异；`Omitted` 与 `OverLimit` 都是 `None`（前者没有统计、后者只带统计）。
    pub(crate) fn lines(&self) -> Option<&[DiffLine]> {
        match self {
            LineDiff::Complete { lines, .. } => Some(lines),
            LineDiff::Omitted | LineDiff::OverLimit { .. } => None,
        }
    }

    /// 判定得出但差异行未随本次派生下发（只带统计）时为真——view 据此写 `diffUnavailable`。
    pub(crate) fn over_limit(&self) -> bool {
        matches!(self, LineDiff::OverLimit { .. })
    }
}

/// 从同一份 `oldText`/`newText` 派生行级统计与结构化差异行（R6 + R10）。
///
/// 统计口径**逐条**复用 [`line_stats`]（同一份权威实现，本函数不重算 Myers 距离），因此不可能出现
/// 「统计说改了 3 行、逐行却给了别的行数」：逐行差异由 LCS 对齐得出，而 LCS 长度与 Myers 距离互为
/// 定值（`added = M - LCS`、`deleted = N - LCS`）。
///
/// 上限（§14 的固定 v1 常量）在**建脚本之前**用输入判定，因此病态输入不会先付出 `O(N·M)` 的代价：
/// 完整逐行差异把每个旧行、每个新行各输出一次，所以元素个数 ≥ `max(N, M)`、文本字节数恰好等于两侧
/// 行文本字节数之和、单行长度不变。超限时**不裁剪**，整份差异行改为 [`LineDiff::OverLimit`]。
pub(crate) fn line_diff(old: Option<&str>, new: Option<&str>) -> LineDiff {
    let Some((added, deleted)) = line_stats(old, new) else {
        return LineDiff::Omitted;
    };
    // `line_stats` 判定得出 ⇒ `new` 必定在场（它把 `new` 缺失映射成 `None`）。
    let new_lines = split_lines(new.unwrap_or_default());
    let old_lines = old.map(split_lines).unwrap_or_default();
    if old_lines.len().max(new_lines.len()) > DIFF_LINE_LIMIT
        || line_bytes(&old_lines) + line_bytes(&new_lines) > DIFF_BYTE_LIMIT
        || old_lines
            .iter()
            .chain(new_lines.iter())
            .any(|line| line.chars().count() > DIFF_LINE_MAX_CHARS)
    {
        return LineDiff::OverLimit { added, deleted };
    }
    let lines = align_lines(&old_lines, &new_lines);
    // 行数上限只能在这里判定：`n + m - LCS` 可能大于 `max(n, m)`（两侧完全不同的 400 行文件会得出
    // 800 行差异）。字节上限不必再查——上面的先验判定是**恰好**的（每个输入行在输出里出现一次）。
    if lines.len() > DIFF_LINE_LIMIT {
        return LineDiff::OverLimit { added, deleted };
    }
    LineDiff::Complete {
        added,
        deleted,
        lines,
    }
}

/// 一组行的文本字节数（不含行尾换行：`split_lines` 已经把换行从行文本里去掉）。
fn line_bytes(lines: &[&str]) -> usize {
    lines.iter().map(|line| line.len()).sum()
}

/// 行级对齐：返回**完整**的逐行差异——每个旧行恰好出现一次（`deleted` 或 `context`）、每个新行恰好
/// 出现一次（`added` 或 `context`），顺序与原文一致。
///
/// 用「最长公共子序列」长度表（`O(N·M)` 时间与空间）加一次左上到右下的回溯。调用方保证
/// `max(N, M) <= DIFF_LINE_LIMIT`，因此表规模有界（≤ 401×401）。平局（两侧都能延伸）固定取
/// **先删后增**，使同一份输入永远给出同一份输出。
fn align_lines(old: &[&str], new: &[&str]) -> Vec<DiffLine> {
    let n = old.len();
    let m = new.len();
    let mut table = vec![0_u32; (n + 1) * (m + 1)];
    let index = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[index(i, j)] = if old[i] == new[j] {
                table[index(i + 1, j + 1)] + 1
            } else {
                table[index(i + 1, j)].max(table[index(i, j + 1)])
            };
        }
    }
    let mut lines = Vec::with_capacity(n + m);
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if old[i] == new[j] {
            lines.push(DiffLine {
                kind: "context",
                text: old[i].to_owned(),
            });
            i += 1;
            j += 1;
        } else if table[index(i + 1, j)] >= table[index(i, j + 1)] {
            lines.push(DiffLine {
                kind: "deleted",
                text: old[i].to_owned(),
            });
            i += 1;
        } else {
            lines.push(DiffLine {
                kind: "added",
                text: new[j].to_owned(),
            });
            j += 1;
        }
    }
    for line in &old[i..] {
        lines.push(DiffLine {
            kind: "deleted",
            text: (*line).to_owned(),
        });
    }
    for line in &new[j..] {
        lines.push(DiffLine {
            kind: "added",
            text: (*line).to_owned(),
        });
    }
    lines
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
    /// 行级差异（统计 + 结构化差异行，**同源**）；`Omitted` = 判定不出（统计与差异行都省略）。
    pub(crate) diff: LineDiff,
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
        // R6/R10 的口径只有一处实现（`line_diff`）：`oldText` 缺席 = 新建（全部行计新增、删除为零）；
        // `newText` 缺席 = 判定不出行数（**省略**两项，绝不填零）；两者都在时才跑行级 diff。差异行与
        // 统计由**同一次**调用给出，因此两者的规模不可能分叉。
        let diff = line_diff(old_text.as_deref(), new_text.as_deref());
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
            diff,
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
        // 相对形式按「相对该会话工作目录根」解释（ACP 上游是绝对路径，适配器也可能给出相对形式）。
        // 前导 `..` 逃出根时按越界处理——判据是**显式计数**的逃逸层级，不是缓冲区里的第一个组件：
        // 只做词法折叠会丢掉越出根的那个 `..`（见 `Normalized::escapes`）。
        let normalized = normalize(reported_path);
        if normalized.escapes > 0 {
            return outside_display(reported_path);
        }
        if normalized.path.as_os_str().is_empty() {
            // 规范化后什么都没剩（空串、`.`、`./`）：没有可下发的文件改动。
            return outside_display(reported_path);
        }
        // 无法与工作目录根比较（根未登记）时同样从严，按越界处理而不是默认信任。
        match workspace_root.map(|root| canonical_path(Path::new(root))) {
            Some(root) if !root.as_os_str().is_empty() => {
                let candidate = join_lexically(&root, &normalized.path);
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

/// 在**词法上**把已规范化的相对片段接到根后面：先拼接再折叠，让片段内部的 `..` 能真正回退到根里的层级。
///
/// 直接 `root.join(segment)` 会让 `sub/../file.txt` 带着未折叠的 `..` 进入 `strip_prefix` 的比较（`Path` 的
/// 前缀比较是组件级字面量比较），从而把合法区内路径误判为越界。片段已保证没有前导 `..`（调用方先查
/// [`Normalized::escapes`]），因此折叠后仍落在根之下。
fn join_lexically(root: &Path, segment: &Path) -> PathBuf {
    let mut joined = root.to_path_buf();
    joined.push(segment);
    normalize(&joined).path
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

/// 词法规范化的结果：路径本身 + **显式计数**的逃逸层级。
struct Normalized {
    /// 折叠 `.`/`..` 之后的路径：前缀与根原样保留；未能匹配的**前导** `..` 按原数留在路径最前面。
    ///
    /// 该取值只在 [`Normalized::escapes`] 为 0 时才是「相对某个根」的可用路径；否则它仍带前导
    /// `..`（[`display_path`] 因此判越界，不拿它做前缀比较）。
    path: PathBuf,
    /// 无法回退的**前导** `..` 个数（相对路径专用）。
    ///
    /// `> 0` 表示该路径逃出了它的起点：`../../etc/passwd` 是 2、`../etc/passwd` 是 1、`a/../b` 是 0。
    /// 绝对根（`/`、`C:\`、`\\server\share`）之上没有可回退的层级，因此该计数在绝对路径上恒为 0，
    /// 根段的 `..` 一律被吞掉。
    escapes: usize,
}

/// 词法规范化：去掉 `.`、处理 `..`、合并重复分隔符，并**显式统计**逃出起点的前导 `..` 个数。
///
/// 前导 `..` 的个数必须显式计数，**绝不能**依赖 `PathBuf::pop()` 的返回值：`pop()` 在弹掉
/// `Prefix`/`RootDir` 组件（`C:\`、`\\server\share`）时返回 `true` 而非失败，于是第二个前导 `..` 会把
/// 根前缀「成功」弹掉、得到一个空缓冲区，随后的路径段被当成普通相对路径追加——`../../etc/passwd`
/// 因此被判成工作区内并下发 `etc/passwd`（R7 的信息泄露缺陷）。
///
/// 逐组件规则：
/// - `Prefix`/`RootDir` 进入结果前缀，并把已累计的 `..` 一次性作废（绝对根之上不可回退：`/..` = `/`、
///   `C:\..` = `C:\`）；
/// - `Normal` 压栈（它只能取消**它前面**的 `..`，不能取消前导 `..`）；
/// - `ParentDir` 优先弹掉栈顶的 `Normal`；栈空时——绝对根下丢弃（根段不可回退），相对形式下计入
///   [`Normalized::escapes`]。
fn normalize(path: &Path) -> Normalized {
    let mut base = PathBuf::new();
    let mut rooted = false;
    let mut escapes = 0usize;
    let mut stack: Vec<OsString> = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) => base.push(component.as_os_str()),
            Component::RootDir => {
                base.push(component.as_os_str());
                rooted = true;
                escapes = 0;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if stack.pop().is_none() && !rooted {
                    escapes += 1;
                }
            }
            Component::Normal(part) => stack.push(part.to_os_string()),
        }
    }
    let mut out = base;
    for _ in 0..escapes {
        out.push("..");
    }
    for part in stack {
        out.push(part);
    }
    Normalized { path: out, escapes }
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
    let normalized = normalize(path).path;
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

/// 剥掉 Windows 的 verbatim 前缀，使「已 `canonicalize` 化的根」与「词法规范化的报告路径」可比较。
///
/// - `\\?\D:\...` 剥掉前缀成为 `D:\...`。
/// - `\\?\UNC\server\share\...`（verbatim UNC）还原为普通 UNC 形式 `\\server\share\...`。
/// - 非 Windows 平台逐字节直通（identity），见下方 `#[cfg(not(windows))]` 版本。
///
/// 除 `canonical_path` 外，出站 ACP 参数（本地 Agent 的 `session/new`/`session/resume` 之 `cwd`）也复用
/// 本函数：前缀规则必须只有一处实现。`derive` 模块本身私有，故 `pub` 只经由 `lib.rs` 顶层
/// `pub use` 暴露为 `acp_core::strip_verbatim`，不额外打开模块。
#[cfg(windows)]
pub fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path,
    }
}

/// 非 Windows 平台上的 [`strip_verbatim`]：verbatim 前缀只存在于 Windows，故逐字节直通（identity）。
///
/// Windows 版本的完整语义（`\\?\` 剥除、`\\?\UNC\server\share` 还原为 `\\server\share`）见其上方
/// `#[cfg(windows)]` 版本。`derive` 模块本身私有，故 `pub` 只经由 `lib.rs` 顶层 `pub use` 暴露。
#[cfg(not(windows))]
pub fn strip_verbatim(path: PathBuf) -> PathBuf {
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
/// 只写合同登记的字段：`addedLines`/`deletedLines` 在判定不出时**整个键缺席**（省略而非零），
/// `outsideWorkspace` 只在越界时为 `true`（工作区内时省略，与既有 fixture 的最小集一致）。
///
/// 差异行（feature `core.file-diff.v1`）与 `diffUnavailable` **互斥**：上限内写完整的 `diffLines`，
/// 超限时只写 `diffUnavailable{reason:"size_limit"}`（绝不写被裁剪的行，也绝不只是静默缺席）；判定
/// 不出时两者都不写（与统计同源，由 [`LineDiff::Omitted`] 一态表达）。本模块总是写出差异行——未协商
/// 该 feature 的连接由投递层按协商结果去掉该键（`server::sync` 是唯一的投递侧门控点）。
pub(crate) fn file_changed_view(change: &DerivedFileChange, session: &str) -> String {
    let mut out = String::from("{\"changeId\":");
    out.push_str(&encode_json_string(&change.change_id(session)));
    out.push_str(",\"kind\":");
    out.push_str(&encode_json_string(change.kind));
    out.push_str(",\"displayPath\":");
    out.push_str(&encode_json_string(&change.display_path));
    out.push_str(",\"summary\":");
    out.push_str(&encode_json_string(&change.summary()));
    if let Some((added, deleted)) = change.diff.stats() {
        out.push_str(",\"addedLines\":\"");
        out.push_str(&added.to_string());
        out.push_str("\",\"deletedLines\":\"");
        out.push_str(&deleted.to_string());
        out.push('"');
    }
    if change.outside {
        out.push_str(",\"outsideWorkspace\":true");
    }
    if let Some(lines) = change.diff.lines() {
        out.push_str(",\"diffLines\":[");
        for (index, line) in lines.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"kind\":");
            out.push_str(&encode_json_string(line.kind));
            out.push_str(",\"text\":");
            out.push_str(&encode_json_string(&line.text));
            out.push('}');
        }
        out.push(']');
    } else if change.diff.over_limit() {
        out.push_str(",\"diffUnavailable\":{\"reason\":\"size_limit\"}");
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

    /// 逐行差异里的 `added`/`deleted` 计数。
    fn counts_of(lines: &[DiffLine]) -> (u64, u64) {
        let added = lines.iter().filter(|line| line.kind == "added").count() as u64;
        let deleted = lines.iter().filter(|line| line.kind == "deleted").count() as u64;
        (added, deleted)
    }

    /// R10：从同一份 `oldText`/`newText` 派生逐行差异（新增/删除/上下文），且与统计**同源**。
    #[test]
    fn line_diff_derives_added_deleted_and_context_lines() {
        let diff = line_diff(Some("a\nb\nc"), Some("a\nX\nc"));
        assert_eq!(diff.stats(), Some((1, 1)));
        assert!(!diff.over_limit());
        let lines = diff.lines().expect("上限内必须带差异行");
        let shape: Vec<(&str, &str)> = lines
            .iter()
            .map(|line| (line.kind, line.text.as_str()))
            .collect();
        assert_eq!(
            shape,
            vec![
                ("context", "a"),
                ("deleted", "b"),
                ("added", "X"),
                ("context", "c"),
            ]
        );
        assert_eq!(counts_of(lines), (1, 1), "逐行计数与统计必须相等");
    }

    #[test]
    fn line_diff_treats_a_missing_old_text_as_a_new_file() {
        let diff = line_diff(None, Some("a\nb"));
        assert_eq!(diff.stats(), Some((2, 0)));
        let lines = diff.lines().expect("差异行");
        assert!(lines.iter().all(|line| line.kind == "added"));
        assert_eq!(
            lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
    }

    /// R6 + R10：判定不出时统计与差异行**一起**省略，且**不是**超限（不得写 `diffUnavailable`）。
    #[test]
    fn line_diff_omits_stats_and_lines_together_when_undetermined() {
        let diff = line_diff(Some("a\nb"), None);
        assert_eq!(diff.stats(), None, "判定不出时省略而非填零");
        assert!(diff.lines().is_none());
        assert!(!diff.over_limit(), "判定不出不是超限：不得标记差异行未下发");
    }

    /// R10：超出 §14 的固定上限时**不裁剪**——只保留统计并把差异行标记为未下发。
    #[test]
    fn line_diff_marks_over_limit_instead_of_trimming_lines() {
        // 行数：两侧各 401 行、内容完全不同（完整差异必然超过 400 行的上限）。
        let old = (0..401)
            .map(|index| format!("old-{index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let new = (0..401)
            .map(|index| format!("new-{index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let diff = line_diff(Some(&old), Some(&new));
        assert_eq!(diff.stats(), Some((401, 401)), "超限时统计照常给出");
        assert!(diff.lines().is_none(), "超限时不得下发被裁剪的行");
        assert!(diff.over_limit());

        // 合计字节：两侧各 400 行 × 100 字节 = 80 KiB > 64 KiB（行数与单行长度都在上限内）。
        let line = "y".repeat(100);
        let text = vec![line.as_str(); 400].join("\n");
        let diff = line_diff(Some(&text), Some(&text));
        assert!(diff.over_limit());
        assert_eq!(diff.stats(), Some((0, 0)));

        // 单行字符数：一行超过 4,096 字符时同样不下发（裁剪那一行就是「下发了被裁剪的内容」）。
        let long = "z".repeat(DIFF_LINE_MAX_CHARS + 1);
        let diff = line_diff(Some("short"), Some(&long));
        assert!(diff.over_limit());
        assert!(diff.lines().is_none());
    }

    /// R6 + R10：逐行差异与统计**同源**——脚本的计数与 `line_stats` 在随机输入上逐例相等，且每个旧行、
    /// 每个新行在脚本里恰好出现一次（顺序与原文一致）。
    #[test]
    fn line_diff_agrees_with_line_stats_on_random_inputs() {
        let mut rng = Lcg(0x9e37_79b9_7f4a_7c15);
        let alphabet = ["alpha", "beta", "gamma", "delta", "epsilon"];
        for _ in 0..400 {
            let n = rng.next(20);
            let m = rng.next(20);
            let old_lines: Vec<&str> = (0..n).map(|_| alphabet[rng.next(alphabet.len())]).collect();
            let new_lines: Vec<&str> = (0..m).map(|_| alphabet[rng.next(alphabet.len())]).collect();
            let old = old_lines.join("\n");
            let new = new_lines.join("\n");
            let diff = line_diff(Some(&old), Some(&new));
            assert_eq!(
                diff.stats(),
                line_stats(Some(&old), Some(&new)),
                "统计必须与既有口径逐字一致"
            );
            let lines = diff.lines().expect("20 行以内必定在上限内");
            assert_eq!(counts_of(lines), diff.stats().expect("判定得出"));
            assert_eq!(
                lines
                    .iter()
                    .filter(|line| line.kind != "added")
                    .map(|line| line.text.as_str())
                    .collect::<Vec<_>>(),
                old_lines,
                "旧侧每个行各出现一次且顺序不变"
            );
            assert_eq!(
                lines
                    .iter()
                    .filter(|line| line.kind != "deleted")
                    .map(|line| line.text.as_str())
                    .collect::<Vec<_>>(),
                new_lines,
                "新侧每个行各出现一次且顺序不变"
            );
        }
    }

    /// R10：`file.changed` 视图按上限写 `diffLines`；判定不出时两个键都不写，超限时只带统计 +
    /// `diffUnavailable`。统计与摘要字段不得被差异内容文本化。
    #[test]
    fn file_changed_view_writes_diff_lines_and_marks_the_omitted_case() {
        let view = r#"{"toolCallId":"tool-1","diff":[{"path":"src/main.rs","oldText":"a\nb","newText":"a\nX"}]}"#;
        let derived = derived_file_changes(view, None).expect("可解析");
        let text = file_changed_view(&derived[0], "session-1");
        assert!(
            text.contains(
                r#""diffLines":[{"kind":"context","text":"a"},{"kind":"deleted","text":"b"},{"kind":"added","text":"X"}]"#
            ),
            "{text}"
        );
        assert!(!text.contains("diffUnavailable"), "{text}");
        // 本用例**没有**传工作目录根，因此展示路径按 R7 只留文件名称并置 `outsideWorkspace`；摘要是
        // `kind` + 展示路径的同源产物（`DerivedFileChange::summary`），不会把请求里的相对路径拼回来。
        assert!(text.contains(r#""displayPath":"main.rs""#), "{text}");
        assert!(text.contains(r#""outsideWorkspace":true"#), "{text}");
        assert!(
            text.contains(r#""summary":"modified main.rs""#),
            "摘要仍只描述改动本身：{text}"
        );

        // 判定不出：`addedLines`/`deletedLines`、`diffLines` 与 `diffUnavailable` 都不写。
        let bare = derived_file_changes(
            r#"{"toolCallId":"tool-2","diff":[{"path":"src/main.rs","oldText":"a"}]}"#,
            None,
        )
        .expect("可解析");
        let text = file_changed_view(&bare[0], "session-1");
        assert!(
            !text.contains("diffLines")
                && !text.contains("diffUnavailable")
                && !text.contains("addedLines"),
            "{text}"
        );

        // 超限：只带统计 + 已登记原因，绝不带被裁剪的行。
        let old = (0..301)
            .map(|index| format!("old-{index}"))
            .collect::<Vec<_>>()
            .join("\\n");
        let new = (0..301)
            .map(|index| format!("new-{index}"))
            .collect::<Vec<_>>()
            .join("\\n");
        let view = format!(
            r#"{{"toolCallId":"tool-3","diff":[{{"path":"src/big.rs","oldText":"{old}","newText":"{new}"}}]}}"#
        );
        let derived = derived_file_changes(&view, None).expect("可解析");
        let text = file_changed_view(&derived[0], "session-1");
        assert!(
            text.contains(r#""diffUnavailable":{"reason":"size_limit"}"#),
            "{text}"
        );
        assert!(
            !text.contains("diffLines"),
            "超限时不得下发被裁剪的行：{text}"
        );
        assert!(
            text.contains(r#""addedLines":"301""#) && text.contains(r#""deletedLines":"301""#),
            "{text}"
        );
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
    fn display_path_escapes_many_leading_parent_traversals() {
        // R7 的动机输入（`design.md` D5）：`../../etc/passwd` 必须判越界且只给文件名称。
        // 旧实现用 `PathBuf::pop()` 的返回值计数前导 `..`，而 `pop()` 弹掉根前缀时返回 `true`，
        // 于是第二个 `..` 把根弹掉、计数归零，`etc/passwd` 被当成相对路径追加，整条路径被误判为区内
        // 并把工作区外的**目录结构**下发到面向客户端的字段上。
        for reported in [
            "../../etc/passwd",
            "../../../etc/passwd",
            "../../../../../../a/b/c",
            "../../Users/alice/.ssh/id_rsa",
        ] {
            let (display, outside) = display_path(Some("/work/api"), reported);
            assert!(outside, "R7：{reported} 必须判越界");
            assert!(
                !display.contains('/'),
                "R7：越界只给文件名称，不得下发目录结构，实际 {display}"
            );
            assert!(
                !display.contains(".."),
                "R7：不得下发回退层级，实际 {display}"
            );
            assert!(
                !display.contains("work") && !display.contains("api") && !display.contains("etc"),
                "R7：不得下发工作目录根或区外目录的片段，实际 {display}"
            );
        }
    }

    #[test]
    fn display_path_escapes_multi_level_traversal_to_the_declared_file_names() {
        assert_eq!(
            display_path(Some("/work/api"), "../../etc/passwd"),
            ("passwd".to_owned(), true)
        );
        assert_eq!(
            display_path(Some("/work/api"), "../../Users/alice/.ssh/id_rsa"),
            ("id_rsa".to_owned(), true)
        );
    }

    #[test]
    fn display_path_keeps_an_inner_parent_traversal_that_stays_inside() {
        // 回归红线：区内路径**内部**的 `..` 只要规范化后仍在根之下，就必须判为区内。
        for reported in ["/work/api/../api/x", "sub/../file.txt", "a/b/../../c.rs"] {
            let (display, outside) = display_path(Some("/work/api"), reported);
            assert!(!outside, "R7：{reported} 未越界，不得误判");
            assert!(
                !display.contains(".."),
                "R7：区内展示值不得含回退层级，实际 {display}"
            );
        }
        assert_eq!(
            display_path(Some("/work/api"), "/work/api/../api/x"),
            ("x".to_owned(), false)
        );
        assert_eq!(
            display_path(Some("/work/api"), "sub/../file.txt"),
            ("file.txt".to_owned(), false)
        );
        assert_eq!(
            display_path(Some("/work/api"), "a/b/../../c.rs"),
            ("c.rs".to_owned(), false)
        );
    }

    #[test]
    fn display_path_accepts_a_relative_form_as_workspace_relative() {
        let (display, outside) = display_path(Some("/work/api"), "src/main.rs");
        assert!(!outside);
        assert_eq!(display, "src/main.rs");
    }

    #[test]
    fn display_path_treats_the_workspace_root_itself_by_the_spec() {
        // 「恰好等于工作区根」既不是越界下发（根的任何片段）也不是一个文件：只下发哨兵 `.` 并标记越界，
        // 使客户端呈现为不可用而不是把根路径当成改动对象。用真实临时目录，使根与报告路径同形（与
        // `display_path_relativizes_inside_the_workspace` 同一口径）。
        let root = std::env::temp_dir();
        let root_text = root.to_string_lossy().into_owned();
        let (display, outside) = display_path(Some(&root_text), &root_text);
        assert!(outside, "R7：根本身不得当作区内文件");
        assert_eq!(display, ".", "R7：不得下发根的任何片段");
        assert_ne!(display, "api");
    }

    /// `C:\..\..\etc\passwd`：根段的 `..` 一律被吞掉（绝对根之上不可回退）；在 Windows 上盘符根必须
    /// 原样保留（这正是旧实现被 `pop()` 弹掉的那一段）。UNC 份额根同理。
    #[cfg(windows)]
    #[test]
    fn normalize_counts_leading_parent_traversals_without_popping_the_root() {
        let normalized = normalize(Path::new(r"C:\..\..\etc\passwd"));
        assert_eq!(normalized.escapes, 0, "绝对根之上没有可回退的层级");
        assert_eq!(normalized.path, Path::new(r"C:\etc\passwd"));

        let unc = normalize(Path::new(r"\\server\share\..\..\x"));
        assert_eq!(unc.escapes, 0);
        assert_eq!(unc.path, Path::new(r"\\server\share\x"));
    }

    #[test]
    fn normalize_counts_leading_traversals_on_relative_paths() {
        // 相对形式：前导 `..` 逐级计数，`Normal` 只能抵消**它前面**的 `..`。
        assert_eq!(normalize(Path::new("../etc/passwd")).escapes, 1);
        assert_eq!(normalize(Path::new("../../etc/passwd")).escapes, 2);
        assert_eq!(normalize(Path::new("../../../../a/b")).escapes, 4);
        assert_eq!(normalize(Path::new("a/../b")).escapes, 0);
        assert_eq!(normalize(Path::new("a/../../b")).escapes, 1);
        // 越界时路径仍带前导 `..`（调用方据此判越界，不拿它做前缀比较）。
        assert_eq!(
            normalize(Path::new("../../etc/passwd")).path,
            Path::new("../../etc/passwd")
        );
    }

    #[cfg(windows)]
    #[test]
    fn normalize_marks_the_root_under_a_windows_prefix() {
        // 根被 `pop()` 吃掉的形状：`C:\`、`C:\..`、`C:\a\..\..` 都必须保留根前缀且计数为 0。
        // verbatim 前缀的剥离发生在 `strip_verbatim`（`canonicalize` 之后），词法层原样保留。
        for (input, expected) in [
            (r"C:\", r"C:\"),
            (r"C:\..", r"C:\"),
            (r"C:\a\..\..", r"C:\"),
            (r"C:\a\b\..", r"C:\a"),
        ] {
            let normalized = normalize(Path::new(input));
            assert_eq!(normalized.escapes, 0, "{input}");
            assert!(normalized.path.has_root(), "{input}: 根必须保留");
            assert_eq!(
                normalized.path.as_os_str().to_string_lossy(),
                expected,
                "{input}"
            );
        }
        // UNC 份额根本身同样不可回退。
        let unc = normalize(Path::new(r"\\server\share\a\..\.."));
        assert_eq!(unc.escapes, 0);
        assert_eq!(unc.path.as_os_str().to_string_lossy(), r"\\server\share\");
    }

    #[test]
    fn display_path_traversal_shapes_stay_outside_on_every_platform() {
        // 越界的**判定**必须与平台无关：这些形状是 R7 的核心判据，不应因宿主是 Windows 而改变。
        // （Unix 形式的根前缀在 Windows 上是普通组件，因此 `C:\..\..\etc\passwd` 只在 Windows 上
        // 走真实前缀分支；其余形状在两条平台上都走同一段计数逻辑。）
        assert_eq!(
            display_path(Some("/work/api"), "../../etc/passwd"),
            ("passwd".to_owned(), true)
        );
        assert_eq!(
            display_path(Some("/work/api"), "../../../a/b"),
            ("b".to_owned(), true)
        );
        assert_eq!(
            display_path(Some("/work/api"), "/work/api/../api/x"),
            ("x".to_owned(), false)
        );
        assert_eq!(
            display_path(Some("/work/api"), "src/main.rs"),
            ("src/main.rs".to_owned(), false)
        );
    }

    #[cfg(windows)]
    #[test]
    fn display_path_treats_a_windows_parent_traversal_as_outside() {
        // `C:\..\..\etc\passwd`：根被吞掉后**不得**把 `etc\passwd` 当成区内相对路径。
        let root = std::env::temp_dir();
        let root_text = root.to_string_lossy().into_owned();
        let outside = format!(r"{}\..\..\Windows\System32\drivers\etc\hosts", root_text);
        let (display, outside_flag) = display_path(Some(&root_text), &outside);
        assert!(outside_flag, "R7：越过盘符根必须判越界");
        assert_eq!(display, "hosts", "R7：只给文件名称");
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
        assert_eq!(derived[0].diff.stats(), Some((1, 1)));
        assert!(!derived[0].outside);
    }

    #[test]
    fn derived_changes_omit_stats_without_new_text() {
        let view = r#"{"toolCallId":"tool-1","diff":[{"path":"src/main.rs","oldText":"a"}]}"#;
        let derived = derived_file_changes(view, None).expect("可解析");
        assert_eq!(derived.len(), 1);
        assert_eq!(derived[0].diff.stats(), None, "判定不出时省略而非填零");
        assert_eq!(derived[0].kind, "modified", "没有改动后内容时不猜改动方向");
    }

    #[test]
    fn derived_changes_report_a_new_file_as_added() {
        let view = r#"{"toolCallId":"tool-1","diff":[{"path":"src/new.rs","newText":"a\nb"}]}"#;
        let derived = derived_file_changes(view, None).expect("可解析");
        assert_eq!(derived.len(), 1);
        assert_eq!(derived[0].kind, "added");
        assert_eq!(derived[0].diff.stats(), Some((2, 0)));
    }

    #[test]
    fn derived_changes_report_an_emptied_file_as_deleted() {
        let view = r#"{"toolCallId":"tool-1","diff":[{"path":"src/gone.rs","oldText":"a\nb","newText":""}]}"#;
        let derived = derived_file_changes(view, None).expect("可解析");
        assert_eq!(derived.len(), 1);
        assert_eq!(derived[0].kind, "deleted");
        assert_eq!(derived[0].diff.stats(), Some((0, 2)));
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

    #[cfg(windows)]
    #[test]
    fn strip_verbatim_removes_the_windows_prefix() {
        for (input, expected) in [
            (r"\\?\D:\tmp\ws", r"D:\tmp\ws"),
            (r"\\?\UNC\server\share\ws", r"\\server\share\ws"),
        ] {
            let stripped = strip_verbatim(PathBuf::from(input));
            assert_eq!(stripped.as_os_str().to_string_lossy(), expected, "{input}");
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn strip_verbatim_is_identity_on_non_windows() {
        // 非 Windows 分支逐字节直通：verbatim 形态的输入也不做任何改写（Linux CI 会真正执行这条）。
        for input in [r"\\?\D:\tmp\ws", r"\\?\UNC\server\share\ws"] {
            let stripped = strip_verbatim(PathBuf::from(input));
            assert_eq!(stripped.as_os_str().to_string_lossy(), input, "{input}");
        }
    }

    #[test]
    fn strip_verbatim_leaves_unprefixed_paths_byte_identical() {
        // 无前缀路径在两条平台上都必须逐字节不变。
        for input in ["/tmp/ws", "relative/ws"] {
            let stripped = strip_verbatim(PathBuf::from(input));
            assert_eq!(stripped.as_os_str().to_string_lossy(), input, "{input}");
        }
    }
}

<!-- WP2 缺陷修复原始报告（执行者 coder-w2-r2，第 2 轮 fix；只处理 review-wp2-r1 的 F6/F7）。 -->

WP ID: WP2（交付单元 MU1a，阶段 fix，第 2 轮）
task_id: "2.2"
role: coder
phase: fix
agent_context:
  agent_id: "CoderWp2Fix / coder-w2-r2"
  isolation: "fork_turns=none（独立修复实例；承接 review-wp2-r1 的 F6/F7，不继承第 1 轮实现或任何 review 对话）"
target_revision: "e5a31dc05ea55015446c19bfce9675f72f2ddc7b"
base_revision: "f64a1968ed56abbac680d661f0e57f05d57fda9d"
scope: "仅 review-wp2-r1 的两项文档缺陷：F6（docs/SYNC_PROTOCOL.md §10.3 措辞）与 F7（判别器拼写）。写入范围严格限于 docs/SYNC_PROTOCOL.md 与 docs/ACP_COMPATIBILITY_MATRIX.md。"
changes: "1 个文件、+1/-1（docs/SYNC_PROTOCOL.md §10.3 的 file.changed 第二个要点句），固定为提交 e5a31dc。F7 经逐字节核对判定为已满足、无需改动（详见第 3 节）。未触碰任何 schema / views.rs / fixture / manifest.json / command.rs / sync.rs / crates/*/tests/**。"
result: PARTIAL（F6 已修并固定为提交；F7 无代码可改——交付文本已等于上游取值，逐字节证明见下。PV1 的字面 `npm run verify` 在分配 worktree 内 exit 101，失败点为与本包无关的既有 `app::daemon_lifecycle` 时序 flake；十道 check 脚本与 cargo fmt/clippy 全绿，详见 Checks）

## 1. 交付概览

| 项 | 取值 |
| --- | --- |
| 仓库 | `D:\Project\acp-remote` |
| worktree | `D:\Project\acp-remote\.worktrees\wp2` |
| 分支 | `feat/wp2-derived-event-fields` |
| 修复基线（base） | `f64a1968ed56abbac680d661f0e57f05d57fda9d`（其上已并入主分支构建修复 `b964ae3`） |
| 修复提交（target） | `e5a31dc05ea55015446c19bfce9675f72f2ddc7b` |
| 提交信息 | `docs(sync): WP2 修复 F6——把 file.changed 行数口径挂到「有无 Diff 元素」` |
| `git diff --stat`（本封装改动） | `docs/SYNC_PROTOCOL.md \| 2 +-` —— 1 file changed, 1 insertion(+), 1 deletion(-) |
| 上下文方式 | fork_turns=none；只读核对后仅改 F6 一处，逐字节证据见第 2/3 节 |
| 上游依赖 | 无（W1，Dependencies = `none`） |
| 资源 | `CARGO_TARGET_DIR=D:\Project\acp-remote\.target-wt\wp2`（本包独占）；仓库根 `node_modules/` 只读消费 |

**写入范围合规**：`git status --porcelain --untracked-files=all` 在本 worktree 内为空（无游离文件、无探针残留，已清理）。相对 base 的 `git diff --name-only` 仅 `docs/SYNC_PROTOCOL.md`（另一行 `vendor/windows-local-ipc/Cargo.toml` 来自 base 之后并入主分支的 `b964ae3`，非本包改动）。未触碰 WP1 独占的 `crates/sync-protocol/src/{command,sync}.rs` 与 TP1 区域 `crates/*/tests/**`。

## 2. F6 — docs/SYNC_PROTOCOL.md §10.3 措辞（已修）

**位置**：`docs/SYNC_PROTOCOL.md:1040`（§10.3「`file.changed` 的三个可选字段的语义」第二个要点）。

### 修改前（逐字）

> - 这两个字段只覆盖**Agent 在其工具调用里声明的改动**：派生源是 ACP 工具调用内容中的类型化 Diff 元素（见 `CORE_PORTS_AND_STORAGE.md`）。Agent 通过 shell 重定向、脚本或外部工具驱动的文件改动不在其中，行数统计也不计入；此类改动不产生 `file.changed` 事件。

### 修改后（逐字）

> - 这两个字段只覆盖**Agent 在其工具调用里声明的改动**：派生源是 ACP 工具调用内容中的类型化 Diff 元素（见 `CORE_PORTS_AND_STORAGE.md`）。判定只依据该次调用内容**是否含 Diff 元素**，与改动由谁驱动无关：调用内容中不含 Diff 元素的（例如只做 shell 重定向、脚本或外部工具驱动的改动），其行数统计不计入，也不产生 `file.changed` 事件。

### 理由（对齐权威依据，不弱化 spec、不引入新语义）

- **可判定性**：原句把结论挂在「改动由谁驱动」上——这只能由作者意图推断，无法从可得输入判定；而 `specs/local-agent-host/spec.md:7` 明令「MUST NOT 从工具调用的自由形状原始输入中猜测文件改动；不含 Diff 元素的工具调用 MUST NOT 被上报为文件改动」。改后把结论挂到「该次调用内容是否含 Diff 元素」，与实现侧唯一可键控的事实一致。
- **不弱化 R5**：`specs/core-derived-events/spec.md:11`（R5）的禁止范围是「MUST NOT 为**不含文件改动的工具调用**派生该事件」，并把来源限定为「类型化 Diff 元素」。改后措辞保留「不含 Diff 元素的调用不产生事件、行数不计入」，与该禁止范围同宽；shell/脚本/外部工具驱动的改动作为「不含 Diff 元素」的**示例**出现（原句中它本是并列主体），未新增或放宽任何语义。
- **不改变行数口径**：仍保留「这些字段只覆盖 Agent 在其工具调用里声明的改动」（对应 R6/D4 的行级差异统计与「只覆盖工具调用里声明的改动」），故与同段第一点位的行数语义、与 `docs/ACP_COMPATIBILITY_MATRIX.md:181`「shell 驱动的改动不计入」一致。
- **风格**：保持中文表述与文档既有 MUST/MUST NOT 口径及术语（「类型化 Diff 元素」「派生源」）不变。

## 3. F7 — 判别器拼写（判定为已满足，无需改动）

### 结论

派发单的「现状」描述在进入本实例上下文时被截断/畸变（其「判别器写成 X 之外的形式」中的 X 也渲染为 snake）。为不据转述臆断，本实例对**交付提交与工作区内的实际字节**做了穷尽核对，结论是：**F7 的目标（统一为上游 `session_info_update`）在 base `f64a196` 上即已满足**，三处散文判别子逐字节等于 `session_info_update`；仓库内**不存在**该判别子的 camelCase / PascalCase 散文写法可供「改正」。因此无可改动，未做（也不应做）越出 F6/F7 范围的字面改写。

### 逐字节证据（SHA-256 前 16 位；camel/snake 字面以转义构建，规避工具输出归一化）

| 字面 | SHA-256[:16] |
| --- | --- |
| `session` + `\x49` + `nfo` + `\x55` + `pdate`（camelCase） | `0a989c921e83d239` |
| `session` + `\x5f` + `info` + `\x5f` + `update`（snake_case，上游取值） | `2c55ff8ca18144fa` |
| `Session` + `\x49` + `nfo` + `\x55` + `pdate`（Rust DTO 名） | `fa4e96d110e5280f` |

| 站点 | 实际 token 的 SHA-256[:16] | 判定 |
| --- | --- | --- |
| `docs/SYNC_PROTOCOL.md:1045`（F7 点名处） | `2c55ff8ca18144fa` | = snake（上游取值） |
| `docs/SYNC_PROTOCOL.md:1060`（同文件映射表） | `2c55ff8ca18144fa` | = snake |
| `docs/ACP_COMPATIBILITY_MATRIX.md:186`（F7 点名处） | `2c55ff8ca18144fa` | = snake |
| `docs/SESSION_CONTINUITY_DESIGN.md:51`（既有文档） | `2c55ff8ca18144fa` | = snake |

- 对 `docs/SYNC_PROTOCOL.md:1045` 与 `docs/ACP_COMPATIBILITY_MATRIX.md:186` 的原始字节（UTF-8）逐字节打印确认：判别子四位分隔符为 `0x5f 0x5f`（下划线 ×2），即 `session_info_update`。
- 全仓库（`git grep` + 字节扫描，排除 `node_modules`/`target`/.git）中 camelCase 字面 `0a989c92…` **零命中**；PascalCase 仅命中 Rust DTO 标识符（`crates/acp-protocol/src/update.rs` 的 `SessionInfoUpdate`、`crates/agent-host/src/mapper.rs`、`schemas/acp/v1/upstream/schema.json` 的 `SessionInfoUpdate` 定义），均为代码标识符、非判别子 wire 取值，无需改动。
- 权威依据核对一致：`schemas/acp/v1/upstream/schema.json:3806` `"const": "session_info_update"`；`crates/acp-protocol/src/update.rs:72/94`；`compatibility/acp/v1/matrix.json:73` `"wireValue": "session_info_update"`。

### 备注（供主 Agent 裁决）

F7 在 `review-wp2-r1` 中登记的是 **SUGGESTION**，其 Recommendation 为「把新增段落统一为 `session_info_update`」——该目标已在 f64a196 达成。若主 Agent 认为仍有一处据转述而异的字面待改，请提供该字面的**可复现来源（文件:行 + 原始字节或哈希）**；本实例在授予范围内穷尽核对未发现该对象，故不动它，以避免对已正确的上游取值做无依据改写或越界扩改。

## 4. Checks（按检查 ID）

| 检查 ID | 命令 | 工作目录 | 环境 | 退出码 | 子检查结果 | 日志路径 |
| --- | --- | --- | --- | --- | --- | --- |
| Local-1 | `cargo fmt --all` | `D:\Project\acp-remote\.worktrees\wp2` | `CARGO_TARGET_DIR=D:\Project\acp-remote\.target-wt\wp2` | 0 | 本轮只改 Markdown，无 Rust 源码改动 → 无格式变化 | （stdout 直读） |
| PV1 | `npm run verify` | `D:\Project\acp-remote\.worktrees\wp2` | `CARGO_TARGET_DIR=D:\Project\acp-remote\.target-wt\wp2` | **101（FAIL）** | `check` 十道脚本**全 PASS**；`check:rust` 的 `cargo fmt --check` PASS、`cargo clippy -D warnings` PASS、`cargo test` 在 `app::daemon_lifecycle` 失败（见下） | `D:\Project\acp-remote\.target-wt\wp2-verify-fix.log` |

### PV1 子检查明细

- `check` → `check:schemas` / `check:commands` / `check:errors`（58 codes）/ `check:features` / `check:assets` / `check:acp`（25 methods, 11 updates, 71 rows）/ `check:docs`（412 relative links, 9048 section refs across 518 markdown files）/ `check:boundaries`（12 crate）/ `check:drift` / `check:agentic`（含 openspec validate）：**全部 PASS**。
- `check:rust` → `cargo fmt --all -- --check`：**PASS**；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`：**PASS**；`cargo test --locked --workspace --all-features`：**在 `-p app --test daemon_lifecycle` 的 `the_periodic_task_runs_again_after_one_full_cycle` 失败（11 passed; 1 failed）**，其余各 suite 全绿（57/57、22/22、19/19 等）。

### PV1 失败归因（环境/既有，非本包缺陷）

- 失败点为 `crates/app/tests/daemon_lifecycle.rs`，该文件相对 base **逐字节未改**（`git diff base -- crates/app/tests/daemon_lifecycle.rs` 为空），且不在本包写入范围内。
- 失败断言是 `daemon.start()` 后立即读取 `daemon.log_events("daemon.maintenance")` 期望恰 1 轮（`assert_eq!(..., 1)`）；日志显示 startup 清理事件**确实存在**（`"reason":"startup"`），是「读取快于日志落盘」的**时序竞态**。
- **非确定性复现**：单独复跑 `cargo test --locked -p app --test daemon_lifecycle --all-features`，失败者变成**另一个**用例（`the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown`），而首轮失败的 `the_periodic_task_runs_again_after_one_full_cycle` **通过**——典型的同文件内多用例互扰/资源竞争 flake，与文档改动无因果。
- 该用例自述「唯一会等满一个 60s 周期，总时长约 70s」，属敏感的时长型用例。
- 结论：本包（Markdown-only）不可能引入该失败；按报告契约据实记 PV1 **FAIL**，但归类为**环境/既有 flake**，不属 WP2 与本次修复的缺陷。建议由主 Agent 或对应包（`crates/app` 所有者）单独立项处理。

## 5. 修改前后对照（提交内）

见第 2 节「修改前/修改后」逐字文本；`git show e5a31dc -- docs/SYNC_PROTOCOL.md` 的 diff 为 `docs/SYNC_PROTOCOL.md` 单行 `-/+`。

## 6. 资源释放

- `CARGO_TARGET_DIR=D:\Project\acp-remote\.target-wt\wp2` 为 provisioner 分配的独占构建目录，保留供后续 reviewer 复用，未清理。
- 探针文件 `.tmp_probe_camel.txt`、`.tmp_probe2.txt` 已删除；`git status --porcelain --untracked-files=all` 干净。
- worktree 由 provisioner 回收，本实例未创建/切换/清理任何 worktree。

## 7. 未执行项与待澄清

- **未执行**独立 review 与 E2E（非本角色职责）：修复后的 F6 文本须由**新的独立 reviewer** 按原问题 ID 复核；F7 建议一并复核其对「已满足」的独立判定。
- **待澄清（F7 唯一悬项）**：如主 Agent 掌握 F7 所指「非 3 处点名站点上的 camelCase 字面」的可复现来源（文件:行 + 字节/哈希），请回传；本实例在授予范围内未发现该对象。此项不阻断 F6 的复核与合入。
- **PV1 待补**：字面 `npm run verify` 因既有 `daemon_lifecycle` flake 未 exit 0；十道 check 与 fmt/clippy 已全绿。若需 PASS 级 PV1，建议在无并发负载时复跑 `cargo test -p app --test daemon_lifecycle`（该 flake 非确定性），或由该 crate 所有者先行修复时序竞态。

```agentic-handoff
version: 1
agent_context:
  agent_id: "CoderWp2Fix / coder-w2-r2"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "2.2"
    work_package: WP2
    role: coder
    phase: fix
    round: 2
    stage: work-package
    base_revision: "f64a1968ed56abbac680d661f0e57f05d57fda9d"
    target_revision: "e5a31dc05ea55015446c19bfce9675f72f2ddc7b"
    evidence_type: DELIVER
    evidence_id: deliver-wp2-r2
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/deliver-wp2-r2.md"
    result: PARTIAL
    evidence_status: NEW
    applicability_basis: "在 .worktrees/wp2（分支 feat/wp2-derived-event-fields，base=f64a196，其上已并入 b964ae3）上修复 review-wp2-r1 的 F6（docs/SYNC_PROTOCOL.md:1040 措辞），固定为提交 e5a31dc（1 file changed, +1/-1）。F7 经 SHA-256 逐字节核对判定为已满足（三处散文判别子均等于上游 session_info_update），无需改动。PV1 在本 worktree 内执行 npm run verify：十道 check 脚本全 PASS，check:rust 的 fmt/clippy PASS，cargo test 的 app::daemon_lifecycle 出现与本包无关的既有非确定时序 flake（该测试文件相对 base 逐字节未改）。日志见 .target-wt/wp2-verify-fix.log。"
    source_evidence: "openspec/changes/sync-scope-and-pwa-client/reports/review-wp2-r1.md"
```

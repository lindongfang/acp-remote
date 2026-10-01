> coder 子 Agent（coder-E2，第 2 轮修复实例）的交付报告。本文件只覆盖 CR5-F1 的修复与
> 顺带的 CR5-F3 措辞补齐，不复述 CR5 检视结论，也不修改权威计划/任务/verification。

## Shared Report

| 字段 | 值 |
| --- | --- |
| `task_id` | `2.5`（WP5 coder 派发）、`3.6`（WP5 独立检视，本行仅记录待复核） |
| `role` | `coder` |
| `phase` | `fix` |
| `agent_context` | 子 Agent `coder-E2`（第 2 轮修复实例），由主 Agent 派发；上下文**不继承**上一轮 `coder-E`，依据是 `reports/cr5-review.md` + 本轮派发全文 + 只读契约 |
| `work_package` | `WP5` |
| `stage` | `work-package` |
| `base_revision` | `4e53fcf9fcd9eb06130f3cab8de7d21e7973cf63`（`feat(agent-host): 落地会话恢复的后端路径与 ACP 会话标识暴露`；开工前 `git status --short` 为空、`git rev-parse HEAD` 与派发给定值一致） |
| `target_revision` | `1376e1b5c5bbd66d022a447e92a3179e50f173fd`（短 sha `1376e1b`） |
| `scope` | 只写 `crates/agent-host/`（`src/bin/acpr-fake-acp-agent.rs` + `src/host.rs` 注释）；未触碰 `crates/agent-host/` 以外任何文件 |
| `result` | **PASS**（本 WP 的计划检查 PV1 阶段 1 全部满足）。**独立 review（3.6）尚未执行，待独立 reviewer 复核** |

## 本轮唯一目标：CR5-F1

CR5-F1 指出：fake ACP child 的 `--dump-requests` 只写 method，而 TP1 的 **SR-R8-1 / SR-R26-2**
要从该文件读 `params.sessionId` / `params.cwd`，因此 R26/R8 缺线级可观察通道，TP2 无法按字面实现。

主 Agent 裁定的唯一修法（方案 a）：**新增并列选项** `--dump-request-params <path>`，
**不得**改动 `--dump-requests` 的既有冻结语义。已按此实现，未自行改方案。

## 改动清单（2 个文件，2 处功能点 + 1 处措辞）

### 1. `crates/agent-host/src/bin/acpr-fake-acp-agent.rs`

| 位置 | 改动 |
| --- | --- |
| `:17-21`（模块文档） | 说明新选项：每条带 `method` 的入站报文追加一行单行 JSON `{"method":…,"params":…}`；并写明 `--dump-requests` 的冻结语义保持不变、两选项并列可共存 |
| `:40`、`:56`、`:81` | `Args` 新增 `dump_request_params: Option<String>` 字段、默认值与 `--dump-request-params <path>` 解析分支 |
| `:51-59` | `Args::parse` 改为委托新增的 `Args::from_argv(&[String])`（程序名由 `parse` 侧的 `skip(1)` 剔除），使参数解析可直接对 argv 序列做单测；解析循环逻辑本身逐字不变 |
| `:152`（`main` 读循环） | 原 `dump_method(&args, method)` 改为 `dump_inbound(&args, method, message.get("params"))`——**只在带 `method` 的入站报文上触发**，与 `--dump-requests` 的触发条件完全相同 |
| `:187-193` | 新增 `dump_inbound(args, method, params)`：分别调用两个 dump 函数，两者互不依赖 |
| `:195-201` | `dump_method` 保持「每行只写 method」不变，抽出公共的 `append_line` |
| `:203-215` | 新增 `dump_request_params`：`json!({"method": method, "params": params 或 null})`，`to_string()` 后按单行追加。`params` 缺失时写 `null`，因此每行键集合恒为 `{method, params}` |
| `:217-226` | 新增 `append_line(path, line)`：抽出原有的 `OpenOptions(create+append)` + `writeln!` + 静默忽略错误，两个选项共用 |
| `:827-1000` | 新增 `#[cfg(test)] mod tests`：5 个单元测试（与被写代码同文件，随实现交付） |

**冻结语义核验**：`dump_method` 的输出逐字未变（仍是 `{method}` 一行），`tests/session.rs` 里
既有的三处 `--dump-requests` 断言（`:963`、`:1032`、`:1115`）在 PV1-3 中全部通过 ⇒ CR7-F5 的冻结
措辞未被改写。

### 2. `crates/agent-host/src/host.rs`（顺带项 CR5-F3，单列）

`:624-625` 在读 `runtime.capabilities` 前补两行注释：本次拉起进程时能力由刚完成的 `initialize` 协商；
复用既有进程（`spawned == false`）时读到的是**那个进程此前**的协商结果——不重新协商，也不虚报能力。
纯注释，无行为变化（CR5-F3 是 SUGGESTION，本轮做了）。

## 需求映射

| 需求 | 观察通道 | 代码位置 |
| --- | --- | --- |
| **R8**（恢复用创建时暴露的 ACP 会话标识，`params.sessionId` 逐字） | `--dump-request-params` 的 `params.sessionId` | 实现：`bin/acpr-fake-acp-agent.rs:203-215`（`dump_request_params`）、`:152`（`main` 触发点）；单测断言 `dump_request_params_keeps_method_and_params_per_line` |
| **R26**（`cwd` 就是持久化的 `request.workspace_cwd` 原文） | `--dump-request-params` 的 `params.cwd` | 同上；单测以含中文与斜杠的路径断言逐字相等（`/持久化的/cwd`），证明未做别名替换或路径重解析 |

两者的**既有**证据链不变：`host.rs:634` 的 `session_resume_params` 调用点、
`host.rs:825-840` 内联单测（`resume_params_carry_the_persisted_values_verbatim`）、
`bin:341-352` 的 pinned-schema 形状校验。本轮只是**新增**了线级取值通道，不是替换。

## 单元测试（同文件 `#[cfg(test)] mod tests`）

| 测试 | 覆盖的义务 |
| --- | --- |
| `dump_request_params_keeps_method_and_params_per_line` | 每行是单行合法 JSON；键集合恰为 `{method, params}`；`params.sessionId` / `params.cwd` 逐字保真；`params` 缺失写 `null` 而不是缺键 |
| `both_dump_options_coexist_in_independent_files` | 两选项同给：各自写各自文件；**`--dump-requests` 侧仍是纯 method 行**（无 JSON、无 params） |
| `a_single_option_creates_only_its_own_file` | 只给 `--dump-requests` 时，新选项的文件根本不出现 |
| `without_options_nothing_is_written` | 两选项都不给：无文件创建、无副作用（原有行为完全一致） |
| `option_parsing_keeps_following_switches` | 新选项的路径值不吞掉后面的开关（`--no-modes` 仍被识别）；两字段独立 |

正常路径无 `unwrap()`/`expect()`/`panic!`：新增代码里的 5 处 `expect` 全部在 `#[cfg(test)] mod tests` 内
（用 `git diff | grep` 逐行核过）。错误处理沿用既有「静默忽略」口径——本选项只为测试断言服务。

## Checks（PV1 阶段 1，crate 子集 = `agent-host`）

工作目录统一为 `D:/Project/acp-remote-wt/session-resume-wp5`（worktree 根），
`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp5`，
配置：`--locked`，工具链由仓库 `rust-toolchain.toml` 固定。
完整原始输出：**[`reports/wp5-coder-fix-cr5f1-PV1.log`](wp5-coder-fix-cr5f1-PV1.log)**。

| Check ID | 命令 | 结果 | 退出码 | 日志位置 |
| --- | --- | --- | --- | --- |
| **PV1**（本 WP 唯一计划检查，三条子命令） | 见下三行 | **PASS** | 见下三行 | `reports/wp5-coder-fix-cr5f1-PV1.log` |
| PV1-1 fmt | `cargo fmt --all -- --check` | **PASS** | 0 | 同上日志 `### PV1-1` |
| PV1-2 clippy | `cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings` | **PASS**（0 warning；`-D warnings` 下含新测试代码） | 0 | 同上日志 `### PV1-2` |
| PV1-3 test | `cargo test --locked -p agent-host --all-features` | **PASS**：逐目标 `5 + 5 + 22 + 19 + 15 + 1 = 67 passed`，0 failed / 0 ignored | 0 | 同上日志 `### PV1-3` |

逐目标明细（与上一轮基线 62 的差异即本轮新增 5 个 bin 单测）：
`src/lib.rs` 5（原 5，含 `resume_params_carry_the_persisted_values_verbatim`）、
`src/bin/acpr-fake-acp-agent.rs` **5（原 0）**、`tests/catalog.rs` 22、`tests/session.rs` 19、
`tests/supervision.rs` 15、`tests/view_contract.rs` 1、doc-tests 0。

未因上游缺失而红的项：本轮 PV1 三条**全绿**，无需区分「上游未包含」造成的失败。

## 未执行项与原因

| 项 | 原因 |
| --- | --- |
| PV1 阶段 2（`cargo fmt/clippy/test --workspace` 全量）、`npm run check`、`npm run verify` | 属候选/主分支门禁，WP6 收口前已登记为红窗口；本 WP 的 Verification 列只含阶段 1。派发明确本轮不靠钩子判定 |
| `cargo-deny`、`gitleaks` | CI 专属，本地无等价物；本轮未改任何依赖（`crates/agent-host/Cargo.toml` 逐字节未变），未触碰密钥 |
| E2E / 集成用例、驱动脚本、测试数据 | 属 TP2 职责，本角色明令不得代写 |
| `git worktree` 创建/切换/清理、`npm install/ci/update` | 派发禁止（`node_modules` 是指向主仓库的 junction，会穿透破坏其余 worktree） |

## 资源释放

- `CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp5`：分配给本 WP 的独立目录，
  **保留**（供 reviewer 复核时复用），未删除共享根 `D:/Project/acp-remote-target`。
- 测试期间产生的临时文件由各单测的 `TempPath`（`Drop` 时 `remove_file`）清理；
  PV1 全绿后无残留断言失败。
- 本轮唯一的仓库外写入是两个报告/日志文件（`reports/wp5-coder-fix-cr5f1.md`、
  `reports/wp5-coder-fix-cr5f1-PV1.log`），均在被 `.gitignore` 排除的 `reports/**/*.log` 约定位置或其旁。
- 未启动任何后台/常驻进程；未创建分支以外的 git 引用。

## 依赖包含关系

- 上游：WP1（`acp-protocol` 的 `SessionResumeRequest/Response`）、WP3（`core` 的 `ResumeSessionRequest`
  与 `SessionEndpoint::agent_session_id`）已在基线 `4e53fcf` 内 ⇒ 本轮可独立开工。
- 本轮**不改动**任何被上游拥有的接口或签名，纯粹在 `agent-host` 的测试用 fake child 上增加一个可观察通道。
- 下游：TP2 的 SR-R8-1 / SR-R26-2 现在有了字面可实现的读取通道（`--dump-request-params` 的
  `params.sessionId` / `params.cwd`）。

## 与检视/测试报告的对应关系

- 本轮修复对象：**CR5-F1（MINOR）**，修法为派发指定的主 Agent 裁定方案 (a)。
- CR5-F1 的 Recommendation 同时指出「若需要线级证据…只在 plan/tasks 补一句新选项」——
  **本角色无权改 `plan.md` / `tasks.md`**，`tasks.md:27` 的冻结措辞目前仍只写 `--dump-requests`，
  新选项**尚未**进入权威文本。这需要主 Agent 补写。
- 顺带闭环：**CR5-F3（SUGGESTION）** 已按建议补注释。
- 未处理且本轮不在范围内：CR5-F2（TP1 用例前提文字，由 main 改 TP1 设计与 `verification.md`）、
  CR5-F4（同 Agent 并发 create/resume 的窄竞态取舍记录，由 main 裁决）。
- **本轮自测不构成独立证据**：3.6 的独立只读检视**尚未执行**，须由新的独立 reviewer 复核本提交。

## 待澄清问题

1. **`plan.md` / `tasks.md` 的冻结措辞需主 Agent 补一句新选项**（`tasks.md:27` 现只写 `--dump-requests`
   「每次收到的带 `method` 的入站报文按行追加 method 名」）。本轮按契约不修改这两个文件，
   因此**门禁视角**上新选项目前在权威文本里没有登记项——请主 Agent 决定是否要在合入候选前补写，
   以及是否需要同步 `verification.md` 里 PV1 阶段 2 的期望计数（本轮后 `agent-host` 单测为 67）。
2. **`params` 缺失写 `null` 的口径**（而非省略该键）是我的实现选择，理由是让每行键集合恒为
   `{method, params}`、TP2 读起来无需分支。若 main/TP2 期望的是「无 params 时省略键」，
   请指派一处一行改动（该行为已被 `dump_request_params_keeps_method_and_params_per_line` 钉住）。
3. **`Args::parse` → `Args::from_argv` 的拆分**是为可测性做的最小重构（程序名由 `parse` 侧 `skip(1)` 剔除，
   循环体逐字未变）。若 main 认为这超出「加一个选项」的字面范围，可回退为在测试里直接构造 `Args`
   （会失去对解析分支的覆盖）。目前判断是拆分更小且更有判别力。

## handoff_index

```yaml
handoff_index:
  - task_id: "2.5"
    work_package: WP5
    role: coder
    phase: fix
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "1376e1b5c5bbd66d022a447e92a3179e50f173fd"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/session-resume/reports/wp5-coder-fix-cr5f1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 base 4e53fcf 之上、目标提交 1376e1b 上于 worktree D:/Project/acp-remote-wt/session-resume-wp5 实跑 PV1 阶段 1（cargo fmt --check / cargo clippy --locked -p agent-host --all-targets --all-features -D warnings / cargo test --locked -p agent-host --all-features），CARGO_TARGET_DIR 独立，三条 EXIT 均为 0，67 passed / 0 failed / 0 ignored；原始输出见 reports/wp5-coder-fix-cr5f1-PV1.log"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    work_package: WP5
    role: coder
    phase: fix
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "1376e1b5c5bbd66d022a447e92a3179e50f173fd"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/session-resume/reports/wp5-coder-fix-cr5f1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "修复交付提交 1376e1b（base 4e53fcf）：新增 --dump-request-params 选项 + 5 个同文件单元测试，顺带闭环 CR5-F3 注释；改动限定在 crates/agent-host/ 的 2 个文件，正常路径无 unwrap/expect/panic"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.6"
    work_package: WP5
    role: coder
    phase: fix
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "1376e1b5c5bbd66d022a447e92a3179e50f173fd"
    evidence_type: REVIEW
    evidence_id: "CR5"
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "任务 3.6 的独立只读检视尚未在本目标版本上执行；CR5 Round 1 的旧报告绑定的是 4e53fcf，不能覆盖 1376e1b。待补：由新的独立 reviewer 在 target 1376e1b 上执行 CR5 复核轮；本行的 PV1 自测证据不得当作其结论"
    source_evidence: NOT_APPLICABLE
```
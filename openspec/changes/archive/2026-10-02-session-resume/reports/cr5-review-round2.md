# CR5 独立检视报告（WP5 / branch / work-package / Round 2 · 复核轮）

> 持久化说明：本报告由 reviewer 实例（只读工具集）以全文返回、由主 Agent 原样落盘。run `a72214f4-09ed-49e0-97e7-a3c95839cd6e`。

## Shared Report

| 字段 | 值 |
| --- | --- |
| `task_id` | `3.6`（WP5 独立检视） |
| `role` | `reviewer` |
| `phase` | `branch`（复核轮沿用原类型） |
| `agent_context` | 子 Agent `reviewer-E2`，run `a72214f4-09ed-49e0-97e7-a3c95839cd6e`（该 ID 取自 main 写入 `verification.md:45` 的登记行）。**实际隔离方式**：本实例为本线程**新建**子 Agent，只收到本轮派发全文 + 只读工具集（read/grep/find/ls/工作区 diff），**未**继承 Round 1 报告作者的对话、**未**参与 `coder-E`/`coder-E2` 的实现或实现讨论。我能自证的只有「本回合只读到派发文本与仓库内容」；宿主是否注入了其它上下文不由我证明。 |
| `work_package` | `WP5` |
| `stage` | `work-package` |
| `base_revision` | `4e53fcf9fcd9eb06130f3cab8de7d21e7973cf63` |
| `target_revision` | `1376e1b5c5bbd66d022a447e92a3179e50f173fd` |
| `scope` | 复核 Round 1 的 CR5-F1（阻断结案项）与 CR5-F3（附带措辞项）；顺带说明 CR5-F2/CR5-F4 是否变好或变坏（不翻案）；不判「用例集合齐不齐」「测试是否充分」 |
| `changes` | 无（只读）。未修改任何文件、未提交、未切换分支、未执行 `cargo`/`npm`/E2E |
| `checks` | 见「Check 证据核对」与「我实际做的检查」；Project Verify 由 coder 负责，本轮不重跑 |
| `issues` | 2×SUGGESTION（CR5-F5、CR5-F6，均非阻断）；CR5-F1/CR5-F3 已闭环 |
| `result` | **PASS**（0×CRITICAL / 0×MAJOR） |
| `evidence_paths` | 本报告；被读取的实现者证据 `openspec/changes/session-resume/reports/wp5-coder-fix-cr5f1.md`、`reports/wp5-coder-fix-cr5f1-PV1.log`；首轮 `reports/cr5-review.md` |
| `resource_cleanup` | NOT_APPLICABLE（未申请任何资源；`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp5` 由 coder 保留，本轮未触碰） |

## Review Context

| Inputs 项 | 本轮实际取值 |
| --- | --- |
| Review ID / Round | **CR5 / 2**（复核轮，沿用原线程 ID 与原类型 `branch`；上一轮 `CR5 / 1`，Target `4e53fcf`） |
| Review Type | `branch`（复核同一线程） |
| Review Stage | 工作包编写阶段 · 修复轮复核 |
| Work Package | `WP5`（`crates/agent-host` 会话恢复后端路径） |
| Repository | `D:/Project/acp-remote-wt/session-resume-wp5`（worktree，工作区干净，见下） |
| Base Revision | `4e53fcf` |
| Target Revision | `1376e1b` |
| 读取的规则 | `openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md` 全文；`AGENTS.md` §3/§4/§7/§9/§10（随派发提供） |
| 读取的需求/契约 | `tasks.md:27`（WP5 义务全文，含 main 已补登的新选项）、`plan.md:122`（Contract Changes·CR5-F1 行）与 `plan.md:327`（WP5 行）、`verification.md:44-45`（证据登记）与 `:218-220`（Review Findings）、`reports/tp1-test-design.md:205-207 / :254-256`（SR-R8-1、SR-R26-2 的读取口径） |
| 使用的验证证据 | `reports/wp5-coder-fix-cr5f1.md`、`reports/wp5-coder-fix-cr5f1-PV1.log`（**实现者跑的，不是我的证据**；我只用它与源码内容做交叉比对） |
| 核对的 Check ID | `PV1`（本 WP 唯一计划检查，阶段 1 = `-p agent-host` 三条子命令）。PV1 阶段 2 / PV2 / `cargo-deny` / `gitleaks` / E2E 不在本轮判定范围 |
| 本轮报告路径 | `openspec/changes/session-resume/reports/cr5-review-round2.md` |
| 限制 | **本实例无 shell/git**：无法执行 `git --no-pager diff 4e53fcf..1376e1b`，也无法 `git rev-parse`／`git show`。我用「工作区相对 reviewer-launch HEAD 的状态 + 目标版本内容通读 + 行号位移分析 + 文件集合与依赖清单比对」重建改动面（方法逐条写在「我实际做的检查」） |

### 版本稳定性

工作区相对 reviewer-launch `HEAD 1376e1b5c5bb` 的 delta 为**空**（无暂存/未暂存改动、无未跟踪路径），即我读到的文件内容就是 Target Revision 的内容，不存在「读到不断变化的工作区」。

## 我实际做的检查（逐条可复核）

1. **工作区状态**：`watchdog_diff` → 无改动（版本已固定在 `1376e1b`）。
2. **通读目标版本全文**：`crates/agent-host/src/bin/acpr-fake-acp-agent.rs`（1–1002 行，含新增 `#[cfg(test)] mod tests`）。
3. **`host.rs` 定点核查**：读 `resume()` 全文（600–680）、`session_new_params`/`session_resume_params`（685–726）；并对全文件做符号行号图，与 Round 1 报告的行号引用逐一对照。
4. **既有 `--dump-requests` 消费方**：`crates/agent-host/tests/session.rs:102-108`（`dumped_methods` 逐行取 method）、`:963`、`:1032`、`:1115` 三处传参——确认**未**被本轮改动，且其语义假设（每行只有 method）与新实现一致。
5. **新选项的全仓出现面**：worktree 内 `dump-request-params|dump_request_params` 只命中 `crates/agent-host/src/bin/acpr-fake-acp-agent.rs`（17/40/62/81/192/202/205-206/889-997），未渗入其它 crate、文档或 `openspec/`。
6. **无新增/删除源文件**：`crates/**/*.rs` 文件清单在 wp5（target）与 wp6（基线等价 worktree `8a08db8`）**逐项相同**。
7. **无新增依赖**：`crates/agent-host/Cargo.toml` 在 wp5 与 wp6 **逐字节相同**；新代码只用既有 `serde_json`（bin 文件 `:30` 已 `use serde_json::{Value, json};`），单测自建 `TempPath`，未引入 `tempfile` 等新 dev-dep。
8. **行号位移分析（替代 `git diff` 的关键手段）**：
   - `insert_session` 仍在 `:120`、`ensure_runtime_tracked` `:215`、`evict_binding` `:280`、`discard_spawned_runtime` `:303`（与 Round 1 引用**逐项相同** ⇒ `resume` 之前零改动）；
   - `session_resume_params` `:719`（Round 1 记 `:717-725`）、内联单测 `:827`（Round 1 记 `:825-840`）——`resume` 之后**恰好 +2 行**；
   - 这 +2 行在目标版本里可直接读到，就是 `:624-625` 的两行 `//` 注释。
   ⇒ **`host.rs` 的改动就是这两行注释**，净增 2 行、位置在 `resume` 内、其余符号位移全部对得上。
9. **PV1 日志与源码交叉比对**（不重跑）：`wp5-coder-fix-cr5f1-PV1.log` 中 `Running unittests src\bin\acpr-fake-acp-agent.rs … running 5 tests` 及 5 个测试名（`option_parsing_keeps_following_switches`、`without_options_nothing_is_written`、`a_single_option_creates_only_its_own_file`、`dump_request_params_keeps_method_and_params_per_line`、`both_dump_options_coexist_in_independent_files`）与我在源码 `:889/:925/:952/:965/:984` 读到的**逐字一致**；三条子命令 EXIT=0，逐目标 `5+5+22+19+15+1=67`。这是**实现者证据**；我只用它证明「日志对当前内容成立」，不据此判定代码正确。

## 逐条复核判定

### 1) CR5-F1（MINOR，阻断结案）—— **已解决**

主 Agent 裁定的三条硬约束，我逐条核对：

| 裁定条款 | 核对结果 | 依据 |
| --- | --- | --- |
| (1) 新增**并列**选项，每条带 `method` 的入站报文**追加一行单行 JSON**，含 `method` 与 `params` | **满足** | `bin:205-215` `dump_request_params`：`json!({"method":method, "params": params 或 null})` → `to_string()` → `bin:217-226` `append_line`（`OpenOptions(create+append)` + `writeln!`）。`serde_json` 会把字符串内的换行转义，**行式读取不会被 payload 里的换行打断** |
| 触发条件与 `--dump-requests` **完全相同** | **满足** | `bin:152-153`：`if let Some(method) = message.get("method").and_then(Value::as_str) { dump_inbound(&args, method, message.get("params")); }`——`dump_inbound`（`:190-193`）先调 `dump_method` 再调 `dump_request_params`，两者共用同一个 `method`。（coder 报告写触发点 `:152`，实际调用在 `:153`、`if let` 在 `:152`，属引用精度问题，不影响判定） |
| (2) **不得**改变 `--dump-requests` 既有语义（仍每行只写 method） | **满足** | `bin:196-201` `dump_method` 只做 `append_line(path, method)`，写入的是**裸 method 串**，无 JSON 包装、无 `params`；`append_line` 只是把原函数体里的 `OpenOptions(create+append)+writeln!+静默忽略` 原样抽出，两个选项共用，**写入字节序列与 Round 1 等价** |
| (3) 两选项可同时使用、各写各的文件；都不给时行为与改动前完全一致 | **满足** | 各读各的 `Option<String>` 路径（`:196-201` / `:206-208` 各自早退）；都不给时两个 `if let`/早退都不进入，不 `open` 任何文件 ⇒ 零副作用。单测 `a_single_option_creates_only_its_own_file`、`without_options_nothing_is_written` 直接断言「未给出的选项的文件根本不出现」 |

**新增选项是否真的产出可断言的 `params`（这是 CR5-F1 的实质）** —— 是。TP1 的两条断言现在有了字面可实现的读取通道：

- **SR-R26-2**（`tp1-test-design.md:254-256`：「发送给 Agent 的 `cwd` 就是持久化取值」）→ 读 `--dump-request-params` 文件中 `method == "session/resume"` 那一行的 `params.cwd`，与传入的持久化 cwd **逐字**比较。方法名随行输出，**多方法并存时不需要依赖行序猜测**（`:889-923` 用含中文与斜杠的 `/持久化的/cwd` 验证逐字保真，未做别名替换/路径重解析）。
- **SR-R8-1**（`:205-207`）→ 同一行的 `params.sessionId`。

**既有断言是否仍然成立** —— 成立。`tests/session.rs:102-108` 的 `dumped_methods` 仍按「每行一个 method」读取，三处既有用例的传参与断言语义未变；Round 1 引用的冻结措辞（`tasks.md:27`）仍在，且 main 已把新选项补登进 `tasks.md:27` 与 `plan.md:122`，措辞（「每行一条含 `method` 与 `params` 的 JSON；**不改** `--dump-requests` 的语义」）与实现**逐字一致** ⇒ 契约与实现已对齐，CR7-F5 的冻结语义未被静默改写。

### 2) CR5-F3（SUGGESTION，附带措辞项）—— **已解决，且确为纯注释**

`host.rs:624-625` 新增两行 `//` 注释，位置在 `let capabilities = lock(&runtime.capabilities)…`（`:626`）之前，写明「本次拉起进程时 `initialize` 刚协商过；复用既有进程（`spawned == false`）时读到的是那个进程此前的协商结果——不重新协商，也不虚报能力」。

- **无行为变化**：新增行的字面形态是 `//` 注释，不是语句；`resume()` 的可执行语句序列（`ensure_runtime_tracked` → 读缓存能力 → `supports_session_resume()` 门控 → `session_resume_params` → `supervisor.request("session/resume")` → 类型化解码 → `evict_binding` → `insert_session`）与 Round 1 逐项一致；
- **行号位移可交叉验证**：`resume` 之前的符号行号**零位移**，`resume` 之后的符号**恰好 +2**，净增的两行就是这两条注释（见「检查」第 8 条）⇒ 没有伴随的语句增删。

### 3) CR5-F2（`ensure_runtime_tracked` 复用活进程 vs SR-R12-2 前提）—— **未变好也未变坏，不翻案**

`ensure_runtime_tracked`（`host.rs:215-278`）与 `resume` 的调用点 `:620-621` 本轮**一字未动**（行号零位移）。CR5-F2 的实质仍是「TP1 的 SR-R12-2 前提文字与实现不符」，属 TP1 侧文本问题（main 已按 Round 1 建议记入 TP 派发提示，见 `verification.md:219`）。本轮新增的 dump 选项**不涉及进程复用路径**，既不能让该前提成立，也不加重它。

### 4) CR5-F4（`discard_spawned_runtime` 关闭该 runtime 全部会话）—— **未变好也未变坏，不翻案**

`discard_spawned_runtime`（`host.rs:303-327`）及其在 `resume` 里的三个调用点（`:629` 门控未宣告、`:643` 请求失败、`:653` 解码失败、`:677` 绑定失败）行号全部**零位移** ⇒ 回收语义与并发竞态窗口与 Round 1 完全相同。本轮没有新增任何会触发该路径的代码。

### 5) 对实现者三项待裁决的独立判断

**(1) `params` 缺失写 `null` vs 省略该键 —— 判定：保留 `null`（对 TP2 更稳）。**

- 对 SR-R8-1/SR-R26-2：`session/resume` 的 `params` **必然存在**，两种口径在这两条用例上等价；差别只体现在 `initialize` / `session/cancel` / `session/prompt` 这类可能无参的行。
- `null` 更稳的三点：① 键集合恒为 `{method, params}`，读取侧可无分支地写 `obj["params"]["cwd"]`，不会因「键缺失 → `undefined`」在 JSON 反序列化后表现成缺字段而被误判成「没发 params」；② 它把「报文没有 params」与「params 是空对象 `{}`」区分开，而省略键会把两者混同；③ 结构断言（键集合恰为两键）可以直接写死，不需要 `contains_key` 分支。
- 代价只有一处：读侧若要判断「本次请求是否带参数」须显式比 `null`。这在测试断言里是一行，不构成风险。
- 该选择已被 `dump_request_params_keeps_method_and_params_per_line`（`:917-920`）钉住，将来改动会立刻失败——这正是我们想要的稳定性。

**(2) `Args::parse` 拆出 `Args::from_argv(&[String])` 是否超范围 / 是否引入风险 —— 判定：在范围内，接受。**

- 范围：`Args` 的解析循环体（`:57-95`）与 Round 1 的循环**逐字相同**，改动只是把「取 argv」与「解析 argv」分开：`parse()`（`:51-53`）做 `std::env::args().skip(1).collect()`，语义与原先 `args().skip(1)` 再索引完全等价（仍用 `std::env::args()`，非 `args_os()` ⇒ 非 Unicode 参数的 panic 行为也未变）。净增约 3 行，是为让新选项的解析分支可被单测直接驱动的**最小**手段。
- 风险：无对外 API 变更（`Args` 是 bin 内的私有结构体）、无新增依赖、无行为差异、既有 19 个 `tests/session.rs` 用例与 fake child 的全部 argv 口径不变（`--scenario/--capabilities/--no-modes/--dump-requests/...` 的匹配分支一字未改，`:74-84`）。
- 判别力确有提升：`option_parsing_keeps_following_switches`（`:984-1000`）能证明「路径值不吞掉后面的开关」，这是本轮新增代码里唯一容易写错的交互点。

**(3) 正常路径是否仍无 `unwrap()/expect()/panic!`（`AGENTS.md` §7）—— 判定：是，成立。**

全文件 `unwrap()|expect(|panic!|todo!|unimplemented!|#\[ignore\]` 的命中只有 6 处 `expect`，行号 `905/906/913/918/946` 及其上下文，**全部落在 `#[cfg(test)] mod tests`（`:827` 起）之内**；正常路径（`main` / `Args` / `dump_*` / `handle` / `start_prompt`）零命中。新代码的错误处理沿用既有「静默忽略」口径（`append_line` 的 `if let Ok(..)`），与同文件既有的 `--dump-env` 写入口径一致。

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| CR5-F1 | MINOR（原级别；本轮**非阻断结案项**已闭环） | `crates/agent-host/src/bin/acpr-fake-acp-agent.rs:40,62,81,152-153,190-226`；契约 `tasks.md:27`、`plan.md:122` | 原问题：`--dump-requests` 只写 method，R8/R26 缺线级取值通道。目标版本已新增并列选项并共用同一触发条件；`dump_method`（`:196-201`）仍写裸 method，`append_line`（`:217-226`）是原写入逻辑的原样抽取 | 无。R8/R26 现有字面可实现的读取通道，`--dump-requests` 冻结语义与既有消费方（`tests/session.rs:102-108`）均未变 | 无需动作 | **已解决**（依据：上文「复核判定 1」的逐条核对 + 契约已补登） |
| CR5-F3 | SUGGESTION（原级别） | `crates/agent-host/src/host.rs:624-625` | 目标版本该处是两行 `//` 注释；`resume` 之前符号行号零位移、之后恰好 +2，净增行即这两行 | 无（纯注释，不改行为）；design D4 第 3 条的措辞歧义已消除 | 无需动作 | **已解决**（纯注释已核实） |
| CR5-F2 | MINOR（原级别） | `crates/agent-host/src/host.rs:215-278`（实现）vs `reports/tp1-test-design.md` SR-R12-2 前提 | 本轮未触碰 `ensure_runtime_tracked` 与 `resume` 的调用点（行号零位移） | 无变化；仍是 TP1 用例前提文字问题，非实现缺陷 | 维持 Round 1 建议（改 TP1 前提文字，不改代码） | 未变化，**不翻案** |
| CR5-F4 | SUGGESTION（原级别） | `crates/agent-host/src/host.rs:303-327` 及其 `resume` 内四个调用点 | 本轮未触碰（行号零位移） | 无变化；同一 Agent 并发 create/resume 的窄竞态窗口与 Round 1 相同 | 维持 Round 1 建议（记录取舍） | 未变化，**不翻案** |
| **CR5-F5** | SUGGESTION | `crates/agent-host/src/bin/acpr-fake-acp-agent.rs:152-153`（`main` 的接线）对照 `:889-1000`（5 个单测） | 5 个新单测**全部直接调用 `dump_inbound`**，因此 `main` 里那句 `message.get("params")` 的接线目前**没有任何测试覆盖**。若将来有人把它改成 `dump_inbound(&args, method, None)`，5 个单测仍全绿，而 R8/R26 的线级通道会**静默退化成 `params: null`** | 通道退化的可观察性缺口（当前实现是正确的，故非功能缺陷）；TP2 的 `crates/agent-host/tests/resume.rs` 若按 TP1 设计的字面口径读取 `params.cwd`/`params.sessionId`，会直接暴露该退化 | 不改本轮代码；在 TP2 派发口径里明确要求「SR-R8-1/SR-R26-2 必须经真实子进程 + `--dump-request-params` 读行」，由端到端用例闭合这条接线 | 新发现（本轮） |
| **CR5-F6** | SUGGESTION | `crates/agent-host/src/bin/acpr-fake-acp-agent.rs:832-857`（`TempPath`）、`:960`、`:978-979`（`!exists()` 断言） | `TempPath` 用 `format!("acpr-fake-params-{}-{name}", std::process::id())` 定位，并依赖 `Drop` 删除；`:960/:978/:979` 断言的是「本次运行没创建该文件」。若上一次运行异常退出留下同名残留（Windows pid 复用是常态），`!exists()` 会**假失败**；反向不成立（不会假通过）。另 `lines()`（`:869-876`）在文件不存在时返回空 vec，靠 `written.len() == 2`（`:901`）兜住，不构成恒真 | 只影响本地/CI 的偶发红，不影响产品代码与本轮结论 | 可选加固：构造时先 `remove_file` 忽略错误，或改用仓库既有的 tempdir 支撑；不改也不阻断 | 新发现（本轮） |

> 严重度对照：CRITICAL=P0、MAJOR=P1、MINOR=P2、SUGGESTION=非阻断建议。**本轮 0×CRITICAL、0×MAJOR。**
> 未复用 Round 1 的结论行：CR5-F1/CR5-F3 的判定基于**目标版本内容**，不是基于 coder 自评或 PV1 日志。

## Assessment

### 结论：**PASS**（Target Revision = `1376e1b5c5bbd66d022a447e92a3179e50f173fd`，`stage: work-package`）

CR5-F1 已按主 Agent 裁定的方案 (a) 解决，`--dump-requests` 的冻结语义逐字未变，CR5-F3 为纯注释闭环；改动限定在 `crates/agent-host/` 的两个文件，无新增依赖/新增源文件，正常路径无 `unwrap()/expect()/panic!`。新增的 2 条 SUGGESTION 均不阻断。

### 5 个新增测试的判别力评估（是否恒真/空转）

| 测试 | 判别力 | 依据 |
| --- | --- | --- |
| `dump_request_params_keeps_method_and_params_per_line`（`:889`） | **有** | 断 `written.len()==2`、每行可 `serde_json::from_str`、排序后键集合恰为 `["method","params"]`、`params.sessionId`/`params.cwd` 逐字相等、次行 `params == Value::Null`。任一处退化（不写、写错字段、丢键、params 被省略）都会失败 |
| `both_dump_options_coexist_in_independent_files`（`:925`） | **有** | `assert_eq!(lines(&methods), vec!["session/resume","session/update"])` 直接钉住「`--dump-requests` 仍是裸 method、无 JSON/无 params」——这是 CR5-F1 约束 (2) 的回归网 |
| `a_single_option_creates_only_its_own_file`（`:952`） | **有** | 断言未给出的选项文件不存在（能抓住「无条件写文件」类回归）；残留文件导致的假失败见 CR5-F6 |
| `without_options_nothing_is_written`（`:965`） | **有（弱）** | 断言两个字段均为 `None` 且两个路径都不存在；它验证的是「默认不产生副作用」，本身无法区分「`dump_inbound` 提前早退」与「早退但仍建文件」——后者由上一条覆盖，组合起来无空洞 |
| `option_parsing_keeps_following_switches`（`:984`） | **有** | 证明新选项的路径值**不吞掉**后续开关（`--no-modes` 仍生效），且两字段独立；这是本轮最容易写错的交互点 |

无「断言恒真」写法；无 `#[ignore]`/`todo!`。覆盖面缺口只有一处（`main` 接线，见 CR5-F5），已登记给 TP2 闭合。

### Check 证据核对（PV1 = WP5 的唯一计划检查）

| 检查 | 证据来源 | 我的核对 |
| --- | --- | --- |
| PV1-1 `cargo fmt --all -- --check` | `wp5-coder-fix-cr5f1-PV1.log:10-11`（EXIT=0） | **读取，未重跑**。实现者证据 |
| PV1-2 `cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings` | 同日志 `:13-16`（EXIT=0，`:14` 为真实 `Checking agent-host`，非纯缓存） | **读取，未重跑**。口径与 `plan.md` 的 PV1 阶段 1（`-p <本 WP crate>`）一致 |
| PV1-3 `cargo test --locked -p agent-host --all-features` | 同日志 `:18-130`（EXIT=0；逐目标 `5+5+22+19+15+1=67`，0 failed/0 ignored；bin 目标 0→5） | **读取，未重跑**。我另做了一项独立比对：日志里的 5 个测试名与我在源码 `:889/:925/:952/:965/:984` 读到的**逐字一致** ⇒ 该日志对**当前内容**成立（这不替代独立执行） |
| 本轮不判 | PV1 阶段 2（workspace 全量）、PV2、`cargo-deny`、`gitleaks`、E2E | 分属候选/主分支/CI 专属门禁，由 WP6、merger 与 CI 补齐；**CR5 Round 2 的 PASS 不覆盖它们** |

### 待补证据（不改变本轮代码判断）

| 待补项 | 是否影响本轮判断 | 应在哪个门禁前补齐 |
| --- | --- | --- |
| PV1 阶段 2 / PV2 的候选与主分支执行结果 | 否（本轮判定只覆盖 WP5 crate 子集与静态检视） | 候选合入前（WP6 收口红窗口后） |
| `crates/agent-host/tests/resume.rs` 中 SR-R8-1/SR-R26-2 的**端到端**读取（唯一能闭合 CR5-F5 的证据） | 否（当前实现正确；这是覆盖缺口不是缺陷） | TP2 交付 / validator 阶段 |
| `git diff 4e53fcf..1376e1b` 的原始输出留档（我的替代方法是行号位移 + 内容通读，无法机械证明「恰好两个文件」） | 否（文件集合、依赖清单、全仓出现面三项旁证一致，见「检查」5/6/7） | 主 Agent 在合入候选前的机械核对 |
| `plan.md`/`tasks.md`/`verification.md` 因 CR5-F1 补登而摘要变化 ⇒ Round 13 的规划 PASS 失效、需 Round 14 独立复核（`verification.md:90` 已自记） | 否（属规划线程，与本轮代码判定分离） | 规划复核线程 |

### Residual risks

1. `main` 的 `message.get("params")` 接线无自动化覆盖（CR5-F5）：通道若退化，5 个单测不会报警；依赖 TP2 的端到端用例兜底。
2. `TempPath` 的 pid + 固定名 + `Drop` 清理组合在「上次运行崩溃 + pid 复用」时可能假失败（CR5-F6），只影响本地/CI 偶发红。
3. `--dump-request-params` 会把**每条**入站报文的 `params` 全量落盘（含 `session/prompt` 的大 content block）；行文件体积随 prompt 增大而无上界（`--dump-requests` 只写 method 故无此问题）。对测试用 fake 不构成正确性问题，仅提示 TP2 打开该选项时注意临时目录容量。
4. Round 1 已登记、本轮未变：workspace 编译在 WP6 收口前保持红（已登记、有主、有界）；真实 Codex/OMP 是否宣告 `sessionCapabilities.resume` 属独立验证任务。

## handoff_index

```yaml
handoff_index:
  - task_id: "3.6"
    work_package: WP5
    role: reviewer
    phase: branch
    round: 2
    stage: work-package
    target_revision: "1376e1b5c5bbd66d022a447e92a3179e50f173fd"
    evidence_type: REVIEW
    evidence_id: CR5
    report_path: "openspec/changes/session-resume/reports/cr5-review-round2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: >
      复核轮（Round 2，Review Type 沿用 branch），检视对象为 WP5 修复交付 1376e1b（base 4e53fcf）。
      工作区相对 reviewer-launch HEAD 1376e1b5c5bb 无 delta，读到的内容即目标版本。
      逐条结论：CR5-F1 已解决（新增并列选项 --dump-request-params，bin:40/62/81/152-153/190-226；
      --dump-requests 仍写裸 method，bin:196-201 + append_line:217-226 为原写入逻辑原样抽取；
      tasks.md:27 与 plan.md:122 已补登该选项且措辞与实现一致；tests/session.rs:102-108 的既有消费方未变）；
      CR5-F3 已解决且为纯注释（host.rs:624-625，resume 之前符号行号零位移、之后恰好 +2）；
      CR5-F2/CR5-F4 未变好亦未变坏，不翻案；新增 2×SUGGESTION（CR5-F5 main 接线无测试覆盖、CR5-F6 TempPath 残留假失败）。
      旁证：crates/**/*.rs 文件清单与基线等价 worktree wp6 逐项相同；agent-host/Cargo.toml 与 wp6 逐字节相同（无新增依赖）；
      新选项字符串在全仓只出现在该 bin 文件。0×CRITICAL / 0×MAJOR。
      限制：本实例无 shell/git，未执行 git diff/rev-parse，未重跑 cargo/npm/E2E；
      PV1 为实现者证据，仅用于与源码内容交叉比对，不作为正确性依据。
    source_evidence: NOT_APPLICABLE
```

**明确结论：CR5-F1 已解决。** 依据：(a) 目标版本 `1376e1b` 新增的并列选项 `--dump-request-params` 确实产出可断言的 `params`（`bin:205-215` 写单行 `{"method":…,"params":…}`，含 `session/resume` 的 `sessionId` 与 `cwd` 逐字取值，且方法名随行输出、多方法并存时无需依赖行序）；(b) `--dump-requests` 的语义逐字未变（`dump_method` 只写裸 method，`append_line` 是原写入逻辑的原样抽取，既有 `tests/session.rs` 的三处消费方与 `dumped_methods` 未变）；(c) 两选项各自独立、都未给出时零副作用；(d) 契约已由 main 补登进 `tasks.md:27` 与 `plan.md:122`，措辞与实现一致，冻结语义未被静默改写。

**总结论：PASS（0×CRITICAL / 0×MAJOR），绑定 Target Revision `1376e1b`、`stage: work-package`。** 非阻断项 2 条（CR5-F5、CR5-F6，均为 SUGGESTION）。

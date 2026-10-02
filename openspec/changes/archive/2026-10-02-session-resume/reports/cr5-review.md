> 由主 Agent 从 reviewer 子 Agent 的返回结果原样持久化（reviewer 工具集只读、无 write）；内容未改写。
> 检视线程：CR5 / WP5 / Round 1；run d001a864-fb19-4ac9-8ec6-6b746246cd31

# CR5 独立检视报告（WP5 / branch / work-package / Round 1）

## Review Context

| Inputs 项 | 本轮实际取值 |
| --- | --- |
| Review ID / Round | **CR5 / 1**（首轮；无 Previous Findings，NOT_APPLICABLE） |
| Review Type | `branch` |
| Review Stage | `work-package` |
| Work Package | **WP5**（`agent-host` 创建回填与进程不在时的恢复；R5–R12、R26、R35、R37） |
| Repository | `D:/Project/acp-remote`（本回合只读上下文）；目标版本位于 worktree `D:/Project/acp-remote-wt/session-resume-wp5`（分支 `agentic/session-resume-wp5`） |
| Base Revision | `8a08db8e77ac67efee317363ce241261af05c008`（= provisioner 登记的重指向基线，见 `verification.md:141-142`） |
| Target Revision | `4e53fcf9fcd9eb06130f3cab8de7d21e7973cf63`（完整 SHA 取自 `reports/wp5-coder-PV1.log` 首部与 `wp5-coder.md` 的 `target_revision`；本回合无 shell，未自行 `git rev-parse`，见「限制」） |
| 读取的规则 | `openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md` 全文；`AGENTS.md` §3/§4/§5/§7/§9/§10（随本派发全文提供） |
| 读取的需求/契约 | `design.md` D1–D6（含 D1 的「契约订正」、D2 Round 9、D3 Round 10/11/13、D4 CR7-F1）；`plan.md` 的 WP5 行、`## Coverage Index`、`## Work Packages`、`## Shared File Ownership`、`## Dependency Handoffs`的红窗口段（DR1-F13/F14/F23/F44）、`## Verification Strategy` 的 PV1 阶段定义；`tasks.md` 2.5 全文；`specs/local-agent-host/spec.md`（R5–R12 场景与 Requirement 全文）；`reports/tp1-test-design.md` §2.1/§3.2（SR-R8-1、SR-R12-2、SR-R26-2、§7-Q1） |
| 使用的验证证据（读取，未重跑） | `reports/wp5-coder.md`、`wp5-coder-PV1.log`、`wp5-coder-precommit.log`、`wp5-coder-red-window.log`；旁证 `reports/merge-u1-integrate-wp3-workspace-clippy-keepegoing.log`（基线红窗口的完整清单） |
| Check Plan 核对 | PV1（阶段 1，crate 子集 = `agent-host`）为本 WP 唯一计划检查 → 已核对（见「Check 证据核对」）；PV2 不在 WP5 的 Verification 列（计划把 PV2 给 WP1/WP2/WP3/WP4/WP6），故本轮不判 PV2；PV1 阶段 2（workspace）属候选/主分支门禁，本轮不判 |
| 限制 | 本 reviewer 上下文**无 shell/git 工具**，无法执行派发要求的 `git --no-pager diff 8a08db8e..4e53fcf9`，也无法自行核实 SHA/tree/`git status`。我用「基线等价 worktree 对比 + 全树内容标记 + 文件集合比对」重建改动面（方法见 Assessment）；E2E、`npm run check` 未重跑 |

## 重点核查逐条依据

**1) 能力门控的真实性与顺序（R10 / design D4）** — 成立。
- `crates/agent-host/src/host.rs:615-632`：`ensure_runtime_tracked`（内含 `initialize`，见 `:250-263`）→ `:626` 读 `runtime.capabilities` → `:628-632` `!capabilities.supports_session_resume()` 时 `discard_spawned_runtime(...)` + `Err(Unavailable(BackendUnsupported))`。**发送点在门控之后**（`:634-641`），未宣告路径**一个字节都不发**、**不建绑定**（`evict_binding`/`insert_session` 都在其后）、**不新建会话**（全文件无第二处 `session/new` 调用）、**不伪造成功**（返回 `Err`）。
- 能力取值口径正确：`acp-protocol/src/capability.rs:281-285` 用 `session_capabilities.resume.is_some()`，而 `resume: Option<Value>`（`:458`）使**显式 `null` 与省略同判**，与 spec R10 的「字段省略或为 null」一致。
- 测试断言同时覆盖「未发送」与「进程已回收」：`tests/session.rs:1046-1090`（`--dump-requests` 有 `initialize`、**无** `session/resume`；心跳文件在 400 ms 内不再增长；`!runtime_running`；`open` 仍失败）。**未按「完全不 spawn」判**——doc 注释明确说明门控必然发生在 spawn 之后（`:627-630`）。

**2) 恢复成功路径** — 成立。按 `request.agent` 解析 profile 并复用既有启动路径（`host.rs:621` → `:252-254` `launch::resolve_launch` → `Supervisor::start`，无第二条 spawn 路径）；`cwd`/`sessionId` 只来自 `request`（`:634` → `:717-725` `session_resume_params(&agent_session_id, &workspace_cwd)`），**无 alias、无路径重解析、无补齐**；参数与响应用 WP1 的 `acp_protocol::message::SessionResumeRequest::new` / `SessionResumeResponse`（`acp-protocol/src/message.rs:528-561`、`:576-590`），不是手写 `Value` 提取。

**3) 单绑定与端点** — 成立。`host.rs:280-301` `evict_binding` 按**全目录扫描**找同一 core 会话的既有绑定，先 `close_session()`（停止接受 turn）再 `remove_session`，之后才 `insert_session`（`:661-679`）；`:120-131` 的 `insert_session` 对同一 core 会话重复登记返回 `DuplicateSession`，构成第二道防线。

**4) 创建时暴露标识** — 成立。`host.rs:543-560`：`AgentSessionId::new(&response.session_id).ok()`，取不出合法值即 `None`（**不编造占位**）；`session.rs:149-151` + `:763-765` 把该标识经 `SessionEndpoint` 暴露给 core；`rebind` 继承（`:557`）使重开端点读回同一标识。

**5) fake ACP Agent 义务（CR7-F5 冻结措辞）** — 一致。`bin/acpr-fake-acp-agent.rs:338-366`（`session/resume` 分支 + `resume-ok`/`resume-error`）、`:229-239`（`session-new-error`）、`:34/51/69/139-141/175-187`（`--dump-requests`）。dump 语义**恰为「每次收到带 `method` 的入站报文追加一行 method」**（append、逐行），与 `tasks.md:27` 与 `plan.md` WP5 行的冻结措辞逐字一致；**未扩展格式**（这正是下面 CR5-F1 的来源）。

**6) 测试覆盖** — 7 个新用例（`tests/session.rs` 13→19、`src` 内联 4→5）逐一对应：创建暴露标识（`:888`）、未取得标识不编造（`:906`）、能力已宣告恢复到可交互（`:950`）、能力未宣告显式不支持且不发送（`:1017`）、Agent 拒绝明确失败（`:1098`）、反复恢复不产生第二个端点（`:1166`），内联单测钉住参数取值与键集合（`host.rs:825-840`）。
- 反向验证的**判别力可由用例本身证成**：若 `discard_spawned_runtime` 早退，`:1017` 用例的「心跳 400 ms 不增长」与「`!runtime_running`」两条断言会失败（子进程仍在写心跳）。实现者自报的具体读数 `left: 9 right: 1` **无独立日志**，我按「用例断言具备判别力」采纳，但不把该读数当已核实证据（见 Assessment 的证据注记）。
- 无 `#[ignore]`/`todo!`/`unimplemented!`（全文件零命中）；新用例与既有 `undeclared_capability_is_refused_without_sending_anything`（基线用例，测 `set_mode`/`set_config` 门控）互不重复。

**7) 边界** — 未越界（以间接方法验证，见 Assessment 的方法说明）：新内容标记只落在 `crates/agent-host/` 的 4 个文件；`docs/` 对 WP5 专有标记零命中；`crates/**/*.rs` 的文件集合与基线等价 worktree（wp6）逐项相同（无新增/删除文件）。

**8) 两条接口事实** — 见 Findings 的 CR5-F1（`--dump-requests` method-only）与 CR5-F2（`resume` 复用活进程 vs SR-R12-2 前提）。

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation |
| --- | --- | --- | --- | --- | --- |
| CR5-F1 | **MINOR**（非阻断） | `crates/agent-host/src/bin/acpr-fake-acp-agent.rs:139-141,175-187`；对照 `reports/tp1-test-design.md` 的 SR-R8-1（§3.2 断言 `params.sessionId`/`params.cwd`）与 SR-R26-2 | `--dump-requests` 只写 method（实现与 `tasks.md:27` 的冻结措辞逐字一致），而 TP1 设计要从该文件读 `params.cwd`/`params.sessionId`。**没有**任何通道能观察到「真的发出去的 `cwd` 等于持久化值」：fake child 只做 pinned-schema 形状校验（`bin:341-352`，`exit(8)`），校验的是「是字符串」而非「等于期望值」 | R26/R8 的**cwd 断言缺少线级可观察通道**；TP2 无法按 SR-R8-1/SR-R26-2 字面实现（只能退化为等价证据链：`host.rs:825-840` 单测钉住「`request.workspace_cwd` → `cwd` 逐字、键集合恰为 `{cwd, sessionId}`」+ `host.rs:634` 的调用点 + 形状校验）。行为本身正确，故不阻断；但若 main 要求 R26 有线级证据，必须回写契约 | 最小修补二选一：**(a)**（推荐，若需要线级证据）在 **WP5 的小范围重开**里新增**并列选项** `--dump-request-params <path>`（每行一条含 `method`+`params` 的 JSON），**不动** `--dump-requests` 的冻结语义 ⇒ 不破坏 CR7-F5，只在 plan/tasks 补一句新选项；**(b)** 保持冻结语义，由 main 裁决「SR-R8-1/SR-R26-2 改用等价证据（内联单测 + 形状校验）」，并在 TP1 设计与 `verification.md` 记录该偏离。不建议直接把 `--dump-requests` 改成 method+params（会静默改写已冻结的选项语义） |
| CR5-F2 | **MINOR**（非阻断） | `crates/agent-host/src/host.rs:215-278`（`ensure_runtime_tracked` 复用活进程）vs `reports/tp1-test-design.md` SR-R12-2 前提（`:248-251`「新进程 P2 + 心跳文件 h2」） | 进程仍活着时 `ensure_runtime_tracked` 返回既有 runtime（`:244-247`，`spawned=false`），因此**不会**出现第二个进程/第二个心跳文件；SR-R12-2 的前提不成立 | **R12 的保证不受影响**：单绑定（`evict_binding` 先让出）已由 `tests/session.rs:1166-1206` 覆盖（旧端点 `prompt` 报 `InvalidRequest`、旧收集器零事件、新端点完成 turn）。受影响的是 TP1 的**用例前提文字**，而非实现 | 由 main 把 SR-R12-2 的前提改为「进程复用、单一心跳文件」，断言改为「恰好一个可派发端点 + `session/resume` 恰好一行 + 只有一个进程」——**不需要改代码**；同时建议把 `bin/acpr-fake-acp-agent.rs:12-13` 的「`resume-ok`（已宣告能力…）」注释改为「`resume-ok` = 恢复成功分支；能力须由 `--capabilities` 传入」，避免 TP2 误以为场景名自带宣告（`:229-239`、`:353-364` 中只有 `resume-error` 会改变行为） |
| CR5-F3 | SUGGESTION | `crates/agent-host/src/host.rs:621-626` | 复用既有 runtime 时不会重跑 `initialize`，门控读到的是该 runtime **此前**协商并缓存的 `runtime.capabilities`（`:250-259` 写入） | 语义上仍属「协商到的能力」，不虚报；但 design D4 第 3 条写「`initialize` 后读取协商能力」，未来读者可能误以为每次 `resume` 都重新协商 | 在 `resume` 的注释里补一句「复用既有进程时读到的是该进程既有协商结果」，不需要改代码 |
| CR5-F4 | SUGGESTION | `crates/agent-host/src/host.rs:303-327`（`discard_spawned_runtime` 关闭该 runtime 的**全部**会话） | `spawned=true` 时该 runtime 刚由本次调用创建；若另一并发任务（同 Agent 的另一 core 会话）在此期间已 `insert_session`，失败恢复会把它的绑定一并 `close_session` 并结束整棵树 | 只在「同一 Agent 并发 create/resume」的窄竞态下可达；无数据损坏（core 已有持久化与后续 `open`/`resume` 路径），且与既有「回收/reclaim 时关闭整台 runtime 会话」的写法一致 | 可在回收前加一条「本 runtime 内是否已有非本次恢复的绑定」判断，或明确记录该取舍；**不阻断**。（同类既有现象：`open` 只 `remove_session` 不 `close_session`（基线同形，见 wp4 `host.rs:514-522`），严格说旧端点仍可派发；该处**不在本 diff 的改动面内**，仅作后续观察项） |

> 严重度对照本派发口径：CRITICAL=P0、MAJOR=P1、MINOR=P2、SUGGESTION=非阻断建议。**无 CRITICAL/MAJOR。**

## Assessment

### 结论：**PASS**（Target Revision = `4e53fcf9fcd9eb06130f3cab8de7d21e7973cf63`，`stage: work-package`）

WP5 的实测行为与 `specs/local-agent-host/spec.md`（R5–R12）、`design.md` D2/D3/D4 的可观察保证一致，改动限定在 `crates/agent-host/`，无新增依赖，正常路径无 `unwrap/expect/panic`（`src/session.rs`、`src/bin/**` 零命中；`src/host.rs` 仅测试代码内使用 `expect`）。以下为逐项核对与我使用的验证方法。

### Check 证据核对（PV1 = WP5 的唯一计划检查）

| 检查 | 命令（cwd = worktree 根，`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp5`） | 日志声明 | 我的核对 |
| --- | --- | --- | --- |
| PV1-1 | `cargo fmt --all -- --check` | EXIT=0 | 与 `wp5-coder-PV1.log`「### 1) … EXIT=0」一致 |
| PV1-2 | `cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings` | EXIT=0，无 warning | 日志一致（`Finished … 0.15s` 为**命中缓存**：指纹相同才可能 up-to-date；同一内容在 pre-commit 第 3 步被实际 `Checking agent-host`）；口径与 `plan.md` 的 PV1 阶段 1（`-p <本 WP crate>`）一致 |
| PV1-3 | `cargo test --locked -p agent-host --all-features` | EXIT=0；逐目标 `5 + 0 + 22 + 19 + 15 + 1 = 62 passed`，0 failed/0 ignored | **逐测试计数与日志逐条相符**；基线为 55（4+0+22+13+15+1，wp4 的 `tests/session.rs` 13 个用例实测计数），差值 7 = 本 WP 新增用例数 |
| 附加 | `node scripts/pre-commit.mjs` | fmt PASS、`npm run check` PASS、workspace clippy **101** | 日志三步齐全；`npm run check` 各门禁计数（13 命令/58 错误码/11 feature/17 schema/159 fixture/12 crate/合同漂移/文档引用/agentic）与报告一致 |
| 附加 | `--no-verify` 理由 | 仅因已登记红窗口 | **成立**：`wp5-coder-precommit.log` 唯一失败步骤是 workspace clippy；`wp5-coder-red-window.log:196-265` 的 **9 条**诊断全部落在 `storage-sqlite`(WP4) 与 `server`(WP6)，`agent-host` 零诊断；基线完整清单（`merge-u1-integrate-wp3-workspace-clippy-keepegoing.log:15-105`）为 **11 条** = 本 WP 的 9 条 + 恰好被本次消除的 2 条 `agent-host`（`host.rs:449` 的 `resume`、`session.rs:746` 的 `agent_session_id`）。⇒ **WP5 消除了 2 条、未新增任何 workspace 诊断**，与 `plan.md` 的红窗口段/DR1-F23 条款完全吻合 |
| 未跑（不计入通过） | PV1 阶段 2（`npm run check:rust` 全量）、PV2、`cargo-deny`、`gitleaks`、E2E | — | 阶段 2 在 WP6(W4) 收口前**预期为红**（已登记且有主）；PV2 不在 WP5 的 Verification 列；`--locked -p agent-host` 成功即证明 `Cargo.lock` 与 `Cargo.toml` 自洽，且本 crate 的 `Cargo.toml` 与基线**逐字节相同**（与 wp4 对比），故无新增外部依赖 |

### 我使用的替代核对方法（无 git 的补偿，及由此产生的边界）

1. **基线等价 worktree**：`session-resume-wp4`/`session-resume-wp6` 均从同一基线 `8a08db8e` 开工且不写 `agent-host`，实测二者 `crates/agent-host` 无 `resume`/`agent_session_id`/`dump_requests` 等 WP5 标记 ⇒ 可作 base 内容的对照物。
2. **文件集合比对**：`crates/**/*.rs` 的清单在 wp6（基线）与 wp5（目标）**逐项相同**（无新增/删除文件）；WP5 未新增 `tests/resume.rs` 之类文件（新增用例文件属 TP2）。
3. **内容标记全树 grep**：`dump-requests|resume-ok|resume-error|session-new-error|ensure_runtime_tracked|evict_binding|discard_spawned_runtime` 只命中 `crates/agent-host/` 的 `src/host.rs`、`src/session.rs`、`src/bin/acpr-fake-acp-agent.rs`、`tests/session.rs`；`docs/` 零命中；`crates/` 内另一处 `resume-ok` 属 WP3 基线（wp4 同样存在，非本 WP 产物）。
4. **既有路径回归**：`ensure_runtime` 的转写与基线逐行等价（仅 `Ok(runtime)`→`Ok((runtime, …))` 与注释差异），既有调用点零改动。
5. **残留的不确定**：`git diff --name-only` 的「恰好 4 个文件」、`HEAD == 4e53fcf9`、`8b3a743^{tree} == 4e53fcf9^{tree}`、`git status --porcelain` 为空，这四项**只有实现者报告的文字**、日志里没有原始输出。可复核的部分是：PV1 日志列出的 19+5 个测试名与当前交付内容**逐条一致**，说明该证据对**当前内容**成立（内容等价地支撑「同 tree 复用证据」的结论），但 tree 相等的机械核对仍建议由 main 在门禁前补一次原始输出留档。
6. **实现者自报的反向实验**（临时让 `discard_spawned_runtime` 早退造成 R10 用例失败）**无独立日志**；我不据此加分，只按用例断言的判别力（心跳不增长 + `runtime_running == false`）认定其具备可证伪性。

### 待补/未覆盖证据（不改变本轮代码判断）

- PV1 阶段 2 / PV2 的候选与主分支门禁：由 WP6、merger 补齐（红窗口收口后）。CR5 的 PASS **不**覆盖它们。
- `cargo-deny`、`gitleaks`：CI 专属，本地无等价物；本 WP 未引入依赖、未触碰密钥。
- 真实 Codex/OMP 是否宣告 `sessionCapabilities.resume`：属独立验证任务（风险已登记），本 WP 只按 fake Agent 断言门控逻辑。

### Residual risks

1. R26/R8 的**线级 cwd 证据**目前不存在；若 main 坚持线级证据，需按 CR5-F1 方案 (a) 小范围重开 WP5（新增并列选项）并回写任务文本。
2. TP1 的 SR-R12-2 前提与 SR-R8-1/SR-R26-2 的读取方式需按 CR5-F1/F2 调整，否则 TP2 会写出必然失败的用例。
3. 同一 Agent 上并发 create/resume 的窄竞态（CR5-F4），无数据损坏，建议记录取舍。
4. workspace 编译在 WP6 收口前保持红（已登记、有主、有界）。

### handoff_index

```
role: reviewer, phase: branch, stage: work-package, round: 1, target_revision: 4e53fcf9fcd9eb06130f3cab8de7d21e7973cf63, evidence_type: REVIEW, evidence_id: CR5, result: PASS, evidence_status: NEW
```
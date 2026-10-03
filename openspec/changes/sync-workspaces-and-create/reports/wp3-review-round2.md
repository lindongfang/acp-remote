# WP3 独立检视报告（Round 2）

- **检视对象**：worktree `D:\Project\acp-remote-wt\wp3-core`，分支 `feat/sync-core-projection`，HEAD = `347399f45a3df3093c9f45c3f98769ba96f846c8`
- **基线**：`6c093f145aa3d69dcd573a6d94e31b692acb5d4b`；父提交 `360361288d3c8e028219d73c77a8db28a4c2639e`
- **隔离方式**：独立 reviewer（`reviewer-core-2`），不继承实现对话；全程对 worktree 只读（`git status --porcelain` 空输出，含 `--untracked-files=all`）；变异测试在 `D:\Project\acp-remote-wt\scratch-wp3-r2`（worktree 之外）进行并已逐个还原；仅新建本报告
- **构建目录**：`CARGO_TARGET_DIR=D:\Project\acp-remote-wt\target-wp3-review2`（变异用 `…-target-wp3-r2-mut` / `…-clippyws` / `…-wstest`，均与实现方隔离）
- **前轮报告**：`reports/wp3-review.md`（Round 1 = `reviewer-core`，PASS + F1 裁定事项 + F2/F3/F4）

## 判定

**PASS（无阻断项；三条 Low 修复全部到位且被变异测试证明非空洞；F1 的文档/spec 一致性经独立核对成立）**

Round 1 的十条判据逐条复核后仍然成立；`347399f` 只动了两个文件、只做了三件事（删空断言 / 修注释 / 补值对象用例），没有触及任何行为、wire 形状或契约语句。六条命令的真实输出见 §4；`crates/storage-sqlite` 的红灯**恰好**是登记在案的 WP4 窗口，错误集合与 Round 1 逐字一致、无新增。

发现 3 项 Low / Info 级事项，均不阻断合入（见 §3）。

## 1. `347399f` 的文件枚举与范围核对

```
$ git show --stat 347399f
 crates/core/src/model/ids.rs   |  4 ++--
 crates/core/src/model/tests.rs | 41 +++++++++++++++++++++++++++++++++++++----
 2 files changed, 39 insertions(+), 6 deletions(-)
```

| 文件 | WP3 申报写范围 | 判定 |
|---|---|---|
| `crates/core/src/model/ids.rs` | 是（`WorkspaceRef` 所在文件） | 在范围内；本提交只改 `try_new` 的 **doc comment 两行**，实现体逐字未动 |
| `crates/core/src/model/tests.rs` | 否（Round 1 已受理的越界） | 在范围内；删 4 行空断言 + 加 36 行新用例 + 保留 1 行 `.summary(None)` 调用点修复 |

`git diff --name-only 6c093f1..HEAD` 全量 10 个文件，与 Round 1 §1 的枚举**逐项相同**（`crates/core/src/{broker.rs, model/ids.rs, model/session.rs, model/tests.rs, ports.rs, use_cases.rs}`、`crates/server/src/node_link/{command,resource}/tests.rs`、`docs/CORE_PORTS_AND_STORAGE.md`、`docs/MODULE_ARCHITECTURE.md`）。`crates/app/**`、`crates/storage-sqlite/**`、协议 crate、`schemas/`、`scripts/`、`fixtures/` **均不出现**。

**行为/契约面变化核对**：整个 `347399f` diff 中没有任何一条可执行语句被改写——`ids.rs` 改的两行都在 `///` 注释里，`tests.rs` 改的一处是删除断言、一处是新增 `#[test] fn`。无新增/删除端口方法、无 wire 字段、无 DDL、无文档版本号变动（`CORE_PORTS_AND_STORAGE.md` 未出现在本提交的文件列表里）。**符合「只做三处修复」的申报。**

## 2. 逐条验收

### 2.1 F2 — 空断言已删，必要的调用点修复仍在 ✅

- 空断言的**文本**在全仓已不存在：`grep -rn "聚合不带目录归属" crates` → 无命中。
- 调用点修复仍在：`crates/core/src/model/tests.rs:543`
  `let summary = local_session(SessionState::Running, None).summary(None);`
  ——这正是 `Session::summary(Option<WorkspaceRef>)` 新签名下 core 内部唯一必须改的调用点（其余三处 `.summary(...)` 在 `broker.rs:4893/5105/5171`，都已在 `3603612` 内改为传 `workspace_ref(...)`）。
- core 确实编译且全绿：`cargo test --locked -p core` → **141 passed / 0 failed**（Round 1 为 140，净 +1 即新增的 `WorkspaceRef` 用例）。

### 2.2 F3 — 注释不再声称 NUL 校验，且**未**新增 NUL 校验 ✅

注释现状（`crates/core/src/model/ids.rs:436-437`）：

```
/// 构造。`display_name` 非空、≤128 字符（与 `WorkspaceRecord::display_name` 同口径，§3.7：
/// 仓库对「展示名」类字段只做字符数校验，NUL 校验只施加于路径与命令行类字段）。
```

实现体（`ids.rs:438-444`）仍只有 `require_bounded(display_name, 1, 128)?;`，`git diff` 确认实现行零改动。

**未新增 NUL 校验的独立证据**：`grep -rn no_nul crates/core/src` 的全部命中只有 5 处——`config.rs:36`（定义）、`config.rs:116/121`（`command`/`args`）、`config.rs:227`（`WorkspaceRecord.canonical_path`）、`config.rs:438`（`ResolvedWorkspace.canonical_path`）、`ids.rs:201`（`check_agent_session_id`）、`backend.rs:207`（`ResumeSessionRequest.workspace_cwd`）。**`WorkspaceRef` 不在其中**。

**与仓库既有口径同源**：`WorkspaceRecord::try_new`（`config.rs:225`）、`AgentRef::try_new`（`ids.rs:355`）、`ExportTemplate::try_new`（`export.rs:178`）、`WorkspaceAliasEntry` 均是裸 `require_bounded(_, 1, 128)`，无一查 NUL。`WorkspaceRef` 现在与之完全一致——这正是 Round 1 F3「不要真的加 NUL 校验」所要求的处置，**做对了**。

### 2.3 F4 — `WorkspaceRef` 值对象用例已补，且经变异证明非空洞 ✅

用例位置 `crates/core/src/model/tests.rs:2544-2579`，紧邻同为值对象边界用例的 `agent_session_id_is_bounded_and_nul_free`，**风格一致**：`/// §3.1：…` 的契约出处注释 + `#[test]` + `WorkspaceAlias::new(..).expect("alias")` 构造 + `assert_eq!(.., Err(InvalidValue::X))` 的具名错误期望 + 中文断言说明。

覆盖与「是否真实」的逐项判定：

| 断言 | 行 | 是否会被实现变更打红 |
|---|---|---|
| `""` → `Err(InvalidValue::Empty)` | 2556-2559 | **是**（变异 1 杀死） |
| `"r"×129` → `Err(InvalidValue::TooLong { max: 128 })` | 2560-2563 | **是**（变异 3、5 杀死；5 还证明 `max` 字面量本身被钉住） |
| `"r"×128` 构造成功且 `chars().count() == 128` | 2564-2571 | **是**（变异 6 杀死：把存储截断到 127 会红） |
| `"Re\0po"` 构造成功且原文往返 | 2572-2578 | **是**（变异 2 杀死：加 NUL 校验会红） |
| `alias()` 往返 + `as_str()` + `display_name()` 往返 | 2551-2554 | **是**（变异 4 杀死：`alias()` 返回别的别名会红） |

**变异实证**（全部在 `D:\Project\acp-remote-wt\scratch-wp3-r2`，逐个改 `WorkspaceRef::try_new` 后跑 `cargo test --locked -p core workspace_ref`，每次结束都还原并 `diff` 确认与 worktree 逐字相同）：

| # | 变异 | 结果 |
|---|---|---|
| 1 | `require_bounded(display_name, 0, 512)` | **FAILED** @tests.rs:2556 — `left: Ok(WorkspaceRef{…display_name: ""}) / right: Err(Empty)` |
| 2 | 追加 `if display_name.contains('\0') { return Err(InvalidValue::Field) }` | **FAILED** @tests.rs:2575 — `与 WorkspaceRecord 同口径: Field` |
| 3 | `require_bounded(display_name, 1, 130)` | **FAILED** @tests.rs:2560 — `left: Ok(...129 个 r) / right: Err(TooLong { max: 128 })` |
| 4 | `alias()` 返回 `"mutant"` | **FAILED** @tests.rs:2552 — `left: WorkspaceAlias("mutant") / right: WorkspaceAlias("acp-remote")` |
| 5 | `require_bounded(display_name, 1, 127)` | **FAILED** @tests.rs:2560 — `left: Err(TooLong { max: 127 }) / right: Err(TooLong { max: 128 })` |
| 6 | 存储截断 `chars().take(127)` | **FAILED** @tests.rs:2564 — `left: 127 / right: 128` |

**6/6 变异全部被杀死，还原后 `workspace_ref` 用例重新 `ok`**。该用例不是实现的复述，也不是「跑通即算」。

### 2.4 F1 — spec delta 与 `CORE_PORTS_AND_STORAGE.md` 的 `[决定]` 一致性 ✅

裁定内容：三列同门、未取得 ACP 会话标识时三者同为 `NULL`。

- **spec delta**（`openspec/changes/sync-workspaces-and-create/specs/workspace-resolution/spec.md:7`，只读核对）：
  > 「**别名与恢复所需的两列同门写入**：三列同属会话创建的那一次窄写提交，只有本次创建取得 Agent（ACP）侧会话标识时才一并落盘；未取得标识时三列同为 `NULL`……其目录归属同样不落盘、会话呈现为未分组。」
  并新增 Scenario「未取得 ACP 会话标识时三列同为空」，THEN 明写「MUST NOT 只写别名而留下另外两列为空」。
- **合同文档**（`docs/CORE_PORTS_AND_STORAGE.md:391`，`[决定]`）：
  > 「……`workspace_alias` 取**同一次解析**用掉的别名原文 `ResolvedWorkspace::alias()`（三者都不依赖后端回报）；`agent_session_id()` 为 `None` 时**三列都不写**，该会话不被当作可恢复会话，目录归属也就是「未分组`。」

**两个方向都不分歧**：spec 的「同门 / 三列同为 NULL / 不得只写别名」与文档的「三列都不写 / 目录归属即未分组」是同一命题的两种措辞；`347399f` 未改动该文档（不在本提交文件列表内），所以「docs 已如此、fix 未改」的申报属实。实现侧（`broker.rs:1456-1470`）也确为 `if let Some(agent_session_id) = …` 包住同一个 `SessionUpdate` 字面量里的三列。**裁定与实现、文档三方一致。**

### 2.5 Round-1 判据的回归复核

| 判据 | 复核方式 | 结果 |
|---|---|---|
| 别名是唯一投影来源，无路径反查 | 读 `broker.rs:3985-4003`：`FakeWorld::workspace_ref` 首行 `state.workspace_aliases.get(session.as_str())?`，函数体无任何 `workspace_cwd` 读取 | **通过** |
| `NULL` 别名 = 未分组，不按路径回填 | 同上（`None` 短路）+ 端口文档 `ports.rs:401-402` 的 MUST NOT + 行为用例 `a_null_workspace_alias_is_never_backfilled_from_the_canonical_path`（本轮 `ok`） | **通过** |
| 恢复路径既不读也不写别名 | `SessionRecoveryRecord`（`backend.rs:228-233`）结构上无别名字段；`broker.rs:1518-1540` 的 resume 提交里根本没有 `state`（`state: None`），因此结构上不可能改写别名；用例 `resume_session_neither_reads_nor_rewrites_the_workspace_alias` 本轮 `ok` | **通过** |
| `crates/app/**`、`crates/storage-sqlite/**` 未动 | `git diff --name-only` 不含二者 | **通过** |
| Node Link 投影面未扩大 | `grep -rn "workspace()" crates/server/src` → **零命中**；`command.rs:2456-2496` 的 `session_summary()` 与 `resource.rs:1222` 的 `session_meta()` 读的是 `agent/state/origin/current_mode/session_id/title/version/created_at/updated_at`，无 `workspace` | **通过** |
| `§5` 与 `ports.rs` 一致 | `node scripts/check-contract-drift.mjs` → `§5 的 15 个 trait / 96 个方法签名与 crates\core\src\ports.rs 一致`（§5/§7 代码块之外无漂移） | **通过** |
| `WorkspaceRef` 无路径字段 | `ids.rs:429-433` 仅 `alias` + `display_name`；`SessionSummary` 新增项是 `Option<WorkspaceRef>` | **通过** |
| 别名原文 | `broker.rs:1441-1444` `workspace.alias().as_str().to_owned()`，无小写化/规范化改写 | **通过** |

### 2.6 `--no-verify` 例外的披露准确性 ✅（两点保留见 §3 的 F2-R2-01 / F2-R2-02）

**披露存在且属实。** 提交信息正文逐字：

> 「本地钩子未完整执行：pre-commit 第 4 阶段是 `cargo clippy --workspace`，在本分支上因已登记的 WP4 红窗口（`storage-sqlite/src/session_store.rs:454` 的 `E0061`）而结构性不可能通过，已在父提交 `3603612` 上复现，非本次改动引入。gitleaks、`cargo fmt --all --check`、完整 `npm run check` 与 commitlint 已手工跑过并通过；该缺口由候选阶段 PV1 与 CI 关闭。」

逐项独立复核：

1. **「第 4 阶段是 `cargo clippy --workspace`」——属实。** `scripts/pre-commit.mjs:103`：
   `["cargo", ["clippy", "--locked", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"]]`，
   文件头 `:10-13` 的四步说明与之一致。
2. **「因 WP4 红窗口结构性不可能通过」——属实。** 我实跑同一条命令：
   `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` → `EXIT=101`，
   唯一错误 `error[E0061]: this function takes 10 arguments but 9 arguments were supplied` @ `crates/storage-sqlite/src/session_store.rs:454`。
3. **「已在父提交 `3603612` 上复现，非本次改动引入」——属实。** `git show 3603612:crates/core/src/model/session.rs` 中 `summary(&self, workspace: Option<WorkspaceRef>)`（:293）与 `SessionSummary.workspace`（:327/:343）**已经存在**——即破坏工作区编译的签名变更由 `3603612` 引入，`347399f` 未触碰 `session.rs`（不在文件列表）。
4. **「gitleaks 已手工跑过并通过」——属实。** 本机 `gitleaks version` = `8.30.1`（与钩子注释记录的 v8.30.1 官方参数一致）；`gitleaks git --log-opts="3603612..347399f" --redact` → `1 commits scanned … no leaks found`，`EXIT=0`。
5. **「`cargo fmt --all --check` 已手工跑过并通过」——属实。** `cargo fmt --all -- --check` → `EXIT=0`（无输出）。
6. **「完整 `npm run check` 已手工跑过并通过」——属实。** `npm run check` → `EXIT=0`，末尾 `Totals: 19 passed, 0 failed (19 items)`、`agentic 宿主入口检查完成：17 个文件`。
7. **「commitlint 已手工跑过并通过」——属实。** `npx --no -- commitlint --edit <该提交正文>` → `EXIT=0`、无输出。
8. **「未使用其它绕过」——未发现任何其它绕过。** 仓库钩子配置为 `core.hooksPath=.husky/_`，`pre-commit` → `node scripts/pre-commit.mjs`，`commit-msg` → `npx --no -- commitlint --edit`；两条钩子**都会**拒绝本提交（前者因 clippy 阶段 101，后者本可放行）。`347399f` 的树中不含任何 `HUSKY=0` 配置改动（`.husky/_/h` 的 `HUSKY=0` 短路是环境变量机制，仓库内无任何文件设置它），`git config --local` 中亦无与提交绕过相关的条目（只有 `core.hooksPath`）。提交本身是普通 `git commit` 对象（作者/提交者一致，无签名、无 `Signed-off-by` 之类的手写豁免标记）。

**结论：这是一次已披露、已授权、有具体技术理由的单点例外，不是隐藏跳过。** 披露内容在事实层面**逐项属实**。

## 3. 发现（Findings）

本轮无阻断项、无中危项。三项 Low/Info：

### F2-R2-01 — 披露中「由 CI 关闭」在本分支上不成立（措辞不准）

- **严重度**：低（Low；仅提交信息措辞，不影响代码/契约）
- **位置**：提交 `347399f` 的正文末句（「该缺口由候选阶段 PV1 与 CI 关闭」）
- **问题**：CI 的 `checks` job 跑的是 `npm run check:rust`，其定义（`package.json:25`）为
  `cargo fmt --all -- --check && cargo clippy --locked --workspace --all-targets --all-features -- -D warnings && cargo test --locked --workspace --all-features`。
  后两条与本地钩子第 4 阶段**逐字相同**。我实跑 `cargo test --locked --workspace --all-features` → `EXIT=101`，同样是那一个 `E0061`。**即在本分支上 CI 同样是红的，它不能「关闭」这个缺口，只能同样地把它暴露出来**——真正关闭缺口的是 WP4 修好 `session_store.rs:454`（以及 `tests/` 下 15 处潜伏字面量）。
- **影响**：无功能影响。风险在于措辞可能让评审者以为「CI 会拦、所以本地跳过没关系」，而实际上 CI 会与本地钩子**同时**红，需要在合入/候选阶段前由 WP4 一并关闭，否则推送即失败。
- **最小修复**：把末句改为「该缺口只能由 WP4 关闭已登记的红窗口一并关闭（CI 的 `check:rust` 同样会红）；本提交仅跳过本地钩子第 4 阶段」。纯提交信息 amend，无需改代码。

### F2-R2-02 — 引入破坏的父提交 `3603612` 没有对应的 `--no-verify` 披露

- **严重度**：低（Low；流程可审计性）
- **位置**：提交 `3603612288d3c8e028219d73c77a8db28a4c2639e` 的提交信息（全文 7 行 + 一个 `Co-Authored-By` trailer，**无任何关于本地钩子未完整执行的说明**）
- **问题**：`3603612` 才是把 `SessionSummary::try_new` 改成 10 参数、并因此使 `cargo clippy --workspace` 结构性失败的那个提交（证据见 §2.6 第 3 点）。按同一逻辑，它当时也必然是以 `--no-verify` 提交的，但其提交信息对此**只字未提**。`347399f` 反过来把这个缺口写清楚了——披露方向是反的。
- **影响**：无功能影响；但若日后有人想复核「这个红窗口是谁、什么时候打开的」，从提交信息里只会看到修复方声明、看不到引入方的声明，审计链断在引入点。
- **最小修复**：不要求改代码（`3603612` 已被 `347399f` 覆盖，不宜 rebase 重写历史）。**在候选阶段 PV1 的记录里显式写一行**：「WP4 红窗口由 `3603612` 引入，其提交未披露 `--no-verify`；`347399f` 已补充披露」，把审计链补在流程记录而非历史里。

### F2-R2-03 — `ports.rs` 的 `workspace_alias` 字段注释未复述「同门」语义（遗漏，非冲突）

- **严重度**：低（Low；信息性）
- **位置**：`crates/core/src/ports.rs:189-195`
- **问题**：字段注释写「与 `workspace_cwd` 在**同一次提交**里写入，取值是本次创建解析使用的**别名原文**……`None` 语义为「未分组」，MUST NOT 由 `workspace_cwd` 反查别名补齐」，**没有**写出「与 `agent_session_id` 同门、未取得标识时三列都不写」。而 §5.2 的 `[决定]`（`docs/CORE_PORTS_AND_STORAGE.md:391`）与 spec delta 都写了。漂移门禁只比对 §5 的 **trait/方法签名**（`scripts/check-contract-drift.mjs` 的 `scanDeclarations`），不比对字段注释，所以这一遗漏不会被门禁发现。
- **影响**：无契约冲突——`ports.rs` 只是**没说**，没说错；权威表述在 spec 与 §5.2，三者不矛盾。但 `ports.rs` 是实现者最常读的一处，「同一次提交」很容易被读成「独立于 `agent_session_id` 的门」，正是 F1 曾经引发歧义的那半句。
- **最小修复**：在 `ports.rs:189` 的注释首句补「与 `agent_session_id` **同门**（`agent_session_id` 为 `None` 时三列都不写，§5.2 `[决定]`）」。一行注释，无行为影响。

## 4. 六条命令的真实输出

全部在 `CARGO_TARGET_DIR=D:\Project\acp-remote-wt\target-wp3-review2` 下、于被检视 worktree 内运行。

### 4.1 `cargo test --locked -p core` → **EXIT=0**

```
running 141 tests
test use_cases::tests::a_null_workspace_alias_is_never_backfilled_from_the_canonical_path ... ok
test use_cases::tests::create_session_persists_the_resolved_alias_next_to_the_canonical_path ... ok
test use_cases::tests::create_session_writes_the_recovery_columns_right_after_create ... ok
test use_cases::tests::resume_session_neither_reads_nor_rewrites_the_workspace_alias ... ok
test use_cases::tests::repointing_a_workspace_keeps_the_persisted_ownership ... ok
test model::tests::workspace_ref_bounds_the_display_name_and_keeps_both_fields ... ok
test result: ok. 141 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

（Round 1 为 140 passed；净 +1 = `WorkspaceRef` 新用例，同文件内删掉的那 4 行是同一用例内的断言，不增减用例数。）

### 4.2 `cargo test --locked -p server` → **EXIT=0**

```
running 296 tests
test result: ok. 296 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 15.03s
running 14 tests
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 4 tests
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 4 tests
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### 4.3 `cargo clippy --locked -p core -p server --all-targets --all-features -- -D warnings` → **EXIT=0**

```
    Checking identity-auth v0.0.0 (D:\Project\acp-remote-wt\wp3-core\crates\identity-auth)
    Checking server v0.0.0 (D:\Project\acp-remote-wt\wp3-core\crates\server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.18s
```

### 4.4 `node scripts/check-contract-drift.mjs` → **EXIT=0**

```
contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 96 个方法签名与 crates\core\src\ports.rs 一致
```

### 4.5 `cargo fmt -p core -p server -- --check` → **EXIT=0**（无输出）

### 4.6 `cargo test --locked -p storage-sqlite` → **EXIT=101**（登记在案的 WP4 红窗口）

```
error[E0061]: this function takes 10 arguments but 9 arguments were supplied
   --> crates\storage-sqlite\src\session_store.rs:454:19
    |
454 |       let summary = SessionSummary::try_new(
    |  ___________________^^^^^^^^^^^^^^^^^^^^^^^-
455 | |         decode(&text(row, "session_id")?, "owned_session.session_id")?,
456 | |         opt_text(row, "title")?,
457 | |         agent_from_row(row)?,
...   |
463 | |         decode(&text(row, "updated_at")?, "owned_session.updated_at")?,
464 | |     )
    | |_____- argument #10 of type `std::option::Option<WorkspaceRef>` is missing
    |
note: associated function defined here
   --> crates\core\src\model\session.rs:333:12
    |
333 |     pub fn try_new(
    |            ^^^^^^^
help: provide the argument
    |
454 |   let summary = SessionSummary::try_new(
...
463 |         decode(&text(row, "updated_at")?, "owned_session.updated_at")?,
464 ~         /* std::option::Option<WorkspaceRef> */,
465 ~     )
    |

For more information about this error, try `rustc --explain E0061`.
error: could not compile `storage-sqlite` (lib) due to 1 previous error
warning: build failed, waiting for other jobs to finish...
error: could not compile `storage-sqlite` (lib test) due to 1 previous error
```

**红灯范围的两项独立核实：**

- **错误集合仍是且仅是那一个 `E0061`。** `cargo check --locked -p storage-sqlite --lib` 的全部 `error` 行只有两条——`error[E0061] …` 与 `error: could not compile storage-sqlite (lib) due to 1 previous error`，即**同一个**错误在 lib 与 lib test 两个目标上各报一次。没有任何第二个错误码、没有任何 warning-as-error、没有任何测试断言失败。与 Round 1 §4 的逐字记录**完全一致**，无新增。
- **`tests/` 下 15 处 `SessionUpdate` 字面量仍只是潜伏。** `grep -rn "SessionUpdate {" crates/storage-sqlite/tests/ | wc -l` = `15`；`grep -rn "SessionUpdate {" -A 8 crates/storage-sqlite/tests/commit.rs | grep -c workspace_alias` = `0`（即这些字面量确实都缺 `workspace_alias` 字段）。它们**没有**出现在本轮编译输出中——因为 lib 先失败、`--all-targets` 的 integration test 目标尚未被编译到。仍是「已登记、等 WP4」的潜伏错误，WP3 未越界修补（`crates/storage-sqlite/**` 不在 `git diff --name-only` 中）。

### 4.7 为核实披露而额外跑的命令（非验收六条，一并留证）

```
$ cargo clippy --locked --workspace --all-targets --all-features -- -D warnings     # 钩子第 4 阶段
error[E0061]: this function takes 10 arguments but 9 arguments were supplied
error: could not compile `storage-sqlite` (lib test) due to 1 previous error
error: could not compile `storage-sqlite` (lib) due to 1 previous error
EXIT=101

$ cargo test --locked --workspace --all-features                                      # CI check:rust 第三条
error[E0061]: this function takes 10 arguments but 9 arguments were supplied
error: could not compile `storage-sqlite` (lib) due to 1 previous error
error: could not compile `storage-sqlite` (lib test) due to 1 previous error
EXIT=101

$ cargo fmt --all -- --check                     → EXIT=0（无输出）
$ npm run check                                  → EXIT=0（Totals: 19 passed, 0 failed (19 items)）
$ npx --no -- commitlint --edit <347399f 正文>   → EXIT=0（无输出）
$ gitleaks version                               → 8.30.1
$ gitleaks git --log-opts="3603612..347399f" --redact
11:34PM INF 1 commits scanned.
11:34PM INF scanned ~1684 bytes (1.68 KB) in 211ms
11:34PM INF no leaks found
EXIT=0
```

## 5. 本次检视的局限（如实声明）

1. **变异测试在 worktree 之外的副本中进行**（`D:\Project\acp-remote-wt\scratch-wp3-r2`，只复制了 `crates/ docs/ scripts/ schemas/ vendor/ Cargo.* rust-toolchain.toml`，未复制 `.git`/`node_modules`），每次变异后都以 `diff` 确认 `ids.rs` 与 worktree 逐字相同后才进行下一次。被检视 worktree 全程只读，`git status --porcelain --untracked-files=all` 为空。
2. **F2-R2-02 中「`3603612` 必然是 `--no-verify` 提交」是推断，不是直接证据。** git 对象不记录是否跳过钩子。可直接确证的是：按 `scripts/pre-commit.mjs` 的逻辑，`3603612` 若完整跑过钩子则必然在 clippy 阶段失败而无法提交；因此只能经 `--no-verify`（或等价的环境变量短路）落库。仓库内未发现任何配置层面的等价短路，故推断链完整，但结论本身仍属推断。
3. **本报告只能证明 core 侧契约与 fake 实现满足「不反查」。** 真实的反查风险实现于 `storage-sqlite` 的 `SessionStore::list`（WP4 范围）；本轮已确认 WP3 未触碰 storage，红灯集合未扩大，但 WP4 落地时仍须确认其 `list` 走 `workspace_alias` LEFT JOIN `owned_workspace` 并自带一条 `NULL` 不回填的存储层用例。
4. **`crates/app` 未被六条命令覆盖**（`cargo test -p core/server` 不含该包），且 `cargo test --workspace` 因 WP4 红灯未能编到 app。已核实 app 侧零构造点、零 `.summary()` 调用，WP3 未向它引入任何新的编译期依赖。
5. **Node Link / Sync 的最终 wire 形态不由本报告担保**（取决于 WP2）。本轮只核实了「WP3 未扩展 Node Link 投影面」：`crates/server/src` 中 `workspace()` 零命中。

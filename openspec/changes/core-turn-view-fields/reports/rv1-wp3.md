All work is complete and the workspace is clean. Here is my report.

---

RV1-WP3/WP4：无未解决阻断项

## 核实结论

**审阅版本**：base `1cd0416`（refs/heads/main）、target `f582376`、记录提交 `5ff2d0c`。已核实 `git status --porcelain` 为空、`git diff --stat f582376..HEAD -- crates/` 为空（即被检视版本 == 当前工作区 `crates/` 内容）、`git diff --stat HEAD` 为空。被检视 diff（`git --no-pager diff 1cd0416..f582376 -- crates/storage-sqlite crates/agent-host`）为 4 文件 / +477 −2：`agent-host/src/lib.rs`(6)、`agent-host/tests/view_contract.rs`(+198)、`storage-sqlite/src/session_store.rs`(+3)、`storage-sqlite/tests/session_version_rule.rs`(+272)。整变更 17 文件，**无 `Cargo.toml`/`Cargo.lock`/`schemas`/`fixtures`/`compatibility` 改动**。

---

### 1. `origin_epoch` 修复的正确性

**结论：正确、必要、且没有削弱「调用方给出 epoch 且不一致 → 拒绝」的判定。三个状态分支之外不存在同类漏填。**

- 修复点：`crates/storage-sqlite/src/session_store.rs:1014` `origin_epoch.get_or_insert(stored_epoch);`，位于 `Update` 分支内。
- **校验在前、补齐在后（所以不削弱判定）**：`session_store.rs:1004-1011` 先做 `if commit.origin_epoch.as_ref().is_some_and(|epoch| epoch != &stored_epoch) { return Err(PortError::Conflict(ConflictKind::VersionMismatch)) }`；`get_or_insert` 只在 `None` 时用**库内已存在的** epoch 填充（`stored_epoch` 来自 `session_version_and_epoch(&mut tx, &session)`，即同一会话行），既不改写调用方给出的值，也不新造 epoch。给出不匹配值仍被拒（该分支未改一行）。
- **与无状态分支逐字一致**：`session_store.rs:1069-1077` 早已是「先校验 mismatch、后 `origin_epoch.get_or_insert(stored_epoch)`」，修复使 `Update` 分支与其对齐。
- **与 §5.2 文档一致**：`docs/CORE_PORTS_AND_STORAGE.md:332`「`origin_epoch` 由 core 在创建会话时用 `IdGenerator` 生成并传入 …存储层只校验"该会话已有 epoch 时必须一致"」；`:264` 的字段注释「新建会话时由 core 生成并传入」。修复填的是「已有 epoch」，不违反该约束。同时它也是 DDL 的硬要求：`crates/storage-sqlite/src/migrate.rs:83`（`owned_event.origin_epoch TEXT`）+ `:103`（`CHECK ((session_id IS NULL) = (origin_epoch IS NULL))`），即会话级事件必须有 epoch，否则成对 CHECK/显式 `InvalidRequest` 会拒绝——这正是 F1 的报错来源（`session_store.rs:1141-1143`）。实现与 §9 判据 17 一致。
- **其它分支无同类漏填**（`rg -n "origin_epoch" crates/storage-sqlite/src/` 逐点核对）：
  - `Create`：`session_store.rs:962-968` 要求调用方必须给（`ok_or(InvalidRequest("session creation requires an origin epoch"))`），`:988` 回写 `origin_epoch = Some(epoch)` —— 无缺口。
  - 无状态分支（`state: None` + `session: Some`）：`:1069-1077` 已补齐 —— 无缺口。
  - `compacted` 路径：只做 `UPDATE owned_event SET compacted_into = ?1`（`session_store.rs:1235-1236`），不写 origin 列 —— 不适用。
  - imported 路径（`commit_receipt`）：epoch 来自 `receipt.origin.origin_epoch`（`session_store.rs:2529/2539`，`OriginEventRef` 必填），与「Owner 注入」合同一致 —— 无缺口。
  - 其余 `owned_*` 表（`owned_interaction`/`owned_command`/`owned_turn`）**没有** `origin_epoch` 列（migrate.rs 中含该列的只有 `owned_session`(59)、`owned_event`(83)、`imported_session`(359/361)、`imported_delivery_index`(375/383)），故不存在第 4 个需要补齐的写入分支。
- 附带行为变化（非缺陷）：`Update` 且调用方未给 epoch、且该批无会话级事件时，`CommitOutcome.origin_epoch` 由原先的 `None` 变为 `Some(存储 epoch)`——与无状态分支既有返回形状一致，是收敛而非分叉。

### 2. 该缺陷是既有缺陷，不是本次引入

**结论：确认为既有缺陷，且是**真实生产路径**的缺陷；此前未被发现的原因是「core 侧用 fake store、storage 侧总是显式传 epoch」，仓库内**没有任何**测试把 Broker 与 SqliteStore 组合起来。**

- 归属：`GIT_PAGER=cat git --no-pager log -1 --format='%h %s' 1cd0416 -- crates/storage-sqlite/src/session_store.rs` → `e732ff4 fix(storage): 收口管理存储的四条状态完整性守卫 (#12)`（即基线之前的最后一次改动与此无关）。本次改动对该文件只有 3 行新增（1 行代码 + 2 行注释），`Update` 分支的 SQL、DDL、`migrate.rs` 均未动。
- 生产可达性（同一类 shape 确实由 broker 组装）：
  - `crates/core/src/broker.rs:1234` 是**唯一**传 `origin_epoch: Some(...)` 的位置（`create_session`）。
  - `crates/core/src/broker.rs:1545-1558`（`commit_locked`：`state = Some(StateChange::Update(...))` + 终态事件 + `origin_epoch: None`）与 `:2011-2030`（`RecoverUnsettled`：`turn.failed`/`command.uncertain`/补写 `agent.message.completed` + `StateChange::Update` + `origin_epoch: None`）都是「状态变更 + 会话级事件同批」，正是 F1 的触发形状。故修复前**turn 终态提交与启动恢复写路径在真实存储上会整体失败**。
- 为什么旧测试没暴露：
  - 所有含 session 级事件的既有提交都**显式**给了 epoch：`crates/storage-sqlite/tests/commit.rs:132`（`StateChange::Update`）与 `:154`（`origin_epoch: Some(origin_epoch())`）同属 `commit_writes_state_turns_events_and_terminal_in_one_transaction`；同类还有 `retention.rs:627`、`enum_coverage.rs:414`。既有 `origin_epoch: None` 的三处都是 node 作用域事件（`commit.rs:608`、`tests/contract_v03.rs:453`、`tests/migration.rs:695`）或空事件 `NotFound`（`commit.rs:1237`）。
  - core 侧测试用内存 fake store（不经过 `session_store.rs`）；`rg -l SqliteStore crates/` 只命中 `crates/storage-sqlite/**`，`rg -l "Broker::new|acp_core::Broker|broker::" crates/ --glob '!crates/core/**'` 无命中 ⇒ 该组合此前从未被任何测试驱动。

### 3. WP3 用例可证伪、且真的覆盖规则

**结论：用真实 `SqliteStore`（非替身），三个方向各在独立 `#[tokio::test]` 里断言；回退修复后确实变红（见变异自检 a），改错期望值也变红（变异自检 c）。**

- 真实存储：`crates/storage-sqlite/tests/session_version_rule.rs:102-107` `SqliteStore::open(StorageConfig::new(dir), &at(0))`，落库为系统临时目录 `acpr-storage-*`（`tests/support/mod.rs:44-56`，用例自建自清）。
- 三个独立方向：
  1. 纯事件提交不递增：`:110` `event_only_commits_keep_the_session_version` —— 断言 `outcome.version == 1`（`:120`）、`load().session.version() == 1`（`:141-150`），并用 `read_view().event_payload()` 做**逐字节** view 断言（`:131-139`）。
  2. 状态变更 +1：`:155` `state_change_commits_bump_the_session_version_by_one` —— 首次 `StateChange::Update` 断言 `+1`（`:182`），中间夹一次纯事件提交断言版本停住（`:189`），再一次状态变更断言 `+1`（`:201`）。注意它用的是 `mode: ModeChange::Set` + `state: None`，恰好覆盖「只要含 `StateChange` 就 +1」这一与 §6 第 19 条推导口径一致的边界。
  3. 过期 `expected_version` 被拒：`:206` `a_state_change_with_a_stale_expected_version_is_rejected` —— 断言恰好 `Err(Conflict(VersionMismatch))`（`:224-232`）、被拒后版本仍为 1（`:233-243`），并有一条正向对照（`:245-271`，`expected_version = Some(1)` → `version == 2`）。
- 回退修复即变红：见自检 (a)（`state_change_commits_bump_the_session_version_by_one` FAIL，报错与 F1 完全一致）。
- 一处力度提示（不阻断）：第 3 个用例的正向对照 `events: Vec::new()`（`:252-265`），所以它**不**守卫该修复；修复目前只被用例 2 单独守住。另 `:270` 有 `let _ = Sequence::new(1).expect("sequence");` 一行死代码（为消化 `Sequence` 导入）。

### 4. WP4 用例真实（非空转），但守卫清单有一个可证伪性缺口

**结论：它是真实驱动 `AgentHost` + fake ACP 子进程的产物断言，不是重新声明映射；「适配器加 `turnId`」与「§10.3 最低字段缺失」都会让它变红（自检 b / d）；字段表与 `docs/SYNC_PROTOCOL.md` §10.3 逐项一致。缺的是「守卫清单不完整」——3 个被声明覆盖的类型断言永不执行，且产物缺失可静默通过（自检 e）。**

- 真实链路：`crates/agent-host/tests/view_contract.rs:63-77`（`AgentHost::new` + `profile_with("agent-1", FAKE_AGENT, ["--scenario", …])`，真子进程）、`:88-114`（`host.create(...)` → `endpoint.prompt(...)` → `collector.wait_for_type(...)`）。`Collector` 是真实 `EventSink`（`tests/support/mod.rs:60-77`），收的就是适配器发出来的 `EndpointEvent`。
- 断言有效：`:152-163` 断言 `event.turn.is_none()`、`view.get("turnId").is_none()`、`view.get("version").is_none()`；`:164-170` 断言每个最低字段存在。自检 (b)/(d) 已证明两处都会真的变红，且自检 (f) 打印出的真实产出类型清单（`agent.message.delta`×2、`agent.thought.delta`、`tool.call.started`、`turn.completed`、`tool.call.updated`、`tool.call.completed`、`session.plan.changed`、`session.commands.changed`、`session.mode.changed`、`session.config.changed`、`session.info.changed`、`session.usage.changed`、`session.update.unknown`、`permission.requested`、`elicitation.requested`）说明用例确实在真实产物上跑。
- 与 §10.3 表格一致性（`docs/SYNC_PROTOCOL.md:1000/1013/1015` 等）：要求 `turnId` 的 15 个类型被 `TURN_SCOPED`（`:26-45`，11 项）+ `CORE_ONLY`（`:53-58`，4 项）**完整**覆盖；要求 `version` 的 2 个类型在 `VERSION_SCOPED`（`:47-51`）。含 `elicitation.requested` 的 `schema`/`initialValues`（`:44`）与 `permission.requested` 的 `description`（`:40`）——两者都在必查清单里，且实现在 `src/mapper.rs:361`、`:396-399` 真的产出这两个字段。**没有**遗漏。
- **缺口（MINOR）**：`user.message.delta`(`:30`)、`turn.cancelled`(`:28`)、`turn.failed`(`:29`) 这三个条目在这三个场景里从不产出（fake 只发 `agent_message_chunk`，见 `src/bin/acpr-fake-acp-agent.rs:689-701`；场景不 cancel/fail turn），而 `:143-146` 对「该类型事件为空」是 `continue` 静默跳过；守卫清单 `:176-185` 又只列了 8 项，**不含**这三项，也不含已经产出并被检查的 `tool.call.updated`(`:34`)/`tool.call.completed`(`:35`)。自检 (e) 证明：把 `tool_call_update` 改成未登记判别子（这两个类型随之消失）后**用例仍然全绿**——即「某个 §10.3 类型在产物里消失」不会失败（该类型确实被产出的独立证据：`src/mapper.rs:132/141` 与 `tests/session.rs:144-145` 都把 `tool.call.updated`/`tool.call.completed` 列为同场景的预期事件）。

### 5. 边界与范围

**结论：agent-host 只改了测试与 `src/lib.rs` 注释（零行为改动）；storage-sqlite 没有改 DDL/`migrate.rs`/端口签名；WP3/WP4 只引用公开 API，无跨 crate 私有实现依赖。**

- `agent-host/src/lib.rs` 的 6 行差异全在文件头 `//!` 文档块内（`pub mod` 声明以上），把「已知缺口」改写为「与 core 的分工」；`git diff --stat` 显示该 crate 仅 `src/lib.rs`(6) + `tests/view_contract.rs`(+198)，`tests/support/mod.rs` **未被本次改动触碰**（用例复用了既有的 `Collector`/`FakeConfig`/`TestIds`/`profile_with`）。
- `storage-sqlite` 仅 `src/session_store.rs`(+3) + `tests/session_version_rule.rs`(+272)；`src/migrate.rs` 无差异，`Update` 分支 SQL 无差异，无端口签名变化 ⇒ `check:drift`/`check:boundaries` 不受影响。
- 公开面引用：WP3 用 `storage_sqlite::migrate::StorageConfig`、`storage_sqlite::session_store::SqliteStore`（`src/lib.rs:14-15` 均为 `pub mod`，`SqliteStore::open` 为 `pub`，`session_store.rs:100`）与 `acp_core::ports::{OwnedCommit, SessionStore, …}`；WP4 用 `agent_host::{AgentHost, HostConfig}`（`src/lib.rs:38/40` 的 `pub use`）与 `acp_core::ports::SessionBackendFactory`。没有 `pub(crate)` 越界、没有复制 broker 内部逻辑。
- 依赖方向未被破坏：新测试不新增任何依赖（无 `Cargo.toml`/`Cargo.lock` 改动），`serde_json` 本就是 `agent-host` 的既有依赖。

### 6. 文档一致性

**结论：F1/F3/F4 与 Check Plan Change 4 的描述与代码/日志事实**逐条吻合**；但 `plan.md` 的 PV3 判据比实际做的强，且这次「判据弱化」没有登记为 Check Plan Change。**

- F1（`verification.md:85`）：报错文本、触发用例名、修复位置（`Update` 分支校验后 `get_or_insert`）、「与无状态分支一致，符合 §5.2」全部与 `session_store.rs:1004-1014` 一致；「回退修复即变红」由自检 (a) 复现。
- Check Plan Change 4（`verification.md:38`）：「1 行修复」「未改 DDL/`migrate.rs`、未改端口签名」属实（diff 3 行 = 1 代码 + 2 注释）；「broker 只在建会话时传 epoch」由 `broker.rs:1234`（唯一 `Some`）核实；「终态事件与状态变更同批」由 `broker.rs:1545-1558` 核实。`plan.md:197` 的 WP3 写范围已同步扩为「+ `src/session_store.rs` 一处修复」⇒ 计划与记录一致。
- F3（`verification.md:87` / core `broker.rs:6693-6704`）：用例确实改成与「当时会话版本」比较，且脚本明确不带终态事件（代码注释 `:6704` 与描述一致）。属 WP2 范围，仅作旁证核对。
- F4（`verification.md:88`）：`view_contract.rs:24` 已 `use acp_core::ports::SessionBackendFactory;`，`:193` 用 `types.iter().any(|kind| kind == core_only)` 而非 `contains(&str)` —— 与「导入 trait、改用 `iter().any(...)`」一致，属编译期修正。
- 日志事实核对：`reports/wp3-storage-version.log` 与 `reports/wp4-agent-host-contract.log` 均以 `# exit=0` 结尾，与 `verification.md` 的 Checks 表 PASS 一致；实测计数亦对得上：`cargo test -p agent-host --all-features -- --list` = **53**（与 `verification.md:22` 一致），`-p storage-sqlite` = **99**（其中 `--ignored` 2 条），即 lib 单测 3 + 既有集成 91 + 新增 3 + ignored 2（`verification.md:21` 的「既有套件 91 passed / 2 ignored」在该口径下成立，措辞略歧义）。
- **不一致点**：`plan.md:251` 的 PV3 判据写「必须覆盖：适配器产出的 view **经 core 提交后**满足 §10.3 的 `turnId`/`version`，且 ACP 三要素不变（R16）」，而实际 `view_contract.rs` 明确**不**跑 core 提交（文件头 `:11-14` 自述「不复制 core 的注入逻辑」，core 注入由 WP2 的 fake store 用例覆盖）；`verification.md:22` 把 PV3 的 scope 改写成实现的事实（诚实），但这条判据调整**未**登记进 `verification.md:31-38` 的 4 条 Check Plan Change。

---

## 发现

**[MINOR] `openspec/changes/core-turn-view-fields/plan.md:251` — PV3 判据「适配器产出的 view 经 core 提交后满足 §10.3」没有任何测试执行；该判据弱化未登记为 Check Plan Change。** 证据：`crates/agent-host/tests/view_contract.rs:11-14` 自述不做 core 注入、`rg -l SqliteStore crates/` 仅命中 `crates/storage-sqlite/**`、`rg -l "acp_core::Broker|Broker::new" crates/ --glob '!crates/core/**'` 无命中（即不存在「适配器 view → core 提交」的组合用例）；`verification.md:22` 与 `:31-38` 只记了 4 条变更。影响：PV3 的「必须覆盖」条件按字面未满足，风险由 WP2（fake store 下注入 + 保真）与 WP4（适配器字段）两半分别覆盖，无产品缺口，但检查计划与实现不一致。建议：在 `verification.md` 登记第 5 条 Check Plan Change（把 PV3 判据改为实测口径），或在 `crates/agent-host/tests/` 增补一条「adapter view 经 Broker + SqliteStore 提交后满足 §10.3」的组合用例后恢复原判据。

**[MINOR] `crates/agent-host/tests/view_contract.rs:28-30` vs `:143-146`/`:176-185` — 守卫清单不完整，3 个类型的断言永不执行，产物消失可静默通过。** 证据：`user.message.delta`(`:30`)、`turn.cancelled`(`:28`)、`turn.failed`(`:29`) 在三个场景中从不产出（fake 只发 `agent_message_chunk`，`src/bin/acpr-fake-acp-agent.rs:689-701`），`:143-146` 对空集合 `continue`，而守卫清单只含 8 项不含它们；已产出并被检查的 `tool.call.updated`(`:34`)/`tool.call.completed`(`:35`) 也不在守卫清单。实测：把 `acpr-fake-acp-agent.rs:504` 的 `sessionUpdate` 改成 `tool_call_update_UNREGISTERED`（这两个类型随之消失）→ `cargo test -p agent-host --test view_contract` 仍 `1 passed`（见自检 e）。影响：`TURN_SCOPED` 声明覆盖 11 个类型，实际只验证 8 个；`turn.cancelled`/`turn.failed` 的 `state`/`error`、`user.message.delta` 的 `messageId/deltaIndex/text` 及其「不产 `turnId`」全部未验证（`src/mapper.rs:250-288` 是同一条 `delta_event` 路径，风险低）。建议：把守卫清单改为遍历 `TURN_SCOPED ∪ VERSION_SCOPED`（缺任一即失败），并给 cancel/failed/user-chunk 补场景或显式登记为未覆盖。

**[SUGGESTION] `crates/storage-sqlite/tests/session_version_rule.rs:88-99` vs `crates/core/src/broker.rs:1545-1558` — 缺陷复现是手写的 `OwnedCommit`（`origin_epoch: None`），没有覆盖「broker 组装」这一真实触达面。** 证据：该 helper 手工构造 `session: Some`, `state: Some(Update)`, `events`, `origin_epoch: None`；仓库内不存在 Broker + SqliteStore 的组合测试。影响：能证伪本次修复（自检 a 已证），但若将来 broker 改传 epoch（或改批次形状），这类缺陷仍不会被任何测试捕获。建议：在后续切片补一条用真实 `Broker` + 真实 `SqliteStore` 的提交用例（本次可不做）。

**[SUGGESTION] `crates/storage-sqlite/tests/session_version_rule.rs:270` — `let _ = Sequence::new(1).expect("sequence");` 是为消化 `Sequence` 导入写的死代码。** 证据：该行右侧无任何断言作用，`Sequence` 在文件中仅此一处使用。影响：无功能影响，但读起来像缺失断言。建议：删掉该行与 `Sequence` 导入，或把它换成对 `appended[].session_sequence`/`global_sequence` 的真实断言。

**[SUGGESTION] `openspec/changes/core-turn-view-fields/verification.md:21` — 「既有套件 91 passed / 2 ignored」口径歧义。** 证据：`reports/wp3-storage-version.log` 各行求和为 **97 passed / 2 ignored**（lib 单测 3 + 既有集成 91 + 新增 3），本地 `--list` 亦为 99 项（2 条 `#[ignore]`）。影响：不是虚假声明，但读者按「97」核对会对不上。建议：改写为「既有集成套件 91 + lib 单测 3 + 新增 3 passed / 2 ignored」。

**[SUGGESTION] `crates/agent-host/tests/view_contract.rs:164-170` + `src/mapper.rs:361` — 最低字段只断言「存在」，无法区分真实投影与恒空默认值。** 证据：`description` 来自 `tool_call.name.unwrap_or_default()`，而 fake 的 `session/request_permission` 不带 `name`（`src/bin/acpr-fake-acp-agent.rs:413-437`），实测产物里 `description` 为 `""`（自检 d 打印的 view）。上游 schema 确有 `ToolCallUpdate.name`（`schemas/acp/v1/upstream.json:380+` 的 `"name"`），故实现本身合规、属既有行为。影响：§10.3 只要求 string，`""` 合规，但 UI 会显示空描述。建议（可选，不建议在本变更动）：给 fake 的权限请求补 `name`，让用例同时锁住「非空投影」，避免契约用例退化为「字段存在即可」。

---

## 变异自检记录

基线（改动前）：`cargo test --locked -p storage-sqlite --all-features --test session_version_rule` → `3 passed; 0 failed`；`cargo test --locked -p agent-host --all-features --test view_contract` → `1 passed; 0 failed`（均已实测）。共 6 条变异，全部执行后已还原。

**(a) 回退 `origin_epoch` 修复** — `sed -i '1014s|.*|// MUTATION-A…|' crates/storage-sqlite/src/session_store.rs`
`cargo test --locked -p storage-sqlite --all-features --test session_version_rule` → **红**：
```
test state_change_commits_bump_the_session_version_by_one ... FAILED
panicked at crates\storage-sqlite\tests\session_version_rule.rs:181:10:
first state change: InvalidRequest("a session-scoped event requires an origin epoch")
test result: FAILED. 2 passed; 1 failed
```
（注意：`a_state_change_with_a_stale_expected_version_is_rejected` 未变红，因其正向对照 `events` 为空——见问题 3。）`git checkout --` 还原 → 复跑 **绿** `3 passed`。

**(b) 让 delta view 多写 `turnId`** — `src/mapper.rs` 的 `delta_event` 的 `json!` 里插入 `"turnId":"00000000-0000-4000-8000-000000000000"`（第 273 行后）
`cargo test --locked -p agent-host --all-features --test view_contract` → **红**：
```
panicked at crates\agent-host\tests\view_contract.rs:156:13:
agent.message.delta: 适配器不得自带 turnId（core 注入）
test result: FAILED. 0 passed; 1 failed
```
`git checkout -- crates/agent-host/src/mapper.rs` → 复跑 **绿** `1 passed`。

**(c) 把 WP3 期望版本改成错值** — `session_version_rule.rs:182` `Version::new(2)` → `Version::new(9)`
`cargo test … --test session_version_rule` → **红**：
```
panicked at crates\storage-sqlite\tests\session_version_rule.rs:182:5:
assertion `left == right` failed: 状态变更提交 +1
  left: Version(2)   right: Version(9)
```
（证明该断言不是恒真）`git checkout --` → 复跑 **绿**。

**(d) 删掉 `permission.requested` 的 `description`** — `src/mapper.rs:361` 删除 `"description": tool_call.name.clone().unwrap_or_default(),`
`cargo test … --test view_contract` → **红**：
```
panicked at crates\agent-host\tests\view_contract.rs:165:17:
permission.requested: 缺少 §10.3 最低字段 description：{"interactionId":"00000000-0000-4000-8000-000000000002","options":[…],"title":"写入文件"}
```
`git checkout --` → 复跑 **绿**。

**(e) 让产物里消失一个「已产出但未被守卫」的类型** — `src/bin/acpr-fake-acp-agent.rs:504` 的 `"sessionUpdate": "tool_call_update"` → `"tool_call_update_UNREGISTERED"`
`cargo test … --test view_contract` → **仍然全绿**：
```
test adapter_views_leave_turn_and_version_identity_to_core ... ok
test result: ok. 1 passed; 0 failed
```
（`tool.call.updated` / `tool.call.completed` 双双消失而用例不失败 ⇒ 守卫清单缺口，见发现 2。）`git checkout --` → 复跑 **绿**。

**(f) 从 `TURN_SCOPED` 移除一个条目** — `tests/view_contract.rs:32` 删除 `("agent.thought.delta", …)`
`cargo test … --test view_contract` → **红**（同时打印了真实产出类型清单）：
```
panicked at crates\agent-host\tests\view_contract.rs:185:9:
本用例必须真实检查到 agent.thought.delta（否则断言等于没测）：["agent.message.delta", … "elicitation.requested"]
```
`git checkout --` → 复跑 **绿**。

**还原确认**：`git status --porcelain` 无输出、`git diff --stat HEAD` 无输出；`git diff --stat f582376..HEAD -- crates/` 无输出（工作区 == 被检视版本）。全部命令均在 300s 内返回（实测单条 ≤ 3s，`--list` 两次 ≤ 5s），未触发超时放弃。

---

## 同类实例扫描

1. **`origin_epoch` 在 `crates/storage-sqlite/src/**` 的全部使用点**（`rg -n "origin_epoch" crates/storage-sqlite/src/`，30+ 命中，逐点分类）：`commit` 内的写侧只有 `Create`(`:964/988`)、`Update`(`:1006/1014`)、无状态(`:1071/1077`) 三个分支 + 事件循环(`:1141`)，**无第 4 处**；其余为读侧（`load`/replay/imported/管理表）。DDL 侧只有 `owned_session`/`owned_event` 与 `imported_*` 含该列，`owned_interaction`/`owned_command`/`owned_turn` 没有 ⇒ 不存在「同类的另一个漏填分支」。
2. **其它 crate 是否「假设 view 自带 turnId/version」或自行补这两个字段**：`rg -n '"turnId"' crates/*/src` 命中仅 `sync-protocol`（wire DTO 字段名，`src/views.rs` 等）与 `core`（注入/读取，`broker.rs:2064/2532/2865-2967`）；`rg -n 'insert\("version"|"version":' crates/*/src` 命中核心注入点与 fake agent 的 `agentInfo.version`。**没有**适配器或存储侧自行生成 view 的 `turnId`/`version`（agent-host `mapper.rs` 的 view 构造里零命中），分工未被第二生产者破坏。
3. **「用固定占位 id 断言真实 id」的脆弱写法**：WP3/WP4 两个新文件**没有**该模式——`session_version_rule.rs:29-33` 的 `ORIGIN_EPOCH`/`TURN` 是输入常量（断言的是版本与字节，不是 id 保真），`view_contract.rs:96-99` 的字面 sessionId 是 `host.create` 的输入，断言只针对 `view.get("turnId").is_none()` 这类**不存在性**，不与任何真实 id 比较（自检 b 也证明该断言靠得住）。同仓内的相邻写法：`crates/core/src/broker.rs:6195` 在**负向**夹具（非法 `compacted` 引用）里用了全零 UUID 字面量，`broker.rs:6788` 有意枚举 `[uuid_text(0), uuid_text(999)]` 作为 R7 的两种占位值——两者都不是「拿占位值当真值断言」，且属 WP2 范围。

---

## 未解决阻断项

无。

（附带说明，供主 Agent 决策，不构成本轮阻断项：`verification.md:22` 的 PV3 scope 与实现一致、诚实；需要动作的是 `plan.md:251` 的 PV3 判据与第 5 条 Check Plan Change 的登记，以及 `view_contract.rs` 的守卫清单补全。两者均为测试/文档类修正，不影响 `f582376` 的代码正确性判断。）

```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "按 6 个问题逐条给出结论 + file:line 证据（session_store.rs:1004-1014/1069-1077/1141、migrate.rs:83/103、docs/CORE_PORTS_AND_STORAGE.md:264/332/1268、broker.rs:1234/1545-1558/2011-2030/6693、SYNC_PROTOCOL.md:1000/1013/1015、session_version_rule.rs:102-271、view_contract.rs:26-58/88-197、verification.md:20-23/31-38/85-88、plan.md:197-198/251/253），并附 6 条已执行并还原的变异、基线绿与全仓同类扫描结论；共 6 条非阻断发现（2 MINOR + 4 SUGGESTION），0 条 BLOCKER/MAJOR。"
    }
  ],
  "changedFiles": [],
  "testsAddedOrUpdated": [],
  "commandsRun": [
    {
      "command": "git status --porcelain / git log -3 / git diff --stat 1cd0416..f582376",
      "result": "passed",
      "summary": "工作区干净；base=1cd0416、target=f582376、记录提交=5ff2d0c；被检视范围 4 文件 +477 −2，无 Cargo/schemas/fixtures 改动"
    },
    {
      "command": "cargo test --locked -p storage-sqlite --all-features --test session_version_rule",
      "result": "passed",
      "summary": "基线 3 passed / 0 failed；变异 (a)(c) 后分别 2 passed/1 failed，还原后再次 3 passed"
    },
    {
      "command": "cargo test --locked -p agent-host --all-features --test view_contract",
      "result": "passed",
      "summary": "基线 1 passed；变异 (b)(d)(f) 后 FAILED 且报错指向预期断言；变异 (e) 仍 passed（暴露守卫清单缺口）；全部还原后 1 passed"
    },
    {
      "command": "cargo test --locked -p agent-host --all-features -- --list / -p storage-sqlite --all-features -- --list [--ignored]",
      "result": "passed",
      "summary": "agent-host=53（与 verification.md:22 一致）；storage-sqlite=99、ignored=2（=lib 3 + 既有集成 91 + 新增 3 + 2 ignored）"
    },
    {
      "command": "rg -n \"origin_epoch\" crates/storage-sqlite/src/ ; rg -n '\"turnId\"' crates/*/src ; rg -l SqliteStore crates/",
      "result": "passed",
      "summary": "无第 4 个需补齐 epoch 的写分支；无适配器自行产 turnId/version；仓库内不存在 Broker + SqliteStore 组合测试"
    }
  ],
  "validationOutput": [
    "变异 (a): panicked at tests/session_version_rule.rs:181:10: first state change: InvalidRequest(\"a session-scoped event requires an origin epoch\") → test result: FAILED. 2 passed; 1 failed",
    "变异 (b): panicked at tests/view_contract.rs:156:13: agent.message.delta: 适配器不得自带 turnId（core 注入）",
    "变异 (c): assertion `left == right` failed: 状态变更提交 +1 / left: Version(2) right: Version(9)",
    "变异 (d): panicked at tests/view_contract.rs:165:17: permission.requested: 缺少 §10.3 最低字段 description：{...}",
    "变异 (e): test adapter_views_leave_turn_and_version_identity_to_core ... ok（tool.call.updated/completed 消失仍绿 ⇒ 守卫清单缺口）",
    "变异 (f): panicked at tests/view_contract.rs:185:9: 本用例必须真实检查到 agent.thought.delta（否则断言等于没测）：[\"agent.message.delta\", …, \"elicitation.requested\"]",
    "还原后: git status --porcelain 无输出、git diff --stat HEAD 无输出、git diff --stat f582376..HEAD -- crates/ 无输出"
  ],
  "residualRisks": [
    "PV3 计划判据（plan.md:251）与实际检查不一致，且第 5 条 Check Plan Change 未登记：若主 Agent/最终验收按 plan.md 逐条核对 PV3，会判为「判据未满足」；建议先登记变更再进入 DU1/最终验收。",
    "view_contract 的守卫清单不完备：user.message.delta/turn.cancelled/turn.failed 的 §10.3 断言永不执行，已产出的 tool.call.updated/tool.call.completed 消失也不会失败（已用变异 e 实证）。",
    "缺陷触达面仍无 Broker + SqliteStore 组合用例：本次回归是手写 OwnedCommit 复现（能证伪修复，但无法保护 broker 未来的批次形状变化）。",
    "本轮未执行 npm run check / npm run verify / 全工作区测试（按检视纪律只跑单测二进制），合同门禁与 workspace 级 clippy/fmt 属 PV1/PV4，需在 DU1 阶段另行核对；本轮结论不覆盖它们。"
  ],
  "noStagedFiles": true,
  "diffSummary": "本轮为只读检视并执行了 6 条已还原的变异，未产生任何文件改动（changedFiles 为空）；被检视对象为 1cd0416..f582376 的 crates/storage-sqlite（+3 修复行、+272 新用例）与 crates/agent-host（+198 新契约用例、lib.rs 注释 6 行）。",
  "reviewFindings": [
    "minor: openspec/changes/core-turn-view-fields/plan.md:251 - PV3 判据要求「适配器 view 经 core 提交后满足 §10.3」，无任何测试执行该组合，且该判据弱化未登记为 Check Plan Change（verification.md:31-38 仅 4 条）",
    "minor: crates/agent-host/tests/view_contract.rs:28-30 与 :176-185 - 守卫清单不完整，3 个 TURN_SCOPED 条目断言永不执行，产物缺失可静默通过（变异 e 实证）",
    "suggestion: crates/storage-sqlite/tests/session_version_rule.rs:88-99 - 缺陷复现为手写 OwnedCommit，缺 Broker + SqliteStore 组合覆盖",
    "suggestion: crates/storage-sqlite/tests/session_version_rule.rs:270 - 死代码 `let _ = Sequence::new(1)…`",
    "suggestion: openspec/changes/core-turn-view-fields/verification.md:21 - 「既有套件 91 passed」口径歧义（日志合计 97 passed/2 ignored）",
    "suggestion: crates/agent-host/tests/view_contract.rs:164-170 - 最低字段仅断言存在，无法区分真实投影与恒空默认值（description 实测为 \"\"）",
    "no blockers: origin_epoch 修复正确且未削弱跨调用方校验，Create/无状态/compacted/imported 分支无同类漏填；缺陷确为既有（e732ff4 之前遗留），非本次引入"
  ],
  "manualNotes": "判定：f582376 的 WP3/WP4 代码与用例经独立复核为 PASS（无 CRITICAL/MAJOR）。① `origin_epoch` 修复正确：mismatch 校验（session_store.rs:1004-1011）仍在 get_or_insert(1014) 之前，只填 None 且只填库内已有值，与无状态分支(1069-1077)逐字一致、与 §5.2(文档:332) 及 DDL 成对 CHECK(migrate.rs:83/103) 一致；其它分支无同类漏填。② 缺陷确为既有（该文件在 base 的最后改动是 e732ff4，与本次无关），且是真实生产路径（broker.rs:1234 是唯一传 epoch 处，:1545-1558 与 :2011-2030 都是「状态变更 + 会话级事件同批 + epoch None」）；此前未被发现是因为既有 storage 用例全部显式传 epoch（commit.rs:132/154、retention.rs:627、enum_coverage.rs:414），且仓库内没有任何 Broker + SqliteStore 组合测试。③ WP3 走真实 SqliteStore，三个方向各有独立断言，回退修复即红（自检 a）、改错期望值即红（自检 c）。④ WP4 走真实 AgentHost + fake ACP 子进程，字段表与 §10.3 完整一致（含 elicitation 的 schema/initialValues、permission 的 description），加 turnId 或缺字段都会红（自检 b/d），但守卫清单漏掉 3 个未产出类型与 2 个已产出类型（自检 e 证明可静默通过）。⑤ 范围干净：agent-host 仅测试 + lib.rs 注释；storage-sqlite 未动 DDL/migrate.rs/端口；无新依赖；跨 crate 只用公开 API。⑥ F1/F3/F4 与 Check Plan Change 4 的描述与代码/日志事实吻合；唯一文档漂移是 plan.md:251 的 PV3 判据强于实现（verification.md 已按实改写但未登记为第 5 条 Check Plan Change）。建议主 Agent 把上述 2 条 MINOR 交测试/文档责任人后即可进入 DU1 阶段；本轮未跑合同门禁与 workspace 级检查，不覆盖 PV1/PV4。"
}
```

# RV1-WP1/WP2：无未解决阻断项

（非阻断发现 5 条 MINOR + 4 条 SUGGESTION，详见 `## 发现`；无 BLOCKER/MAJOR。）

## 检视输入与限制

| 项 | 值 |
| --- | --- |
| Review ID / Type / Stage | RV1 / branch / WP1+WP2 交付前 |
| Repository | `D:/Project/acp-remote`（主工作区，固定版本 `f582376`，记录提交 `5ff2d0c`，基线 `1cd0416`） |
| 实际检视范围 | `crates/core`（`broker.rs`/`model/json.rs`/`model/tests.rs`/`ports.rs`/`model/mod.rs`）+ `docs/CORE_PORTS_AND_STORAGE.md` + `docs/MODULE_ARCHITECTURE.md`；对照 `docs/SYNC_PROTOCOL.md` §10.3、`schemas/sync/v1/event-views.schema.json`、`crates/storage-sqlite/src/session_store.rs`、`crates/sync-protocol/src/views.rs` |
| 读取的规则/需求 | `openspec/schemas/agentic/roles/reviewer.md`、`proposal.md`、`specs/core-event-view-identity/spec.md`、`design.md`、`plan.md`、`tasks.md`、`verification.md`、`AGENTS.md` |
| 我实际执行的命令 | `cargo test --locked -p core --all-features`（基线/还原后各 1 次 + 逐变异）、4 条变异自检、2 条临时探针用例（跑完即 `git checkout` 还原）、只读 grep/`git diff`/python 集合比对 |
| 未执行（超出本轮范围） | `npm run check`（PV1）、`-p storage-sqlite`（PV5）、`-p agent-host`（PV3）、全工作区测试（PV4）；未复核 WP3/WP4 的用例接缝 |
| 版本稳定性 | 每次变异前核对 `git diff --stat f582376 -- crates/core docs/…` 为空，结束后 `git status --porcelain` 为空、`git diff --stat HEAD` 为空、`cargo test -p core` = `91 passed; 0 failed; 0 ignored`。**现象**：会话中途出现过一次瞬时 ` M crates/agent-host/src/mapper.rs`（`git diff` 为空、下一次 `git status` 即恢复、内容与 HEAD 无差异），疑似并发进程触碰 mtime；不影响本结论（我按固定提交 diff 与逐次核对的 `f582376` 内容判断）。 |

## 核实结论

### 1. 注入范围：只注入 `docs/SYNC_PROTOCOL.md` §10.3 要求的类型，与 §10.3 逐项一致（含 `turn.delta_compacted`）
- 表定义：`crates/core/src/broker.rs:165-181`（`TURN_ID_VIEW_EVENT_TYPES`，15 项，含 `turn.delta_compacted`）、`crates/core/src/broker.rs:184-185`（`SESSION_VERSION_VIEW_EVENT_TYPES`，2 项）。
- 门控调用点只有两处：`broker.rs:2524`（turnId 门）与 `broker.rs:2541`（version 门），都在 `finalize_owned_views` 内。
- 逐项比对（脚本，`schemas/sync/v1/event-views.schema.json` 的 `required` 集合 vs const）：
  ```
  schema turn : 15 ['agent.message.completed', … 'turn.delta_compacted', 'user.message.delta']
  core   turn : 15
  DIFF turn   : schema-core = []  core-schema = []
  DIFF ver    : schema-core = []  core-schema = []
  ```
  与 `docs/SYNC_PROTOCOL.md:1000-1016` 的表格也一致（§10.3 标题在 `:991`；`turn.delta_compacted` 见 `:1007`）。
- **不会**向未协商类型添加字段：注入完全由这两张表驱动，无第三个 `ensure_view_string_field` 调用点；反例已被用例固定——`views_without_turn_attribution_get_no_turn_id`（`broker.rs:6487`）用 `session.info.changed`（无归属）与 `terminal.output`（不在集合内）断言 view 逐字节不变。
- 结论：**符合**。

### 2. 单源归属：turn 只在批组装处解析一次，注入只消费该值；不存在 view 与落盘 `turn_id` 不一致的路径
- 唯一解析点：`broker.rs:1432` `let turn = event.turn.clone().or_else(|| running.clone());`，随后经 `PendingEvent::from_persistence(…, turn, …)`（`broker.rs:1483-1491`）写入 `PendingEvent.turn`。
- 注入只读该字段、不重推：`broker.rs:2528` `let Some(turn) = event.turn.clone() else { continue };`。
- 落盘同一字段：`crates/storage-sqlite/src/session_store.rs:1165` `.bind(pending.turn.as_ref().map(TurnId::as_str))` → `owned_event.turn_id`。**view 的 `turnId` 与 `owned_event.turn_id` 同源于同一个结构体字段，结构上不可能不一致。**
- core 全部**非测试** `OwnedCommit` 构造点（11 个）与漏斗核对（脚本枚举 `OwnedCommit {` → 最近的 `commit_owned(`）：
  | 行 | 所在函数 | commit_owned | view/事件携带的 turn |
  | --- | --- | --- | --- |
  | 1131 | `resolve_locked` | 1144 | 无事件 |
  | 1220 | `create_session` | 1236 | 无事件 |
  | 1546 | `commit_chunk` | 1559 | 同一个 `turn`（`:1490`） |
  | 1632 | `dispatch_one`（turn.started） | 1662 | `view_turn(&next.turn)` + `Some(next.turn)`（`:1650/1656`） |
  | 1693 | `fail_turn`（turn.failed） | 1741 | `view_turn_error(&turn.turn)` + `Some(turn.turn)`（`:1692/1712`） |
  | 1779 | `record_uncertain` | 1792 | 补写 completed：`:2096` 用同一 turn |
  | 1884 | `commit_compaction` | 1897 | `view_delta_compacted(turn)` + `Some(turn)`（`:1878/1880`） |
  | 2020 | `recover_command` | 2033 | `view_turn_error(turn.id())` + `Some(turn.id())`（`:1973/1975`） |
  | 2234 | `accept` | 2256 | 调用方构建（`turn.queued` 于 `:731/735`） |
  | 2295 | `apply_state` | 2308 | 无事件 |
  | 2361 | `commit_terminal` | 2374 | `command.*`，非 §10.3 要求类型 |
- 全仓唯一的 `store.commit(` 调用点在 `broker.rs:2502`（`commit_owned` 内），无旁路。
- 广播路径不二次拼 view：`publish`（`broker.rs:2455-2461`）只发 `CommittedDelivery::Owned(CommittedEvent)` 定位信息，正文由消费方从存储读（`ReadView::event_payload`），因此广播与落盘天然同源。
- 唯一"无归属"路径（我实测到）：**turn 终结后异步到达的 delta** → `turn_id = NULL` 且 view 无 `turnId`（两者仍一致，但 view 不满足 §10.3）。见发现 F1。
- 结论：**单源成立**。

### 3. 冲突失败关闭：真的零落盘/零发布/状态不变；「取值一致时字节不变」有可证伪断言
- 冲突判定：`ensure_view_string_field`（`broker.rs:3015-3032`）——`Text(existing)==value` → `Ok(view.clone())`（唯一"不改写"路径）；`Text(_)`/`NonText` → `PortError::InvalidRequest(conflict)`（`:3023`）；`Absent` → 前置插入后交 `ViewJson::new` 复核。
- 位置保证：`finalize_owned_views` 在 `deps.store.commit` **之前**调用（`broker.rs:2501-2502`），失败即在 `commit_chunk` 的错误分支 `Err(error) => Err(error)`（`broker.rs:1610`）上抛 → **零落盘**；该分支不调用 `publish` → **零发布**；该分支也不调用 `slot.finish_turn()`/不改 `slot.deltas` → **状态不变**。
- 用例 `a_conflicting_turn_id_fails_closed_without_side_effects`（`broker.rs:6529`）断言：错误是 `InvalidRequest`、事件类型序列严格 `["turn.queued","turn.started"]`、`publish_count()==2`（该批零帧）、turn 仍 `Running`、`world.event_turn` 无该批归属。
- 「取值一致时字节不变」可证伪：`a_matching_turn_id_is_kept_byte_for_byte`（`broker.rs:6567`）断言落盘 view **字符串等于**输入 view（`views.contains(&view)`），任何重排/重写/重复键都会不等。我用变异（见自检 3/4）确认该路径与冲突路径都能变红。
- 双重保险（非缺陷）：`insert_string_member_front` 自带同名键守卫（`model/json.rs:271-273`），即使调用方误在冲突分支走插入也会失败关闭。
- 结论：**符合**，且断言可证伪。

### 4. 版本语义：core 推导与 `session_store.rs` 一致；`replayed` 跳过比对合理；但真实 mode/config 流程下注入的是「变更前」版本
- 规则一致：`predict_session_version`（`broker.rs:2563-2596`）`state.is_some()` → `+1`、否则不变、`StateChange::Create` → 1；存储层 `StateChange::Update` 无条件 `version = version + 1 … RETURNING version`（`session_store.rs:1052-1080`）、`Create` 写 `1`（`:1005`）、无状态分支读回当前值（`:1090-1095`）。
- `expected_version` 为 `Some` 且**无**状态变更：存储层确实**不**校验（`:1090` 分支没有 `expected_version` 比较，只有 Update 分支 `:1000-1004` 有），而 core 会把它当"当前版本"（`broker.rs:2569`）。该组合今天**不可达**：唯一带事件的 `accept` 调用是 `submit_prompt`（`broker.rs:756`，事件仅 `turn.queued`），`cancel`/`mode.set`/`config.set`/`permission.resolve` 的 accept 事件为空（`:800/855/920/985/1045`）→ 见 SUGGESTION F7。
- `replayed` 跳过比对：`broker.rs:2505` `if let (Some(predicted), None) = (predicted, &outcome.replayed)`。**合理且必要**：真实存储在幂等命中时返回的是会话**当前**版本，不是首次提交的版本（`session_store.rs:872-887` 的 `session_version_and_epoch`），比对必然误报。对重放批而言注入字节被丢弃、返回首次落盘字节，无二次注入。
- `version` 注入范围正确：仅 `SESSION_VERSION_VIEW_EVENT_TYPES`（2 类），`broker.rs:2541` 单点驱动。
- 我实测的真实 `set_mode` 流程（临时探针，跑完已还原）：
  ```
  PROBE-mode-changed view = {"version":"1","currentModeId":"code"}
  PROBE-set_mode returned = 2
  PROBE-session version now = 2
  PROBE-all views = [("session.mode.changed", "{\"version\":\"1\",\"currentModeId\":\"code\"}"),
                     ("command.completed", "{\"requestId\":\"…\",\"result\":{\"turnId\":\"2\"}}")]
  ```
  即：`session.mode.changed` 是**独立的事件批**（`submit_mode_set` → `flush_locked` `broker.rs:865`），状态变更在**另一个提交**里（`apply_state` `broker.rs:2288/2295`）→ 注入值是变更前版本，且**没有任何事件 view 承载变更后版本**。见发现 F2（规范字面满足，语义需裁定）。

### 5. imported 路径：确实不注入、不重写
- `deliver_imported`（`broker.rs:1318-1352`）：先 `commit_receipt`（`ports.rs:472` 独立端口，只收摘要/索引），再 `publish(CommittedDelivery::Imported { … payload, payload_digest … })`（`:1342-1350`）原样转发；函数体内**没有** `finalize_owned_views`/`ensure_view_string_field`，也不经过 `commit_owned`。
- Check Plan Change 1 的理由成立且与代码吻合：入参只有 `payload: Option<EventPayload>`（`broker.rs:1325`，core 侧无 turn 来源），`payload_digest` 由调用方给出（`:1324`）并原样传给 `commit_receipt`（`:1335`）。
- 用例 `imported_deliveries_keep_the_owner_view_bytes`（`broker.rs:6826`）断言发布 view 与 Owner 字节**逐字相等**（含 `turnId`/`version`）、缺失时不伪造、索引摘要等于调用方给出的值。
- 结论：**符合**。

### 6. 端口与依赖：均未变，core 仍无 `serde_json`
- `git diff --stat 1cd0416..f582376 -- crates/core/Cargo.toml crates/storage-sqlite/src/migrate.rs schemas fixtures compatibility` → **空**（无新依赖、无 DDL/migration、无合同资产改动）。
- `crates/core/src/ports.rs` 只加注释（`:63-67`），`TurnAccepted`（`:69`）结构未变。
- `grep -rn serde_json crates/core` → 仅注释（`model/json.rs:4`、`model/mod.rs:18`、`ports.rs:7` 等），无依赖、无 `serde_json::Value`。
- 复跑基线：`cargo test --locked -p core --all-features` → `91 passed; 0 failed; 0 ignored`，与 `verification.md` 的 PV2 记录（91 passed）及 `reports/wp2-core-injection.log` 一致；该 log 头部记录 `cargo fmt --check` / `clippy -D warnings` / `check-crate-boundaries.mjs` 均 exit 0。
- 结论：**符合**。

### 7. 文档一致性：与代码事实一致、未把计划写成现状；但有 2 处文档自身缺陷
- `docs/CORE_PORTS_AND_STORAGE.md:242`（§5.1 `TurnAccepted.turn` 语义）与 `ports.rs:63-67`、`broker.rs:727`（`deps.ids.turn_id()`）、`broker.rs:1674`（`Ok(_) => Ok(true)`，不采信适配器值）一致，措辞未夸大。
- §6 第 19 条（`:750-752`）逐句与代码吻合：提交前注入、冲突 `InvalidRequest`、漂移 `PortError::Corrupt`（`broker.rs:2507-2511`）、`replayed` 不比对、imported 不注入。
- §9 判据 31（`:1268`）的 6 条与用例一一对应（①②③④⑤⑥ ↔ `broker.rs:6415/6487/6529/6567/6610/6637/6693/6787`）。
- `docs/MODULE_ARCHITECTURE.md` 新增的 §4.1 职责分工段与 §4.7「`owned_session.version` 由本 crate 在事务内实现、core 只按同一规则推导并比对」与 `session_store.rs:1052-1080` 事实一致。
- `verification.md` 的 Check Plan Changes 1–3 与实现一致：CPC1 见问题 5；CPC2 的"无状态变更时存储层不校验 `expected_version`"经 `session_store.rs:1000/1090` 核实；CPC3 的 `TurnAccepted.turn: TurnId` 必填经 `ports.rs:69-71` 核实。
- 缺陷：`docs/CORE_PORTS_AND_STORAGE.md:7` 新增"版本：0.7（2026-09-24…）"，与既有 `:14`"版本：0.7（2026-09-23…）"**重号**（该文件已有 0.8/0.9/0.10，新修订应为 0.11）→ F4。`plan.md:250` 的 PV2 通过判据仍写"imported 补齐（R4/R11）"，与 CPC1/spec/代码相反 → F5。
- 结论：主体**符合**，两处文字需修（非阻断）。

### 8. 代码质量：正常路径无 panic 类调用；错误类型语义恰当；无过度设计
- 新增**非测试**行中没有 `unwrap()`/`expect(`/`panic!`/`unreachable!`：用 diff 行号脚本过滤的结果里，测试模块（`mod tests` 起于 `broker.rs:4930`）之外唯一的命中是 `broker.rs:4734`，位于 `#[cfg(test)] pub(crate) mod test_support`（`broker.rs:3063`）内部。
- 错误语义：冲突/畸形输入 → `InvalidRequest`（`broker.rs:3023`，且错误信息刻意不带取值，避免把适配器数据带进协议错误，见 `model/json.rs:233-235` 注释）；**内部规则漂移** → `PortError::Corrupt`（`broker.rs:2508`）——两者分工与 `§6 第 19 条`一致。
- 可议点（非缺陷）：`top_level_member`/`insert_string_member_front` 的 `InvalidValue` 经 `impl From<InvalidValue> for PortError`（`crates/core/src/model/error.rs:144-148`）映射为 `InvalidRequest`，而对"已通过 `ViewJson` 校验的文本"而言失败更接近内部不变量破坏（`Corrupt`）；这属于措辞层的 SUGGESTION，不影响行为。
- 无过度设计：插入工具复用既有文本工具（`object_members`/`decode_json_string`）、只做前置插入、无新依赖；未使用代码仅 F7 指出的一个不可达分支。

## 发现

- **[SEVERITY: MINOR] `crates/core/src/broker.rs:2528`（注入条件）+ `openspec/changes/core-turn-view-fields/tasks.md:11`（"实测：无"）** — turn 终结后异步到达、且事件类型属于 §10.3 `turnId` 集合的事件（如晚到的 `agent.message.delta`）会产出**不含 `turnId`** 的 view，而 tasks.md/verification.md 记录"这类路径实测：无"、也没有用例固定该降级。**证据（临时探针，跑后已还原）**：
  ```
  PROBE-late-delta view = {"messageId":"…700","deltaIndex":"0","text":"late"}
  PROBE-late-delta stored turn = None
  PROBE-late-delta types = ["turn.queued","turn.started","turn.completed","command.completed","turn.completed","agent.message.delta"]
  ```
  落盘 `turn_id = NULL`，view 无 `turnId`（二者仍一致，符合 spec 的条件式要求，且 D4 允许"宁可缺字段"），但与本变更的目标（"使 view 在持久化与广播两条路径上都满足 §10.3"）在这条路径上不成立。**影响**：Sync 切片按 §10.3 逐字段校验时会拒绝这类事件；目前无任何登记。**建议**：① 在 `verification.md` 如实登记该路径（把 tasks.md 2.3 的"实测：无"改为"实测：turn 终结后晚到的 §10.3 类型事件"）；② 补一条用例固定该降级；③ 在 Sync 切片前裁定处置（core 侧按会话最近终态 turn 归属 / 由 Sync 对该缺失宽容 / 适配器保证终态在最后一条 delta 之后）。

- **[SEVERITY: MINOR] `crates/core/src/broker.rs:865` + `broker.rs:2288`（`apply_state`）— 版本字段语义落差**：真实 `session.mode.set`/`session.config.set` 流程里，`session.mode.changed` 与状态变更是**两个提交**，因此注入的 `version` 是**变更前**版本，变更后版本没有任何事件 view 承载。**证据**：同上探针输出 `session.mode.changed = {"version":"1",…}`、`set_mode` 返回 2、会话最终版本 2。**影响**：客户端用最后一个事件的 `version` 做乐观并发（`expectedVersion`）会被判 `VersionMismatch`；`session.mode.list` 返回的 `ModeState.version`（当前版本）与事件里的 version 不一致。**规范判定**：spec R8/R9 的字面要求（"该次提交后的会话版本"）**满足**，§10.3 未规定 mode/config 事件的 version 应取变更前还是变更后，所以**不判为违规**；但它与 design D2 的假设场景（"一次提交包含状态变更…该批中 mode.changed 取值等于该次提交后版本"）在真实流程里不会发生。**建议**：把 mode/config 的状态变更与 `session.*.changed` 事件合并为同一提交（可让注入值=变更后版本），或明确在 §6 第 19 条写"mode/config 事件承载变更前版本"；在 Sync 切片前裁定，否则该切片会踩到。

- **[SEVERITY: MINOR]（变更外既有缺陷，扫描中发现）`crates/core/src/broker.rs:2960-2971`（`view_command_completed`）+ `broker.rs:2343/2370`（`commit_terminal`）— `command.completed.result.turnId` 承载的是会话版本**：`apply_state` 把 `Some(version)` 传入 `commit_terminal`，`view_command_completed` 把它渲染成 `{"turnId":"<十进制版本>"}`。**证据**：探针输出 `("command.completed", "{\"requestId\":\"…\",\"result\":{\"turnId\":\"2\"}}")`；`docs/SYNC_PROTOCOL.md:1133` 的 `result.turnId` 示例是 UUID。**本变更未触碰这两处**：`git diff 1cd0416..f582376 -- crates/core/src/broker.rs | grep -c "command_completed\|commit_terminal"` → `0`。**影响**：`session.mode.set`/`session.config.set` 的终态把版本伪装成 turnId（本次探针里"变更后版本 2"只能从这个错名字段读到）；任何按 UUID 解析 `result.turnId` 的客户端会得到垃圾值。**建议**：登记为独立事项（不在本变更内修），例如 `result` 改为 `{"version":"2"}` 或 `null`。

- **[SEVERITY: MINOR] `docs/CORE_PORTS_AND_STORAGE.md:7` — 修订版本号重号**：新增行"版本：0.7（2026-09-24…）"与既有 `:14`"版本：0.7（2026-09-23…）"重复（文件已有 0.8/0.9/0.10，本次应为 0.11）。**证据**：`grep -n "^> 版本" docs/CORE_PORTS_AND_STORAGE.md` 输出两行 0.7。**影响**：修订历史无法区分两次 0.7（`check:docs` 不校验版本号，门禁不会拦）。**建议**：改为 0.11 并放在版本块末尾（现有块顺序已乱，如 0.9 在 0.8 之前）。

- **[SEVERITY: MINOR] `openspec/changes/core-turn-view-fields/plan.md:250` — PV2 判据未随 Check Plan Change 1 同步**：PV2 行仍写"必须覆盖：…**imported 补齐（R4/R11）**…"，与 CPC1、`specs/core-event-view-identity/spec.md` 的"保留 Owner 注入的归属"、`tasks.md:12`（"不注入、不重写、不补齐"）以及代码相反。**影响**：plan 的通过判据与 spec 冲突，按 plan 自己的 Failure and Recovery（"specs 改动 ⇒ 覆盖索引与受影响任务同步后重跑 plan 阶段检查"）属漏同步。**建议**：把该行改为"imported 保留 Owner 取值（R4/R11）"。

- **[SEVERITY: SUGGESTION] `crates/core/src/broker.rs:3023` — broker 层 `MemberValue::NonText` 无用例**：`top_level_member` 的 `NonText` 分支在 `model/tests.rs:387-390` 有单测（`number`/`nested`），但"适配器给的 `turnId` 不是字符串（如 `null`/`1`）→ 提交以 `InvalidRequest` 失败关闭"这条**集成**行为没有用例。**建议**：在 `a_conflicting_turn_id_fails_closed_without_side_effects` 旁加一个非字符串取值变体（几行即可）。

- **[SEVERITY: SUGGESTION] `crates/core/src/broker.rs:2569` — `(_, Some(expected))` 分支不可达且无覆盖**：当前生产路径中不存在"`expected_version = Some` + 无 `StateChange` + 含要求 `version` 的 view"的提交（见问题 4 枚举），也无任何用例走该分支；一旦将来出现，它会**先落盘再 Corrupt**（断言在存储返回之后）。**建议**：要么加用例把该组合的失败关闭语义固定下来，要么删除该快捷分支改为始终回读当前版本。

- **[SEVERITY: SUGGESTION] `crates/core/src/broker.rs:165-185` — 同一张 §10.3 表在四处重复**：`docs/SYNC_PROTOCOL.md:1000-1016`（文档表）、`schemas/sync/v1/event-views.schema.json`（机器 $defs）、`broker.rs:165-185`（core const）、`crates/agent-host/tests/view_contract.rs:25-52`（跨 crate 用例清单）。四者目前一致（我用脚本比对 schema↔const，DIFF 为空），但 `AGENTS.md` §5 要求封闭词表"只能有一处机器定义"。**建议**：把"core const == schema required 集合"断言纳入 `npm run check`（例如在 `scripts/` 下加一个小门禁或扩展现有 schema 门禁），并在 §6 第 19 条的注释里指向该门禁。

- **[SEVERITY: SUGGESTION] `crates/core/src/broker.rs:1382-1400`（`flush_locked`）— 冲突/漂移失败会丢弃该批已缓冲事件**：`flush_locked` 先把 `slot.pending` 整体 `take`，冲突失败时那批事件既未落盘也不回到缓冲，只有错误上抛。这与"失败关闭不得静默"一致，但代价（事件丢失、无重投）未在任何文档登记。**建议**：在 §6 第 19 条补一句"冲突/漂移失败时该批适配器事件被丢弃，由上层决定是否把 turn 判为失败"，避免下游误以为可重试。

## 变异自检记录

基线：`cargo test --locked -p core --all-features` → `test result: ok. 91 passed; 0 failed; 0 ignored`（real 0.42s，已缓存）。

| # | 变异内容（改坏） | 命令 | 观察 | 还原 |
| --- | --- | --- | --- | --- |
| 1（注入） | 从 `TURN_ID_VIEW_EVENT_TYPES` 删除 `"agent.message.delta"`（`sed -i '/^    "agent.message.delta",$/d'`，1 行） | `cargo test --locked -p core --all-features owned_views_get_the_authoritative_turn_id` | **红**：`panicked at crates\core\src\broker.rs:6448:9: assertion left == right failed: 注入必须只前置一个成员…` left=`{"messageId":…,"deltaIndex":"0","text":"hi","unknownField":{"nested":[1,2]}}` right=`{"turnId":"…0100","messageId":…}`；`test result: FAILED. 0 passed; 1 failed` | `git checkout -- crates/core/src/broker.rs` → 同用例 `ok` |
| 2（版本断言） | `broker.rs` 的 `if outcome.version != predicted {` → `if false && …`（比对不可达） | `cargo test --locked -p core --all-features a_version_rule_drift_fails_closed` | **红**：`panicked at crates\core\src\broker.rs:6686:70: 漂移必须失败: ()`；`FAILED. 0 passed; 1 failed` | 还原后同用例 `ok`（1 passed） |
| 3（失败关闭·静默保留变体） | `ensure_view_string_field` 冲突分支改为 `Ok(view.clone())`（=采纳适配器取值、放弃失败关闭） | `cargo test … -p core --all-features a_conflicting_turn_id_fails_closed_without_side_effects` | **红**：`panicked at …broker.rs:6544:72: 冲突必须失败: Accepted { request: RequestId("…901"), turn: Some(TurnId("…0100")) }`；`FAILED` | `git checkout` → 该用例 `ok` |
| 4（失败关闭·真覆盖写入） | 同时删除 `model/json.rs:271-273` 的同名键守卫 + 让冲突分支落到前置插入（=写出重复键的真覆盖） | 同 #3 命令 | **红**：`panicked at …broker.rs:6543:72: 冲突必须失败: Accepted { … }`；`FAILED` | `git checkout -- crates/core/src/broker.rs crates/core/src/model/json.rs` → `ok` |

额外注意（诚实记录）：我第一次尝试"把冲突分支改成覆盖写入"时**只改冲突分支、保留 `insert_string_member_front` 的同名键守卫**（即上表 #4 的一半），该用例**仍然是绿的**——因为守卫把它挡成了 `InvalidValue::Field`（`PortError::InvalidRequest`）。这**不是**断言弱化，而是失败关闭有两条独立防线；只有像 #4 那样把两处都改坏才会变红。需要的话可把这一层写进 §9 判据 31③ 的说明，避免读者误以为单点失效。

临时探针（不属于变异，跑完即还原）：`rv1_probe_late_delta_after_turn_end`（见发现 F1）、`rv1_probe_mode_change_version_split`（见发现 F2），输出已在上文引用。

**还原确认**：`git status --porcelain` 连续两次为空；`git diff --stat f582376 -- crates/core crates/storage-sqlite docs/CORE_PORTS_AND_STORAGE.md docs/MODULE_ARCHITECTURE.md` 为空；`git diff --stat HEAD` 为空；还原后 `cargo test --locked -p core --all-features` = `91 passed; 0 failed; 0 ignored`。工作副本行尾：`git checkout` 会把 `broker.rs`/`json.rs` 以 CRLF 写回（`core.autocrlf=true`，`git status` 视为未修改），检视前的副本为 LF；两者对 git 与编译等价，我未以任何方式"修好"仓库文件。

## 同类实例扫描

| 检索 | 命令 | 结论 |
| --- | --- | --- |
| 其它 view→存储/发布路径 | `grep -rn "store.commit\|commit_owned\|commit_receipt\|deps.publisher.publish" crates/core/src/` | 只有一条 owned 漏斗（`broker.rs:2502`，被 11 个构造点共用）；imported 走 `commit_receipt`（`broker.rs:1338`）后原样 `publish`（`:1342`）；`publish`（`:2455`）只发定位信息，正文由存储读 → **无第二条可改写 view 的路径** |
| 其它按事件类型分支的硬编码表 | `broker.rs:141`（`EPHEMERAL_EVENT_TYPES`）、`:145-160`（`persistence_policy`）、`:2935-2943`（`interaction_request_kind`）、`:2976-2984`（`turn_terminal`）、`:2887-2889`（`is_agent_message_delta`）、`event_origin` | 与 §10.3 字段表**不同维度**：前者决定持久策略/交互类别/turn 终态；我把 `persistence_policy` 的 ShortTerm 集合与 `docs/CORE_PORTS_AND_STORAGE.md` §6 与 `docs/SYNC_PROTOCOL.md` §10.3（主 Agent 注：原文此处把 §10.3 的归属误写成 CORE 文档；为通过文档引用门禁只更正归属，未改结论） 对照未见矛盾（`terminal.output`/`session.usage.changed` 为短保留、`agent.thought.delta` 不产 completed 见 §6 第 14 条）。`turn_terminal` 只认 completed/cancelled/failed，与 §10.3 的 `turn.*` 五类**无需**一致（queued/started 非终态），非漂移 |
| 同一 §10.3 表的多份拷贝 | `python3` 比对 `schemas/sync/v1/event-views.schema.json` 的 `required` ↔ `broker.rs:165-185` 的 const | turn 15/15、version 2/2，**DIFF 为空**；`crates/sync-protocol/src/views.rs:387-753` 的 15 处 `#[serde(rename="turnId")]` 也是同一集合（该 crate 是协议权威域）。漂移风险=四处手抄（见 SUGGESTION） |
| core 之外是否假设 view 已带 `turnId` | `grep -rn '"turnId"' --include=*.rs crates/ \| grep -v '^crates/core/'` | ① `crates/sync-protocol/src/views.rs`：`turn_id: Uuid` **必填**（如 `:387-400` 的 `AgentMessageDelta`）→ 证明本变更要解决的缺口真实存在，也说明 core 注入值必须满足 `uuid`（`TurnId` 用 `check_uuid`，`model/ids.rs:235-238`，已满足）；② `crates/agent-host/tests/view_contract.rs:157` 断言适配器**不**产 `turnId`（与 core 分工一致）；③ `crates/storage-sqlite/tests/compaction_recovery.rs:113` 直接写死 `turnId:"3333…"` 而 `owned_event.turn_id` 为空（测试自建行、绕过 core）。**无任何代码消费 core 的运行时 view 字节**（`server` 未实现） |
| 封闭词表门禁是否覆盖该表 | `grep -rn 'turnId' scripts/`；`package.json` 的 `check` 链 | `scripts/` 无任何脚本读取该表 → 目前**没有**门禁能发现 core const 与 schema 漂移（见 SUGGESTION F8） |

## 未解决阻断项

无（无 CRITICAL/MAJOR）。需主 Agent/作者裁定或登记的非阻断事项：

1. F1：turn 终结后晚到的 §10.3 类型事件（缺 `turnId`）——是登记为已知边界+补用例，还是纳入 core 归属策略（tasks.md 2.3 的"实测：无"需改写）。
2. F2：mode/config 切换的 `version` 语义（变更前 vs 变更后）——建议在 Sync 切片前裁定；当前 spec 字面满足，但有乐观并发陷阱。
3. F3：变更外既有缺陷 `command.completed.result.turnId = 会话版本`——建议另开事项登记（本变更不该顺带修，否则越界）。
4. F4/F5：两处文档缺陷（CORE 文档版本号重号 0.7、plan.md:250 "imported 补齐"）——建议在本变更内直接修（都属 §10 的"变更类型→权威文档"与覆盖索引同步义务）。
5. F6–F9：可选的断言/门禁强化，随 DU1 或后续变更处理即可。

```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "逐条回复了 8 个检视问题（含 file:line 证据）、4 条变异自检（真跑，含原始输出）、2 条临时探针（含原始输出，跑后还原）、同类实例扫描（schema↔core const 集合比对 DIFF 为空、唯一 store.commit 漏斗），并在「未解决阻断项」给出残余风险与需裁定事项；工作区已还原为干净状态（git status 空、91 passed）"
    }
  ],
  "changedFiles": [],
  "testsAddedOrUpdated": [],
  "commandsRun": [
    {
      "command": "cargo test --locked -p core --all-features",
      "result": "passed",
      "summary": "基线/还原后各一次：test result: ok. 91 passed; 0 failed; 0 ignored（与 verification.md 的 PV2 记录一致）"
    },
    {
      "command": "cargo test --locked -p core --all-features owned_views_get_the_authoritative_turn_id（变异 1：删除 const 中的 agent.message.delta）",
      "result": "failed",
      "summary": "预期变红：assertion left == right failed（缺 turnId 的 view vs 预期的 view）"
    },
    {
      "command": "cargo test --locked -p core --all-features a_version_rule_drift_fails_closed（变异 2：禁用 commit_owned 的版本比对）",
      "result": "failed",
      "summary": "预期变红：panicked: 漂移必须失败: ()"
    },
    {
      "command": "cargo test --locked -p core --all-features a_conflicting_turn_id_fails_closed_without_side_effects（变异 3/4：冲突分支改静默保留；再改成真覆盖写入+删除同名键守卫）",
      "result": "failed",
      "summary": "两种变异均变红：panicked: 冲突必须失败: Accepted { … }；只改冲突分支（守卫仍在）时为绿，说明有两条独立防线"
    },
    {
      "command": "cargo test --locked -p core --all-features rv1_probe_late_delta_after_turn_end -- --nocapture",
      "result": "passed",
      "summary": "探针（已还原）：晚到 delta 的 view 无 turnId、turn_id = None → 发现 F1"
    },
    {
      "command": "cargo test --locked -p core --all-features rv1_probe_mode_change_version_split -- --nocapture",
      "result": "passed",
      "summary": "探针（已还原）：mode.changed 注入 version=1（变更前），set_mode 后会话版本=2，无事件承载 2 → 发现 F2/F3"
    },
    {
      "command": "git status --porcelain && git diff --stat f582376 -- crates/core crates/storage-sqlite docs/CORE_PORTS_AND_STORAGE.md docs/MODULE_ARCHITECTURE.md",
      "result": "passed",
      "summary": "均为空：所有变异/探针已还原，工作区与固定版本一致"
    },
    {
      "command": "python3 比对 schemas/sync/v1/event-views.schema.json 的 required 集合 ↔ broker.rs:165-185 的 const",
      "result": "passed",
      "summary": "turn 15/15、version 2/2，DIFF 为空"
    }
  ],
  "validationOutput": [
    "cargo test -p core（基线）: running 91 tests → test result: ok. 91 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out",
    "变异1 输出: test broker::tests::owned_views_get_the_authoritative_turn_id ... FAILED / assertion `left == right` failed: 注入必须只前置一个成员，未知字段与既有字节不变",
    "变异2 输出: test broker::tests::a_version_rule_drift_fails_closed ... FAILED / panicked at crates\\core\\src\\broker.rs:6686:70: 漂移必须失败: ()",
    "变异3 输出: test broker::tests::a_conflicting_turn_id_fails_closed_without_side_effects ... FAILED / panicked at crates\\core\\src\\broker.rs:6544:72: 冲突必须失败: Accepted { request: RequestId(\"…901\"), turn: Some(TurnId(\"…0100\")) }",
    "变异4 输出: 同上 FAILED（真覆盖写入，需同时删除 model/json.rs:271-273 的守卫）",
    "探针1 输出: PROBE-late-delta view = {\"messageId\":\"…700\",\"deltaIndex\":\"0\",\"text\":\"late\"} / PROBE-late-delta stored turn = None",
    "探针2 输出: PROBE-mode-changed view = {\"version\":\"1\",\"currentModeId\":\"code\"} / PROBE-set_mode returned = 2 / PROBE-session version now = 2",
    "文档核对: grep -n '^> 版本' docs/CORE_PORTS_AND_STORAGE.md → 两行“版本：0.7”"
  ],
  "residualRisks": [
    "F1（MINOR）：turn 终结后晚到的 §10.3 类型事件产出缺 turnId 的 view；tasks.md 2.3 的“实测：无”不准确，无对应用例，Sync 切片会踩到",
    "F2（MINOR）：mode/config 切换的 version 注入的是变更前版本，变更后版本无任何事件承载；spec 字面满足但语义需裁定（乐观并发陷阱）",
    "F3（MINOR，变更外既有缺陷）：command.completed.result.turnId 承载会话版本字符串（无需在本变更修，但建议登记）",
    "F4/F5（MINOR）：CORE 文档版本号 0.7 重号；plan.md:250 的 PV2 判据仍写“imported 补齐”",
    "F6-F9（SUGGESTION）：NonText 无集成用例；(_, Some(expected)) 分支不可达且无覆盖；§10.3 表四处手抄且无门禁断言；冲突失败会丢弃已缓冲事件未登记",
    "本轮未执行 PV1/PV3/PV4/PV5（npm run check、storage-sqlite、agent-host、全工作区），WP3/WP4 用例的接缝真实性与断言强度由 rv1-wp3/wp4 负责；因此“本报告无阻断项”仅覆盖 WP1+WP2 静态与 core 用例范围",
    "会话中途曾出现一次瞬时 ' M crates/agent-host/src/mapper.rs'（内容与 HEAD 无差异，随即消失），提示本工作区可能存在并发进程；我的结论基于固定提交 f582376 的 diff，并在每次变异前后核对工作区一致性"
  ],
  "noStagedFiles": true,
  "diffSummary": "本轮未产出任何持久化改动：4 条变异与 2 条临时探针均已通过 git checkout 还原，终态 git status --porcelain 为空、git diff --stat f582376（crates/core、crates/storage-sqlite、docs/CORE_PORTS_AND_STORAGE.md、docs/MODULE_ARCHITECTURE.md）为空。",
  "reviewFindings": [
    "no blockers（无 CRITICAL/MAJOR）",
    "minor: crates/core/src/broker.rs:2528 + openspec/changes/core-turn-view-fields/tasks.md:11 - turn 终结后晚到的 §10.3 类型事件不注入 turnId（探针实测），记录“实测：无”不准确且无用例",
    "minor: crates/core/src/broker.rs:865/2288 - mode/config 切换的 version 是变更前版本，无事件承载变更后版本（探针实测 version=1 / session=2）",
    "minor: crates/core/src/broker.rs:2960-2971（变更外既有）- command.completed.result.turnId 承载会话版本（实测 {\"turnId\":\"2\"}）",
    "minor: docs/CORE_PORTS_AND_STORAGE.md:7 - 修订版本号 0.7 与 :14 重号（应为 0.11）",
    "minor: openspec/changes/core-turn-view-fields/plan.md:250 - PV2 判据仍写“imported 补齐（R4/R11）”，与 Check Plan Change 1/spec/代码相反",
    "suggestion: crates/core/src/broker.rs:3023 - MemberValue::NonText 在 broker 层无集成用例",
    "suggestion: crates/core/src/broker.rs:2569 - (_, Some(expected)) 分支当前不可达且无覆盖，且该组合会“先落盘后 Corrupt”",
    "suggestion: crates/core/src/broker.rs:165-185 - §10.3 表在文档/schema/core const/agent-host 测试四处重复，建议开门禁断言 const == schema",
    "suggestion: crates/core/src/broker.rs:1382-1400 - 冲突/漂移失败会丢弃该批已缓冲的适配器事件，代价未在 §6 第 19 条登记"
  ],
  "manualNotes": "范围与限制：本轮只覆盖 WP1（文档）+WP2（core 实现与回归），未运行 npm run check / -p storage-sqlite / -p agent-host / 全工作区测试（PV1/PV3/PV4/PV5 由对应 review 与主 Agent 的检查负责），因此本报告不能作为 PV 通过证据、也不能替代 DU1 复核。核心静态结论：注入集合与 docs/SYNC_PROTOCOL.md §10.3 及 schemas/sync/v1/event-views.schema.json 逐项相等（15+2，DIFF 为空，含 turn.delta_compacted）；注入只在 commit_owned→store.commit 之前发生，唯一 store.commit 调用点，11 个非测试 OwnedCommit 构造点全部经漏斗，view 的 turnId 与 owned_event.turn_id 同源于 PendingEvent.turn；冲突/漂移失败关闭零落盘零发布；imported 原样转发；端口/依赖/DDL 未变；新增非测试代码无 unwrap/expect/panic。所有变异与探针均已还原（git status 空、91 passed）。关于行尾：git checkout 会把 broker.rs/json.rs 以 CRLF 写回（autocrlf=true，git 视为未修改），检视前副本为 LF，对 git 与编译等价。"
}
```

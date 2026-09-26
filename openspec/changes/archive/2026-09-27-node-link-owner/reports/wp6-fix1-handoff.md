# WP6 fix1 Handoff — RV1-WP6 的 P1 阻断项与 P2/建议修复（`node-link-owner`）

## Shared Report

- **task_id**：RV1-WP6-F1（P1 阻断）、F2、F3、F6、F8、F10、F11、F12。
- **role / phase**：coder / fix（wp6-fix1）。
- **agent_context**：worker 子 Agent（本机 worktree 独占）。工作目录 `D:\Project\acp-remote-wt\node-link-owner`（分支 `agentic/node-link-owner`）；规划根 `D:\Project\acp-remote`。起点 `8bc2ff3e078549cebed2ab99b5981d5ed37e0581`（启动时核实一致、工作区干净）。
- **target_revision**：`f630859fec505773995adb5f6cf800c4c2da3115`（HEAD，含本修复全部提交）。三批提交：
  - `69419dd` **fix(node-link)**：RV1-WP6-F1 —— `session.create` 的幂等与终态落进 `owned_command`（core + `storage-sqlite` + `server::node_link` + `docs/CORE_PORTS_AND_STORAGE.md`）；
  - `709ef55` **fix(node-link)**：RV1-WP6-F2/F3/F8 —— 离场连接的进程内状态回收、终态/accepted 的收据分量透传、已知永久失败的错误码映射；
  - `f630859` **fix(node-link)**：RV1-WP6-F6/F10/F11/F12 —— 撤销通知回读持久状态、观察循环用例、死代码、`NODE_LINK_PROTOCOL.md` §12.7 注记。
  - **分批理由**：F1 是跨 crate 的纵向切片（API 形状 + 存储语义 + 文档），单独一批便于独立复核；F2/F3/F8 都在 `node_link/command*`（F3 另含 core 的两处终态结果），一批；F10/F11/F12 在 `local_admin` 测试缝与 `node_link` 用例、F6 是文档，一批。每批提交前都跑过 fmt + `npm run check` + clippy（pre-commit 钩子）与相应测试，三批的树各自可编译、可测。
- **scope**（允许写入范围内）：
  - `crates/core/src/{broker.rs,use_cases.rs}`（F1/F3）、`crates/storage-sqlite/src/session_store.rs` 与 `tests/commit.rs`（F1）；
  - `crates/server/src/node_link/{command.rs,command/tests.rs}`（F1/F2/F3/F8/F11/F12）、`crates/server/src/local_admin/{router.rs,test_support.rs}`（F10）；
  - `docs/CORE_PORTS_AND_STORAGE.md`（F1 的 §4/§5.1/§6）、`docs/NODE_LINK_PROTOCOL.md`（F6：§12.7 注记 + 文件头修订记录）。
  - **未改**：`openspec/**`、`schemas/**`、`compatibility/**`、`fixtures/**`、`crates/app/**`、`identity-*`、`node-link-protocol`（F1/F8 都只用了已登记的错误码，未新增 wire 码）。
- **checks**（最终 `f630859` 一轮，原始输出在 `reports/wp6-command.log` 的「wp6-fix1 轮次」分节）：
  - `cargo fmt --all -- --check` 退 0；
  - `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` 退 0；
  - `cargo test --locked --workspace --all-features` 退 0：71 个测试目标、累计 953 passed、0 failed；
  - `npm run check` 退 0（含合同漂移门禁：§5 的 15 个 trait / 93 个方法签名与 §7 的 36 条 DDL 逐条一致——本修复**未**改 §5 的 rust 块与 §7 的 DDL）。
  - 关键修复都有**红向证据**（先证明缺陷存在再修）：F1 两条（去掉存储回填 → 存储用例失败；短路「按持久记录判幂等」→ 重启后重复创建第二个会话）、F2/F3/F8 一条（逐条短路后 4 条用例失败）、F10/F12 一条（回读短路成「未撤销」+ 删掉 Shutdown 支 → 3 条用例失败）。详见日志。
- **issues**：无阻断项。需要主 Agent / reviewer 知晓的实现期结论见下「设计与口径说明」与「残余风险」。
- **result**：**PASS**（RV1-WP6 的 8 条发现全部处理；F4/F5/F7/F9 由主 Agent 另行裁定，本轮未改）。**不代表**独立 review、`[PV5]`/E2E 或合并已完成。
- **evidence_paths**：`reports/wp6-command.log`（含红向/绿向原始输出）、本文件、`reports/rv1-wp6.md`。
- **resource_cleanup**：只跑本机 `cargo`/`npm` 门禁与测试；无 listener、无端口绑定、无后台进程、无外部 Agent。用例仍走既有 fake 端口（`CommandStore`/`FakeBackends`/`RecordingCloser`）与临时目录守卫；F1 的存储用例用 `TempDir` 守卫自清理；未留下未提交工作区改动（`git status --porcelain` 为空）。

## handoff_index

```yaml
handoff_index:
  - { task_id: "RV1-WP6-F1", role: coder, phase: fix, stage: work-package, target_revision: "f630859fec505773995adb5f6cf800c4c2da3115", evidence_type: FIX, evidence_id: RV1-WP6, report_path: "reports/wp6-fix1-handoff.md", result: PASS, evidence_status: NEW, applicability_basis: "P1 阻断项：session.create 的幂等键 (ownerNodeId, accessNodeId, requestId) 现在落在 owned_command（创建提交里回填 session_id），终态（终态事件 + terminal_event_id）由 settle_session_create 提交；重启后同键重试回首次 sessionId（core/storage/server 三层用例，含重开存储与重建 route 的「重启」路径），同键不同指纹 → idempotency_conflict，崩溃窗口由既有 recover_unsettled 进 uncertain。红向证据：去掉存储回填 → storage 两条用例失败；短路「按持久记录判幂等」→ 重启后重复创建（commit 数 3 ≠ 2）。", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV1-WP6-F2", role: coder, phase: fix, stage: work-package, target_revision: "f630859fec505773995adb5f6cf800c4c2da3115", evidence_type: FIX, evidence_id: RV1-WP6, report_path: "reports/wp6-fix1-handoff.md", result: PASS, evidence_status: NEW, applicability_basis: "poll_pending 开头按 registry.handles() 回收不在存活集合里的 states 条目（限流窗口 + 观察表一起消失），判据不再依赖是否仍有 pending；用例 connection_state_is_reaped_once_the_connection_leaves_the_registry（红向：删掉回收调用即失败）。", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV1-WP6-F3", role: coder, phase: fix, stage: work-package, target_revision: "f630859fec505773995adb5f6cf800c4c2da3115", evidence_type: FIX, evidence_id: RV1-WP6, report_path: "reports/wp6-fix1-handoff.md", result: PASS, evidence_status: NEW, applicability_basis: "core 终态记录带上收据分量（turn 终结/取消 → {\"turnId\":…}，模式/配置切换 → {\"version\":…}；此前恒为 NULL）；server 的 command.accepted.result 透传 core 收据的 turn（session.create 仍为 null，schema 的 if/then）。用例：core terminal_result_carries_the_receipt_fields、server command_accepted_result_carries_the_receipt_turn、terminal_mapping 的「core 记录 result 为 NULL 时仍回 {}」。", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV1-WP6-F6", role: coder, phase: fix, stage: work-package, target_revision: "f630859fec505773995adb5f6cf800c4c2da3115", evidence_type: FIX, evidence_id: RV1-WP6, report_path: "reports/wp6-fix1-handoff.md", result: PASS, evidence_status: NEW, applicability_basis: "docs/NODE_LINK_PROTOCOL.md §12.7 补注本切片的结果投影范围（session.read/session.mode.list/session.config.list 回 nodelink.command.unsupported，不得返回被裁剪的结果），文件头加一行修订记录（check:docs 绿：378 links / 4131 section refs）。", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV1-WP6-F8", role: coder, phase: fix, stage: work-package, target_revision: "f630859fec505773995adb5f6cf800c4c2da3115", evidence_type: FIX, evidence_id: RV1-WP6, report_path: "reports/wp6-fix1-handoff.md", result: PASS, evidence_status: NEW, applicability_basis: "wire_error 新增三支永久失败映射（capability.unsupported_by_{client,broker,agent} 与 state.version_conflict → nodelink.command.unsupported；command.uncertain → nodelink.command.uncertain），都不再冒充 internal.unavailable + retryable=true；用例给正例（5 组码 → 非 retryable 的登记码）与反例（未知码与 session.busy 保持保守映射）。", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV1-WP6-F10", role: coder, phase: fix, stage: work-package, target_revision: "f630859fec505773995adb5f6cf800c4c2da3115", evidence_type: FIX, evidence_id: RV1-WP6, report_path: "reports/wp6-fix1-handoff.md", result: PASS, evidence_status: NEW, applicability_basis: "RecordingCloser 在 close_node/export_revoked 通知**当时**经 TrustStore::nodes_for / ExportStore::export 回读撤销状态并记录；router 的两条撤销用例断言回读值全为 true（红向：回读短路成 false 即失败，说明断言真的绑定在持久事实上）。", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV1-WP6-F11", role: coder, phase: fix, stage: work-package, target_revision: "f630859fec505773995adb5f6cf800c4c2da3115", evidence_type: FIX, evidence_id: RV1-WP6, report_path: "reports/wp6-fix1-handoff.md", result: PASS, evidence_status: NEW, applicability_basis: "删除 command/tests.rs 的 `let _ = node;` 死代码绑定与不再使用的 node 局部变量（clippy -D warnings 退 0）。", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV1-WP6-F12", role: coder, phase: fix, stage: work-package, target_revision: "f630859fec505773995adb5f6cf800c4c2da3115", evidence_type: FIX, evidence_id: RV1-WP6, report_path: "reports/wp6-fix1-handoff.md", result: PASS, evidence_status: NEW, applicability_basis: "补 dispatch 的观察循环两条用例：Shutdown 触发后任务必须在 5s 内结束（红向：删掉 select 的 shutdown 支即超时失败）；超龄观察项被放弃且不发帧。", source_evidence: NOT_APPLICABLE }
```

## F1 的实现取舍（供 reviewer 复核）

1. **幂等落在 `owned_command`，进程内表整体删除**。原 `session_creates`（`BTreeMap<(node, requestId), SessionCreateEntry>` + `MAX_TRACKED_SESSION_CREATES`）在持久化之后是纯粹的重复状态，且它的注释把淘汰语义写成「按插入序」而实现是 BTreeMap 键序（RV1-WP6-F1 顺带指出的不一致）。本轮的取舍是**删除**而不是保留成短期缓存：保留缓存会让「重启后」与「同进程内」两条路径给出不同的权威来源，而结果对象现在可以从持久记录逐字还原（`SessionCreateResult` 的原文就是落盘的结果），缓存不再带来任何收益。红向证据正是这条取舍的判据（短路持久记录查询 → 重启后重复创建）。
2. **`session.create` 是两次提交**：创建提交（`StateChange::Create` + 幂等行）与终态提交（终态事件 + `CommandTerminalRecord`）。因此一次成功的创建是 **2 次 `commit`**（原先 1 次），已有的 commit 计数断言随之改为 2；崩溃窗口正是这两次之间。
3. **存储层回填 `session_id`**：`IdempotencyRecord.session = None` 表示「装配期没有目标会话」（目前只有 `session.create`），存储层把创建事务里分配的会话 id 写进该行，并在幂等比对里**不**拿它参与比对（`command`/`kind`/`expected_version`/`request_fingerprint` 四项仍恒比）。理由：终态提交与启动恢复都按 `(session_id, request_id)` 定位该行，回填前两处都定位不到；而 §3.1 明确 `SessionId` 只能由存储层在事务内分配，core 不得预生成。已写进 `docs/CORE_PORTS_AND_STORAGE.md` 的 §6 第 20 条与 §5.1/§4；§5 的 rust 块与 §7 的 DDL 未动（无新端口方法、无新列）。
4. **终态由适配层投影、core 只负责落盘**：`settle_session_create(actor, requestId, status, result, error)` 的 `result` 是 Node Link 的 `SessionCreateResult` 原文（core 不能也不该构造它：它要 `exportId` 与 `sessionMeta` 投影）。回读时把该原文还原成 `CommandResultPayload::SessionCreate`，因此首次回复、重试与 `command.status` 重查三者逐字同形，且 `terminalEventId` 非 null。
5. **创建在幂等行落盘前失败**（授权、本机 workspace 解析、写盘失败）没有记录可回读：本地合成同形 `failed` 终态（`create_terminal` 保留的唯一用途）。这类失败是确定的（同一请求重试得到同一结果），已在代码注释里写明；`command.status` 对这类 requestId 回 `nodelink.command.not_found`（从未落盘）。

## 设计与口径说明（需要知晓）

- **F3 在 `command.status` 重查路径上的边界**：未终结的 mutation 经 `command.status` 重查时 `command.accepted.result` 为 `null`。turn 归属不在持久命令行里（它在 `owned_turn.causation`），而重查的单点读入口只有命令行本身；首次提交与同键重试都带 `turnId`（重试走 broker 幂等路径，`turn_for_request` 从 turn 表恢复）。已写进 `send_accepted` 的文档注释。若 reviewer 要求重查也带 turn，需要新增一条「按 requestId 取未终态 turn」的用例入口（core 用例面变更），不属本轮授权范围。
- **F8 的 `state.version_conflict` 映射**：按 RV1-WP6-F8 的裁决映射为非 retryable 的 `nodelink.command.unsupported`。语义上「版本冲突」在 Access 重新 attach/重读版本后仍可能成功，但未修改的重提交必然再次冲突，且 Node Link 的登记码表里没有版本类错误码（挂到 retryable=true 的 `internal.unavailable` 正是被指出的缺陷）。取舍与理由写进了 `wire_error` 的函数文档；真正未知的 core 码与 registry 里登记为可重试的码（如 `session.busy`）保持保守映射（不谎报「永久」）。
- **观察到但**未**修的既有不一致（不属本轮发现，未擅自改协议可见字节）**：`view_command_completed(request, Some(version_text))` 把**模式/配置切换的版本**写进了事件 view 的 `result.turnId`（`{"result":{"turnId":"5"}}`）。该 view 是持久化的事件 payload（Sync 的 `command.completed` view 只要求 `result` 是 object，未约束字段名），改它会动重放可见字节与相关 fixtures，因此保留原样，登记为本轮残余观察项。

## 残余风险

1. **`command.status` 重查未终结 mutation 时 accepted 不带 `turnId`**（见上）：对「首次回复 vs 重查」的严格同形要求是一个已登记的口径缺口，不属于 R66 明文（R66 只要求两种 `command.status` 形式同形，那条有用例）。
2. **超龄放弃分支的用例依赖直接构造 `since`**（`Instant::now() - (WATCH_MAX_AGE + 1s)`），没有为它引入测试缝；`WATCH_MAX_AGE` 本身仍是固定常量。
3. **F9（`RevocationCloser` 只记日志）与 F4/F5/F7 未触及**：按主 Agent 裁决留给 WP7 / 已同步到 spec/design。
4. **`[PV5]`/E2E 仍未跑**：本轮的 `nm run check` 与 workspace 测试是 `[PV3]` 级别；F1 的「重启」路径在 core fake 与重建 route 的层面覆盖，真实 SQLite 的跨进程重启链路仍待候选门禁（3.11/2.22）。

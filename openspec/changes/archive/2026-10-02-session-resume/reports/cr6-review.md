# CR6 独立检视报告（WP6 / branch / work-package / Round 1）

> 持久化说明：本报告由 reviewer 实例（只读工具集，无写权限）以全文返回、由主 Agent 原样落盘。run `208b74bb-275f-4c9b-adae-2dd5f5a83df6`。

## Review

**结论：PASS**（0 CRITICAL / 0 MAJOR）。3 条非阻断发现（2 MINOR + 1 SUGGESTION）。

**隔离方式**：全新实例，只收到本轮指令（角色契约 + 派发简报），未参与 WP6 实现；工作树在 `76ab01126114` 上**干净无未提交改动**。**工具限制（必须记录）**：我可用的 `watchdog_diff` 只覆盖「工作树 vs 启动 HEAD」，不含已提交范围，因此**无法生成 `5ab7e9d..76ab011` 的权威 diff**；本轮结论基于对目标版本**内容**的逐文件阅读与交叉比对，写入范围（F 项）因此只能按内容与实现者报告推断，不是 diff 级证据。

---

### A–F 逐项结论

**A. 路由正确性 —— 通过**
`on_submit`（`command.rs:246-268`）→ `CommandName::SessionResume => self.on_session_resume(...)`。
`on_session_resume`（`:985-1201`）顺序：payload 判别 → `session_target`（**只解析 `sessionRef`，不查任何本机记录**）→ `authorize_node(node,"grant.remote-work",Some(&target.export))` → `attachment_is_current` → 幂等回读 → in-flight → `accepted(null)` → `core.resume_session`。授权严格先于 `resume_session`；core 侧同样先 `authorize` 再 `load_recovery`（`broker.rs:1494-1502`），双层成立。
**payload 非空即拒**：`forbidden_payload_field`（`:2252-2290`）对 `CommandName::SessionResume` 的 `rejected` 恒为 `true`，在严格解码**之前**生效 → `command.rejected(nodelink.command.unsupported_field)` + `details.field`。单测（`tests.rs:1151`）与路由用例（`:2746`）双覆盖。
**越权无副作用**：拒绝路径只写一条 `authorization.denied` 审计（R69/R82 明确要求，`command.rs:1437-1452`），不碰命令/会话状态；用例以 `commit_calls` 不变作证。

**B. 终态语义 —— 通过（本轮重点，逐条核对）**
- **DR1-F39 终态唯一**：全函数只有一处构造 `Uncertain`——投影失败分支（`command.rs:1097`）。`Err(error)` 单一分支一律 `LocalOutcome::Failed(port_error_code(&error))`（`:1113`）。能力未宣告 / 两列 NULL / cwd 复校验失败 / payload 非法 / 越权五类确定失败**没有任何一处用 `uncertain` 掩盖**。
- **能力未宣告**：`Unavailable(BackendUnsupported)` → `port_error_code` → `"nodelink.command.unsupported"` → `wire_error` → `ErrorCode::CommandUnsupported`（`error.rs:135` = `"nodelink.command.unsupported"`，registry `retryable:false`，`errors.json:79`）→ 终态 `failed`。用例 `session_resume_without_persisted_recovery_data_fails_as_unsupported`（`tests.rs:2836`）同时断言 `status=="failed"` 与该错误码，能区分 `failed`/`uncertain`。
- **CR3-F1**：`command.rs:775-785` 在 `create_session` 出错后先 `command_status` 回读，有持久行（= 会话行已随同一次提交落盘）→ `Uncertain`，无行 → `Failed`。在位。
- **DR1-F41 同源与成对**：`session_resume_result`（`:926`）与 `session_create_result`（`:918`）**共用** `remote_session_ref_and_meta`（`:943`），形状必然一致。`settle_session_resume`（`:1126-1176`）与 outcome 三臂一一成对；投影失败 → 不 settle `Completed`、改结 `Uncertain`（既有模式）。core 侧 `settle_session_resume` 显式只终结 `command=="session.resume"` 的记录，否则 `InvalidRequest`（`broker.rs:1656`），不误伤其它命令。

**C. 能力门控口径 —— 通过**
实现口径正确且与文档一致：门控在 `initialize` 之后、`session/resume` 发送之前；未宣告时 `discard_spawned_runtime` 回收本次拉起的子进程、**一个字节都不发**（`agent-host/src/host.rs:608-640`），`NODE_LINK_PROTOCOL.md:656` 如实写明「能力只有经 `initialize` 才能得知，因此能力不支持的判定发生在进程拉起之后、发送之前」。WP6 用例中「不启动进程」的断言只出现在**两列 NULL** 场景，而该路径 core 在调用 `backends.resume` 之前就 `return`（`broker.rs:1500-1502`），确实无 spawn——断言没有错误地要求「无 spawn」。注：路由层**没有**覆盖「spawn 后后端报不支持」的用例（`FakeBackends::resume` 恒成功），留交 Coverage Index 判断，非缺陷。

**D. 编译涟漪与错误码封闭 —— 通过**
- `core_payload`（`:2596-2602`）：`WirePayload::SessionResume(_) => return Err("session.resume is dispatched by its own handler")`，与 `SessionCreate` 同形早退，**未**映射到 core payload；该函数仍无 `_ =>` 通配臂。
- `port_error_code`（`:2500-2513`）：`Unavailable(BackendUnsupported) → "nodelink.command.unsupported"`，复用既有码；`wire_error` 词表未新增；`compatibility/errors/v1/errors.json` 中无 resume 相关新码。
- 五个替身均已实现 `load_recovery`：`CommandStore`（`tests.rs:343`）**按 §3.6 口径**把「行不存在/任一列 NULL」收敛为 `Ok(None)`，不推导不补齐；`SliceStore`/`NotTouched`/`FixedStore` 用 `unreachable!`（「本路由用例不读恢复数据」的显式信号，非默认值糊弄）；`FlakySessionStore` 原样转发 `SqliteStore`（`owner.rs:458-463`，故障注入只作用在 `commit`）。`SessionBackendFactory::resume` / `SessionEndpoint::agent_session_id` 亦齐。
- **新 trait 方法无默认实现**：`ports.rs:104-111`（`resume`）、`:119`（`agent_session_id`）、`:409-412`（`load_recovery`）均以 `;` 结尾 → 编译期即证明每个实现体都被补齐，替身没有被塞默认值绕过。

**E. 测试质量 —— 通过（6 项逐条核实）**
① `session_submit_body` 的 scope 白名单已含 `"session.resume"`（`tests.rs:966`）→ **用例现在真的走到了路由**（8 条用例都断言了 `command.accepted`/`command.terminal`/`command.rejected` 的具体帧内容，若仍在信封层被 `schema_invalid` 拦掉，这些断言不可能成立）。② 终态错误码走 `body["terminal"]["error"]["code"]`（`:2856`、`:2897`），路径正确。③ 成功用例断言 `sessionMeta.state == "idle"` + `remoteSessionRef` 三字段，不再断言 wire 上不存在的 `sessionMeta.sessionId`。④ `invalid/session-resume-with-cwd.json` 已登记进 `conn/tests.rs:2281` 的 `deferred_body_cases`（该清单要求每个 `valid:false` 业务族 fixture 显式登记）。⑤ 全仓 `zz_debug_resume` grep = 0 命中。⑥ 编辑残片已消失，`wp6-coder-f2-PV1-fmt.log` exit 0。
未发现恒真/空转断言，唯一例外见 CR6-F2。

**F. 边界与规范 —— 通过（附工具限制）**
生产路径无 `unwrap()`/`panic!`；`command.rs` 仅两处 `expect`，都在**固定短字面量**的兜底上（`node_revocation_reason`、`error_info`），既有代码。文档内容与实现、机器词表一致：`NODE_LINK_PROTOCOL.md:651-662`（payload 空对象、终态契约、cwd 复校验回 `nodelink.internal.unavailable`）、`:656`（能力门控口径）、`:664`（投影范围收窄注记）；`SESSION_CONTINUITY_DESIGN.md:3` 与 `README.md:26` 的状态陈述与代码相符，未把未实现能力写成已存在。

---

### 第 5 节开放问题的独立判定

**「组合根接线是 no-op」——确认成立，且恢复路径在 app 的真实组合下确实可达，不存在「替身能过、真实装配走不通」。** 依据：

1. **接线已由上游闭合**：`compose.rs:194`（`session_store: Arc<dyn SessionStore> = store.clone()`，`store` 是 `SqliteStore::open` 的同一实例）→ 注入 `BrokerDeps.store`；`compose.rs:228`（`backends: Arc<dyn SessionBackendFactory> = host.clone()`）→ 注入 `BrokerDeps.backends`。因为 WP3冻结的三个方法**没有默认实现**，「`app/src/**` 不需要改」是编译期事实，不是判断。
2. **真实实现可用**：`SqliteStore::load_recovery`（`session_store.rs:2059-2088`）真读 `agent_id, agent_name, agent_session_id, workspace_cwd`，行不存在/任一列 NULL → `Ok(None)`，非法取值按列值损坏报错，不返回半条记录；`AgentHost::resume`（`host.rs:608-681`）真实重拉进程、按 `supports_session_resume()` 门控、成功后 `evict_binding` 保证单一活跃绑定，并把**持久化的** `agent_session_id` 交给端点。
3. **可达链路闭合（关键）**：写入侧 `host.rs:545` `AgentSessionId::new(&response.session_id)` → `SessionInit.agent_session_id` → `SessionEndpoint::agent_session_id()` → core `create_session` 紧接着提交两列（`broker.rs:1446-1460`）→ `owned_session` v5 两列非 NULL（`migrate.rs:19/67-68/636-637`）。因此真实创建的会话**有**恢复数据，`load_recovery` 返回 `Some`，恢复路径可达、不是恒定 `unsupported`。
4. WP6 在 `crates/app/` 的实际改动只有**测试替身**（`tests/support/owner.rs`）的编译涟漪，不涉及 `app/src/**`。

---

### Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation |
| --- | --- | --- | --- | --- | --- |
| CR6-F1 | MINOR | `crates/server/src/node_link/command.rs:1005-1035`（`on_session_resume` 幂等分支）+ `:2596`（`fingerprint_of(&submit.payload)`） | `session.resume` 的 payload 恒为 `{}`，语义全部落在 `sessionRef`。同一 `(owner,access,requestId)` 改 `sessionRef` 指向**另一个同样已授权**的会话时，指纹恒同 → `same==true` → 直接回**首次会话**的终态结果，而非 `nodelink.command.idempotency_conflict`。core 的幂等行本就带 `session`（`broker.rs:1519`），差异可判定但未被用 | 误用/恶意的 Access Node拿到 A 的 `remoteSessionRef` 却以为恢复的是 B。**非本包引入**：同一形态在 `on_dispatched`（全部既有 session-scoped mutation）同样存在，§12.5 只要求「命令名或 payload 指纹」；但 resume 是第一个 payload 恒空、该盲区**必然命中**的命令 | 在 resume 的幂等比对里追加 `record.session() == Some(&target.session)`，不一致回 `nodelink.command.idempotency_conflict`（core 侧 `IdempotencyRecord.session` 已具备该信息）；或按 §10 在 §12.7 登记「同一 requestId 不得跨会话复用」。**不建议**顺手扩到通用 mutation 臂（超出本包范围） |
| CR6-F2 | MINOR | `crates/server/src/node_link/command/tests.rs:2793-2833`（用例 `an_unauthorized_session_resume_is_rejected_before_any_local_read`）；`tests.rs:343-357`（`CommandStore::load_recovery` 无读计数） | 用例名与文档声称「不读取会话行」，但唯一断言是 `store.commit_calls()` 不变。`CommandStore` 没有 `load_recovery` 调用计数，因此该用例**无法区分**「授权先于本机读取」与「授权先于副作用提交」 | 路由层的 R31 证据强度低于其名称所宣称；真正的区分证据在 core 侧 `resume_session_authorizes_before_reading_the_session_row`（`broker.rs:2308`，用 `recovery_read_count`）。不是假绿（断言为真且有意义），但覆盖面被高估 | 给 `CommandStore` 加 `recovery_read_calls` 原子计数，在被拒路径断言为 0；或把用例名/注释改成它真正证明的东西 |
| CR6-F3 | SUGGESTION | `command.rs:1178-1199`（`local_terminal` 兜底） | 恢复成功但 `settle_session_resume` 因存储不可用返回 `Ok(false)` 时，Owner 本地发一帧 `completed`（`terminalEventId: null`），而持久记录仍是 `accepted`——`command.status` 重查回 `accepted`，启动恢复按 §6 第 16 条改写为 `uncertain` | 与 `session.create` 的既有形态同源（RV2 已在 §12.7 `:649` 登记该收窄），**非本包引入**；但 resume 的副作用（拉起进程）更重，口径值得同样登记一次 | 在 §12.7 `session.resume` 的结果契约补一句同类注记（与 `:649` 同形），或在 §12.7 统一表述为一条跨 `create`/`resume` 的收窄 |

---

### Assessment

- **本轮结论：PASS**（0 CRITICAL / 0 MAJOR）。静态审查已完成 A–F 全部范围。
- **待补证据（不影响本轮代码判断）**：① `cargo fmt --all -- --check`、`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`、`cargo test --locked --workspace --all-features` 的 **workspace 全量**在 `76ab011` 上的执行记录——实现者只跑了 `-p server -p app` 与 workspace `clippy --keep-going`（`reports/wp6-coder.md:194-198`），workspace `test` 明确留给候选阶段。该门禁属 PV1 阶段 2，不阻断本轮工作包判定。② 已提交范围的权威 diff：本轮工具无法生成，F 项的写入范围需在集成阶段由 merger 用真实 diff 复核。
- **接缝风险专项**：代码由 `coder-F` 与 `coder-F2` 两个实例接力产出，接缝处（`command.rs` +408 行由前者写、6 处测试构造缺陷由后者修）我逐项核对后**未发现语义割裂**：实现路径自洽、测试确实走到路由、错误码路径与文档/词表三方一致。

### handoff_index

`task_id`: `2.6` / `3.7`（WP6 的 DELIVERY/REVIEW 行）· `role: reviewer` · `phase: branch` · `round: 1` · `stage: work-package` · `target_revision: 76ab01126114294e7064e2618eedad0615b2561d` · `evidence_type: REVIEW` · `evidence_id: CR6` · `result: PASS` · `evidence_status: NEW`

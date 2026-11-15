# R1 独立测试差异定义：Source schema 11

状态：tester 独立定义，等待 reviewer 逐项冻结；不表示产品已实现、已编译或已验收。

本定义只覆盖 `REBASE-PLAN.md` 的 R1（Source schema 10 到 11）。不覆盖 R2 discover/create、R3 WSS/Web 或外部 Harness。受测实现必须保留 schema 10 的 native release 表、索引、数据和身份事实，并在同一 SQLite `BEGIN IMMEDIATE` 迁移事务中增加 Source 的两表两索引。

## 固定输入与隔离

- 规格和边界：`REBASE-PLAN.md`、`REVIEW-R1-PLAN.md`；Source 四个 DDL 的参考叶为方案记录的 `source_sessions_schema.rs` SHA-256 `de8a8122773418f8547d1c0cf3896260a8009fa4e05a3f07f96d28aff584f4b8`。
- Rust 工具：仓库锁定的 Rust 1.98.1，`--locked --offline`；独占 `CARGO_TARGET_DIR` 和临时 SQLite 数据目录。不得复制或复用不匹配的 PE，不得改 Git index，不运行 `npm audit`。
- 测试前由 runner 以 `cargo test --lib -- --list` 绑定实际限定名；每个测试使用 `--exact --test-threads=1 --nocapture`，记录自然退出、进程树清理和数据库路径。零测试、仅启动或历史日志不算通过。
- fixture 必须在独立临时数据库中创建并在进程结束后删除；报告保存每项迁移前后 `sqlite_schema`、`node_meta.schema_version`、`PRAGMA main.user_version`、release 行计数及关键表/索引清单。

## R1 用例

以下为 L3 数据库合同与 L5 节点启动存储入口的增量判据。L3 运行真实 SQLite，不用 SQL mock；L5 复用实际 initialize/preflight 入口验证拒绝。每项均须断言迁移结果、双版本头、存在于输入中的旧数据以及失败后的数据库状态；“通过”要求实际断言执行而非只检查返回码。

| ID | 场景和最小判据 |
| --- | --- |
| R1-01 | **schema 0 fresh**：空 main 数据库初始化到 11；四个 Source DDL、旧 core/session/release 结构均存在，metadata 与 `PRAGMA main.user_version` 都为 11，空身份检查通过。 |
| R1-02 | **精确 schema 9→10→11**：用精确旧 9 布局（不得只删除 release 表伪造；不得残留 schema 10 或 Source 对象）穿过原 release 10 步骤及 Source 11；旧 schema 9 canary、身份和原布局均保留，成功后 release 表/索引完整；原 release 三故障点仍须各回滚到精确 9。9 不可能预置 release 行，含行保留由 R1-03 单独证明。旧 release 测试的 `old_layout` 必须同时排除后继 Source 对象，但“回滚到 9”快照须显式断言 Source 四对象均不存在，防止排除过滤掩盖迁移遗留。 |
| R1-03 | **精确 schema 10→11**：独立含 release 表/索引/至少一行 release 数据及有效身份的 10 fixture；只增加两表两索引，版本头从 10 同步到 11，原表 SQL/行/索引摘要不变。 |
| R1-04 | **schema 11 reopen**：已完整 11 布局重开只验证不重建、不重复发送任何 session RPC；重复初始化后所有结构/行/双头摘要相同。 |
| R1-05 | **四 DDL before/after 故障**：分别在每个 Source table/index DDL 之前及之后注入故障（共 8 个点），断言迁移返回预期 migration error，事务回滚到精确输入布局，版本头仍为输入值；不得留下半个表或索引。每个故障使用独立 fixture，避免跨例污染。 |
| R1-06 | **Source metadata/pragma/validation 故障**：分别在写 `node_meta`、写 `PRAGMA main.user_version`、最终完整布局验证前后注入故障；断言旧布局/版本头精确回滚，且不以自动修复或重建掩盖结构错误。 |
| R1-07 | **结构拒绝负例**：在 schema 10/11 fixture 中逐项加入 main/temp trigger、额外非预期 index、`temp` shadow table/view、Source 仅部分布局、错误列/外键、已有 Source 数据的外键破坏；独立枚举 metadata/`main.user_version` 不一致（10/11、11/10）、未知高版本、旧版本夹带 Source 布局。初始化必须拒绝并保留原始结构，不能静默删除/补建。对 profile/source/harness/workspace 外键逐组检查约束定义与坏引用数据；schema 11 缺表/缺索引也拒绝。 |
| R1-08 | **真实 deferred-FK COMMIT 失败**：在已完成四 DDL、双头写入和验证后，使用真实 `DEFERRABLE INITIALLY DEFERRED` 外键违规使 COMMIT 返回失败；断言所有迁移写入回滚，双头与 release 数据回到输入状态，不能把执行中间状态报告成 11。 |
| R1-09 | **COMMIT 报告丢失**：在真实 COMMIT 成功后模拟返回报告丢失（现有 commit-unknown fault 语义）；断言调用报告错误但数据库保持完整 11 布局、四 DDL/双头/release 数据均存在，重开只验证且不得重发会话 RPC；验收报告明确“实际已提交、报告丢失”，不能凭调用错误伪称数据库回滚。 |
| R1-10 | **身份和 release 保留**：合法无 `node_meta` 身份行且所有必要表空的 11 数据库允许初始化；无身份但任一 Source 表非空必须拒绝，逐表覆盖；多/冲突身份拒绝。schema 11 的空身份检查包含 Source 两表，10 的检查不得查询不存在的 Source 表。10→11 后 release actor、撤销/审批事实、旧索引及 release 行逐列/逐行相等。 |
| R1-11 | **最新版本与精确旧版本分离及受影响 fixture**：分别验证精确 9 回滚路径、精确 10→11 成功路径和 11 重开；不得用“初始化 11 后删 release 对象再改版本头”的混合 fixture 代替 schema 9。更新 `node_catalog_schema/tests.rs::synthetic_eight()`：先按依赖顺序删 Source indexes、Source 子表再删 Source 父表，再删 catalog/release 对象及改双头为 8；其 exact rollback snapshot 必须断言 Source 四对象不存在、catalog/release 结构和双头回到精确 8。确受影响开发测试叶：`app/src/session_storage/schema.rs`、`app/src/session_storage/native_release_schema/tests.rs`、`app/src/session_storage/node_catalog_schema/tests.rs`，以及新增 `app/src/session_storage/source_sessions_schema.rs` 和新增/扩展 Source 开发测试叶；最终执行清单逐叶绑定实际名称。 |

## 推荐测试入口（实现后绑定）

实现应在受影响的 schema/release 测试叶中提供稳定限定名，至少覆盖：`schema11_upgrades_exact_nine_and_preserves_release_layout_and_data`、`schema11_upgrades_exact_ten_and_preserves_release_rows`、`schema11_reopen_is_validation_only`、`source_ddl_faults_roll_back_to_exact_input`、`source_metadata_pragma_validation_faults_roll_back`、`source_structural_negatives_are_rejected`、`source_deferred_commit_failure_rolls_back`、`source_commit_report_loss_keeps_complete_schema11`，并保留并适配 `synthetic_eight` 的 catalog 故障回归和 `release_precommit_faults_roll_back_to_exact_nine` 的精确 9 断言。最终 runner 必须以 `--list` 输出绑定实际名称；名称不存在或执行数不符即阻断。

## 交付判定与限制

R1 仅在 R1-01 至 R1-11 全部真实执行并通过、独占数据库和进程自然收敛、源/索引前后身份不漂移后才可报告“R1 通过”。本文件不授权产品写入、不证明 R2/R3、不代签完整 WP4。任何未执行项、工具/权限阻断、测试 PE 与源不匹配或历史结果复用都必须逐项标为未验证/受阻。
---

# R3 独立测试差异定义：认证 WSS、Web 与外部原生 ID 读取

状态：tester 独立定义，已对齐当前代码基线细节并可进入 R3 冻结审阅；不表示 R3 产品已实现、已验收或已放行合入。本节追加于既有 R1 定义之后，完整保留 R1 原文不变，覆盖 `REBASE-PLAN.md` R3 与 WV15 处置，承接 `specs/structured-history/spec.md`、`specs/harness-management/spec.md`、`tasks.md` 3.1/4.1/4.2 及 `CL-061`/`CL-062`/`CL-063`/`CL-065`/`CL-085`（不改变各任务完整完成判据）。

## 固定输入、冻结合同与执行门禁

- **封闭 WSS 动作与校验（`app/src/management_ws/session_protocol.rs` 与 `clients/web/src/management/session-protocol.ts`）**：
  1. `source.session.register`：`{ selection: { sourceId, harnessId, profileId, profileRevision, workspaceId, launchPath }, operation: { kind: "discover", cursor: null | string(1..=4096) } | { kind: "create" } }`，其中 `launchPath` 非空且 UTF-8 长度 `<= 4096` 字节；基于域隔离 SHA-256 `(actor.client_id, command.requestId())` 推导确定性 `SourceSessionOperationId`（与同节点 `native.release.register` 按 `(actor, requestId)` 隔离一致），经 `SourceRegistrationProof { generated_id: false }` 对同一 actor 的 `(actor, selection, action, intent_digest)` 幂等去重（保留原 `registered_at_ms` 与 `auth_generation`，同 actor 冲突返回 `request_conflict`；不同 actor 各自使用相同 `requestId` 独立产生各自 `sourceOperationId`，不跨 actor 冲突）。
  2. `source.session.execute`：`{ sourceOperationId }`，强制校验原登记 `actor` 归属后等待或返回当前/已结算 `SourceSessionFact`（caller 取消/断开仅丢弃 await，Node 拥有的后台 `Operation` 继续收敛）。
  3. `source.session.status`：`{ sourceOperationId }`，只读查询 `SourceSessionFact`（遵循 `source_session_m05` 共享读取权限：允许同节点当前受信任的其它已配对客户端访问，撤销或外部节点客户端拒绝）。
  4. `source.session.reconcile`：`{ sourceOperationId }`，显式对账，不发送新的 session RPC（同节点当前受信任已配对客户端可触发，撤销或外部节点客户端拒绝）。
  5. `source.session.result.open`：`{ sourceOperationId }`，打开 `native_session_result` 的 `DocumentTransfer`（`targetId: null`, `revision: null`，按 `app/src/adapters/acp/source_sessions.rs:347-362` 直接承载 ACP 原生响应 `"result"` 或 `"error"` 原始 JSON 对象、无外层包装），允许同节点当前受信任已配对客户端访问并逐块校验当前读取 actor，撤销或外部节点客户端拒绝，并执行 offer 消费/收回语义。
  6. `source.session.result.release`：`{ sourceOperationId }`，收回当前激活的结果 offer（同节点当前受信任已配对客户端可访问，撤销或外部节点客户端拒绝）。
  7. `view.open_source`：`{ selection: { ... }, nativeSessionId: string(1..=4096), reason }`，不伪造 `product_targets`、`ReadTargetProof` 或 `Run`，经 `resolve_source_read`（由 `SourceReadSelectionProof`、`ReadScope::SourceOnly`、`source.external_reads` 门控，`ensure_external_route`/`ensure_external_target` 仅在 `InvalidTarget` 时建立外部路由并在已有 target 时复用）打开共享视图。
- **错误映射与能力分离**：
  - `ViewError::UnsupportedCapability`（源自 `AcpError::UnsupportedCapability`）与 `SourceSessionFailure::Native(DispatchError::ProtocolFault(NativeProtocolFailure::UnsupportedCapability))` 均映射为 `"unsupported_capability"`，严格区分于 `ViewError::Unsupported` 及 `#[cfg(not(windows))]` 返回的 `"platform_unimplemented"`。
  - 新建或外部会话不支持 ACP 历史重放时，`view.open_source` 仍以 `CurrentOnly` 实时捕获模式打开（`historyIncomplete: true`），在状态/描述符中报告重放限制，不因缺少重放拒绝打开视图。
- **Web 客户端与 WV15 处置（`clients/web/`）**：
  - `clients/web/tests/error-contract-vectors.ts` 的 `MANAGEMENT_ERROR_CASES` 由 `clients/web/tests/management-error-contract.test.ts` 严格锁定为 C50 的 50 项基线（`expect(codes).toHaveLength(50)`），R3 保持该 50 项基线测试文件不动；在 `protocol.ts` 与 `error-messages.ts` 新增 `"unsupported_capability"` 并通过独立的 R3 测试用例验证其解析与中文报错文案。
  - `session-protocol.ts` 与 `ManagementConnection` 实现全部 7 个动作及 `native_session_result` `DocumentTransfer` 分块读取器；结合 `SourceSessionFact` 与流内无外层包装的 ACP 原生 `"result"`/`"error"` JSON 对象严格解析 `sessions` + `nextCursor` / `sessionId` / `remote_error`，并提供仅基于 `textContent` 的原始 JSON 安全回退（零 `innerHTML`/脚本执行）。
  - Source Discover/Create 面板仅依赖显式校验输入（不依赖伪造的 `product.status`），提供“打开只读视图”与手动 `nativeSessionId` 回退；`SessionViewDisplay` 通过 `sharedViewSpec`（`target` vs `source`）与 `inspectedQueueTargetId`、composer、`nativeReleasePanel` 隔离，刷新及 WSS 重连后从来源重读并在失败时标旧。
  - 保留 `clients/web/tests/browser/session-view-main.ts` 的 WV15 选区/range 严格断言（禁止放宽或删改测试）；`execution-v4` 下 Chromium/Blink 的 `precondition_not_established_on_addRange` 在验收报告中如实记为 `WV15 FAILED`。
- **程序门禁（Rust 与 Web）**：
  - Rust：先以 `cargo test --lib -- --list` 绑定完整限定名，再用 `--exact --test-threads=1` 逐项执行；通过 `cargo fmt --check`、`cargo clippy --tests`、`cargo test --locked --offline`（默认特性与 `--features native-dispatch-dev-fixtures` 均需验证），核查进程树与临时目录清理。
  - Web：通过 `npm run check` 与 `npm test`；**严禁运行 `npm audit`**。

## R3 用例

以下用例 `R3-01` 至 `R3-12` 相互独立；每项必须断言实际线框、状态转移、错误码、权限边界或 DOM 隔离结果，不得仅检查连接成功或返回 HTTP/WS 200。

| ID | 场景和最小判据 |
| --- | --- |
| R3-01 | **封闭 WSS 动作语法与双端编码/解析器校验**：对 `source.session.register`、`source.session.execute`、`source.session.status`、`source.session.reconcile`、`source.session.result.open`、`source.session.result.release` 与 `view.open_source` 在 Rust `session_protocol.rs` 与 Web `session-protocol.ts` 分别执行正向往返与负向注入：缺字段、未知多余字段、非法 base64url `Id128`、非法 `reason`、`launchPath` 空串或 `> 4096` UTF-8 字节、`discover.cursor` 空串或 `> 4096` 字节、`view.open_source.nativeSessionId` 空串或 `> 4096` 字节均返回 `invalid_command`/`invalid_request`；`discover.cursor: null` 合法；`#[cfg(not(windows))]` 对上述命令返回 `"platform_unimplemented"`。 |
| R3-02 | **`source.session.register` 幂等域、冲突拒绝与跨 actor 隔离**：通过域隔离 SHA-256 `(actor.client_id, command.requestId())` 推导确定性 `SourceSessionOperationId`；**同一**已认证 actor 使用相同 `requestId` 与相同 `(actor, selection, action, intent_digest)` 重复发送 `source.session.register` 时，经 `SourceRegistrationProof { generated_id: false }` 返回同一 `sourceOperationId` 并保留原 `registered_at_ms` 与 `auth_generation`，不新建 SQL 登记也不派发 RPC；**同一 actor** 使用相同 `requestId` 但改变 `selection`/`action`/`cursor` 时返回 `"request_conflict"`；**不同 actor** 各自使用相同 `requestId` 时互不冲突，分别产生各自的 `sourceOperationId`；selection 无效或 actor 已撤销/非当前连接时在登记前拒绝，零子进程启动与零 `session/list` / `session/new` 调用。 |
| R3-03 | **`source.session.execute` / `status` / `reconcile` 生命周期、共享所有权与取消隔离**：`execute` 强制校验原登记 `actor`（同节点另一受信任已配对客户端不可越权执行他人的未执行登记），并返回当前或已结算 `SourceSessionFact`；在 `execute` 等待远端响应期间断开 WSS 或取消 caller future，Node 拥有的 `Operation` 继续执行至终态，重连后可通过 `source.session.status` 观测同一 `sourceOperationId` 的真实终态事实；`status`（只读查询）与 `reconcile`（不发送新 session RPC、join 原任务并核对原 receipt 后收束状态）遵循 `source_session_m05` 模型，允许同节点当前受信任的其它已配对客户端访问，撤销或外部节点客户端一律拒绝。 |
| R3-04 | **`source.session.result.open` / `release` 与 `native_session_result` 分块文档流**：`result.open` 与 `result.release` 遵循 `source_session_m05` 共享读取权限：允许同节点当前受信任的其它已配对客户端访问，撤销或外部节点客户端一律拒绝；`result.open` 仅在操作存在可用结果 offer 时返回 `DocumentStreamDescriptor`（`streamKind === "native_session_result"`、`targetId === null`、`revision === null`、`encoding === "utf8_json"`），并执行 offer 消费/收回语义；分块帧下发期间每次准备/提交均校验当前读取 actor 与原始 socket 归属，撤销任一读取 actor 后仅该 actor 后续分块/credit 立即被拒并关闭流，同节点另一未撤销读取方不受阻塞；调用 `result.release` 会收回 active offer 并使后续 `result.open` 返回结果不可用错误。 |
| R3-05 | **错误码分离与 Web C50 基线保持：`unsupported_capability` vs `platform_unimplemented`**：当 ACP 对端不支持 `session/list` 或历史重放（产生 `AcpError::UnsupportedCapability` → `ViewError::UnsupportedCapability` 或 `SourceSessionFailure::Native(DispatchError::ProtocolFault(NativeProtocolFailure::UnsupportedCapability))`）时，WSS 结果明确返回 `{ ok: false, error: { code: "unsupported_capability" } }`；同时验证 `ViewError::Unsupported` 与非 Windows 平台路径仍返回 `"platform_unimplemented"`；保持 `clients/web/tests/error-contract-vectors.ts`（50 项 C50 基线）与 `clients/web/tests/management-error-contract.test.ts` 原样通过，并通过独立 R3 测试验证 Web `protocol.ts` 接受 `"unsupported_capability"` 及 `error-messages.ts` 返回对应中文报错文案。 |
| R3-06 | **无平台 Target 的外部原生 ID 只读视图 (`view.open_source`)**：使用外部创建且尚无平台 `product_targets` 行的原生会话 ID 调用 `view.open_source`，断言经 `resolve_source_read`（`SourceReadSelectionProof` + `ReadScope::SourceOnly` + `source.external_reads`）成功打开只读共享视图，且数据库 `product_targets` 未新增伪造 target 行、未构造 `ReadTargetProof`、未创建平台 `Run`；若该原生会话已存在真实 target，则 `ensure_external_route`/`ensure_external_target` 仅在 `InvalidTarget` 时新建外部路由并复用既有 target；在无 `list` 能力时直接通过 `view.open_source`（手动 `nativeSessionId`）仍可打开且可正常 `view.close`；查看激活产生无 Run 审批时按选项独立裁决（`CL-061`/`CL-065`）。 |
| R3-07 | **无重放能力时的 `CurrentOnly` 降级开启与内容边界**：对不支持 ACP 历史重放的新建或外部原生会话调用 `view.open_source`，断言节点**不拒绝**打开视图，而是返回 `view.opened` 且 `status.mode === "current_only"`、`status.historyIncomplete === true`、`status.cacheRecoverable === false`；实时连接新增输出可见，状态提示明确历史不完整且缓存丢失后无法补回，不从平台队列/命令/审计表回填前文，亦不向 SQLite 写入聊天正文（`CL-062`）。 |
| R3-08 | **Web `ManagementConnection`、无外层包装 `native_session_result` 分块解析与安全展示**：验证 `ManagementConnection` 7 个新方法的请求-响应匹配、错误码抛出（含独立 R3 验证 `"unsupported_capability"`）与超时/关闭合同；按 `app/src/adapters/acp/source_sessions.rs:347-362` 合同，验证 `native_session_result` 流负载直接为 ACP 原生响应 `"result"`（`discover`/`create`）或 `"error"`（`RemoteError`）原始 JSON 对象、无外层包装；Web 端结合 `SourceSessionFact` 与流内 JSON 严格解析 `sessions` + `nextCursor` / `sessionId` / `remote_error`，并在结构不匹配/非法 JSON/超限时明确失败或回退；对含 `<script>`、`onload=`、`<svg>` 或双向控制字符的会话标题/原始 JSON，验证仅通过 `textContent` 安全渲染与原始 JSON 回退展示，零 `innerHTML` 赋值、零脚本执行且零管理动作触发。 |
| R3-09 | **Web Source Discover/Create 面板与 `sharedViewSpec` 只读隔离**：验证 Source Discover/Create 面板仅依据用户显式输入与配置选择渲染，不调用或伪造 `product.status`；通过“发现会话 → 打开只读视图”或无列表时的“手动输入 `nativeSessionId` → 打开只读视图”打开后，断言 `sharedViewSpec`（`source` vs `target`）将视图与 `inspectedQueueTargetId`、输入 `composer` 及 `nativeReleasePanel` 彻底隔离——切换/关闭外部只读视图不影响当前队列或释放面板目标，外部视图下不暴露队列写入/释放误绑定；页面刷新或 WSS 断线重连后对 `source` 规格视图自动按 `reconnect` 从来源重新校验读取，重连/刷新读取失败时保留旧缓存并明确标记为旧视图（`data-stale="true"` 及状态栏失败提示，`CL-063`）。 |
| R3-10 | **WV15 严格断言保留与 `execution-v4` 诚实记录**：静态与运行核对 `clients/web/tests/browser/session-view-main.ts` 的 `runWV15`，确认焦点、`#session-target-id` / `#session-input-body` 的 `selectionStart`/`selectionEnd`、`document.getSelection().addRange` 选区及 `precondition_not_established_on_addRange` 检查保持完整严格（严禁放宽、跳过或改写断言使其虚绿）；在当前报告与汇总中如实记录 `2026-11-13-wv-main-dom-acceptance-v2/execution-v4` 下 Chromium/Blink `WV15 FAILED` 事实，并确保 R3 新增 DOM 变更不破坏通过的 `WV13`/`WV14`。 |
| R3-11 | **L5/L6 验收映射与三类证据边界 (`CL-061`/`062`/`063`/`065`/`085`)**：按证据来源分列并禁止互相代签——(1) 合成浏览器 DOM 夹具验证面板、分块结果消费、只读隔离与标旧；(2) 合同对端/受控 ACP 子进程验证 `CL-061`（外部创建发现与无列表按 ID 查看）、`CL-062`（清缓存/重启及原生压缩/删改/不可用后权威重建）、`CL-063`（外部追加内容后显式刷新/重开/重连重新校验及失败标旧）、`CL-065`（无 Run 查看激活及真实选项审批裁决）；(3) 真实 Harness + Windows Node + 真实认证 WSS + 真实浏览器执行 `CL-085` 最小闭环（外部建/更新会话 → 平台发现或按 ID 查看 → 标准能力下刷新/释放重看），真实程序未产生的可选分支保留为未执行/真实受阻。 |
| R3-12 | **Rust/Web 构建、列表绑定与卫生门禁**：在交付版本上完整执行：Rust `cargo fmt --check`、`cargo clippy --tests`、`cargo test --locked --offline`（覆盖默认特性与 `--features native-dispatch-dev-fixtures`），所有定向 Rust 测试必须先通过 `cargo test --lib -- --list` 绑定完整限定名后以 `--exact --test-threads=1` 执行并断言非零测试数；Web 端执行 `npm run check` 与 `npm test`，确认全程未运行 `npm audit`；记录受测输入文件 SHA-256、独占临时目录清理及无残留测试进程。 |

## 交付判定与限制

R3 仅在 `R3-01` 至 `R3-12` 按对应证据层级真实执行、同版核验通过且问题关闭后，方可用于支撑 `tasks.md` 3.1/4.1/4.2 对应子项的收敛评估；合成浏览器与合同对端结果不得代签 `CL-085` 真实 Harness 闭环，WV15 在 `execution-v4` Chromium/Blink 下的 `FAILED` 结论必须诚实保留直至底层原因解决并重跑通过。任何未执行项、权限/工具阻断或源码哈希漂移均须按未验证/受阻逐项列明。
# R2 实际入口与可观测性缺口

2026-11-13，独立静态读取。当前 HEAD `65ec7815bb0280f4519989d93a7b4ed3b4b37994` 加未提交工作区，不能当固定可执行版本。未编译、未执行 list/产品测试、未复制工作区；本目录输入摘要不覆盖 Root 全量源码。

## 当前 Root 可用与缺失

| 接点 | 当前实际位置 | 对 R2 的直接影响 |
| --- | --- | --- |
| 标准 new/list 构造 | `app/src/adapters/acp/strategy.rs::Strategy::new_session`、`list_sessions`（静态读取时约 316/377 行） | 不需要私有扫描或第二协议实现。list 当前还对 cwd/cursor 做 `len()>4096` 拒绝；本批 opaque cursor 先用合法短值，仅验证原样透传。该旧自设字段门槛不能冒充协议上限，若实际修改须明确依据和受影响回归。 |
| 原协议 source/唯一接收 | `app/src/adapters/acp/native_dispatch.rs::AcpSource`、`NativeLifecyclePort::next`；`transport.rs::Connection/EventReceiver` | Source service 须在现有 receiver 上分发，不能为 new/list 另抢 receiver。当前无 `adapters/acp/source_sessions.rs`。 |
| 原 runtime/配置/关闭 | `app/src/node/session_services.rs::OwnedNodeSessionServices` 和 `NodeManagementSessionServices`；`ProductNative`、configuration Arc、socket_authority、documents | 已有实际 native/Doc 关闭接线，不能用旧候选覆盖。当前两结构无 Source service，`node/source_sessions.rs` 不存在。 |
| Source launch/close | `application/native_dispatch/source.rs`、`source_ports.rs`、`source_jobs.rs::SourceCloseOperation/SourceQuiescence` | 原 jobs/tasks/cleanup 的同一身份和 joined 事实可复用；需把新 producer/SQL/retire 纳入原 close，不建立平行 closer。 |
| 必要存储 | 当前 `session_storage/source_sessions_schema.rs` 两表两索引，schema 11 | 只有 DDL 不证明 application 服务存在。当前无 `session_storage/native_dispatch/source_sessions.rs`、`node_configuration/source_sessions.rs`，R1 独立交付尚待主执行者绑定。 |
| Doc 当前 owner | `node/document_transfer.rs::NodeDocumentService`、`application/document_disposal.rs::DocumentDisposalOwner/DocumentTaskIdentity/DocumentShutdownObservation` | 当前 NodeDocumentService 是 Queue producer，没有旧候选 `source_parts/open_source_result`；新 Source producer须接实际 disposal/runtime/registry，单独验证 provenance、逐块权限及析构。 |
| 原 SQL close 观察 | `session_storage/registry_connection_owner.rs::OriginalSqlCloseReceipt`、cfg(test) `SqlCloseTestSnapshot`（owner/operation/sql_actual/sql_task_join/sql_report/worker/app_join） | 当前已有比旧候选更新的物理 close/worker 证据。必须让 Source receipt保留/透出原对象身份，不能覆盖该 owner 或从新 SELECT 构造确认。 |

## 旧候选可参考 ABI，不能当当前可调用

只读目录：`C:/Users/jack/.codex/visualizations/2026/10/30/01a18dc1-45b2-7f82-981e-964264221cef/session-loop-q003-source-sql-repair-v2/workspace`。

- `node/source_sessions.rs::NodeSourceSessionManagement`：register、execute、query、reconcile、open_result、release_result；execute 以原 socket lifetime/当前 actor 作最后受理 fence，受理后返回 Node-owned receipt。
- `application/native_dispatch/source_sessions/operations.rs::call_recorded`：原 SQL receipt 先 remember，安装原 TaskSlot 后才运行 SQL future；caller receiver 取消不能成为 task owner。`register/query/prepare_execute` 是上层端口。
- `source_sessions/sql_inventory.rs::SourceSqlInventory/SourceSqlEntry`：保存 task、receipt、typed update，`confirm` join 原 task 然后 `confirm_original`；`freeze_metadata` 检查 sealed/原 join/cleanup。
- `source_sessions/reconciliation.rs::reconcile_original`：原 group 序列化，先 confirm 原 entries，再按 proof做存储协调；新 query 不能扮演此入口。
- 旧 cfg(test) `pause_next_sql_for_test(role)`、`original_sql_for_test(id)`、`original_sql_cleanup_for_test(id)`、`source_close_for_test(id)` 可作为观察 ABI 起点，不能原样认为足够；当前 Root 尚无它们。
- 旧 `node/source_sessions/development_tests.rs::source_session_real_discover_create_repeat_doc_and_original_target_load` 把 discover/create/原 target load 合在一例。首批改为独立 V01/V02，不把既有 target load 带成外部 ID 读取证明；V03 参考旧 `source_session_real_written_unknown_is_not_retried_or_replaced` 的真实 exit-create child。
- 旧 `dev-fixtures/source_session_dev_child.rs` 有 list/new 计数、delayed-create gate、exit-create 以及原 result 数字 canary，当前 Root 无该 fixture。其 initialize 固定返回低版仅用于合同夹具，不能代签新连接最高共同版。保持当前默认环境继承和 Windows launcher 归属，不复用旧环境/关闭代码覆盖 Root。

## 编码前必须补足的最小观察合同

以下是测试需要的事实，不限定生产公有 API 名称；cfg(test) 接点必须接实际 owner，不得返回预制成功。

| 事实 | 最小数据/事件 | 缺口及绑定用例 |
| --- | --- | --- |
| 原 operation 在 caller 失去返回前可定位 | invocation/operation opaque identity、role、task-installed 事件，非敏感来源关联 | 旧 register 在函数内生 ID；若取消 caller 未拿到 ID，无从证明原任务仍在。M02 必须由 owner 透出同 invocation identity。 |
| 确定从未调度 SQL | 原调度 owner 产生的 `NeverScheduled` 或等价不可伪造观察、同 invocation SQL-start 计数、拒绝原因 | 旧 `Weak::new()`、inventory 空、receipt observe None 不能区分拒绝/尚未安装/已脱离；需要真正调度前观察。M02。 |
| SQL 已开始及原三故障轴 | role、原 task/receipt/close identity、transaction knowledge、SQL-enter/commit实际结果、close ack、worker/app join 各自事实 | 旧 pause_before 在安装后 SQL 前，不能证明 SQL-enter；需要连接 owner 实际边界。M03。不要求每个 SQL role都加矩阵。 |
| query 无修复，reconcile 按原 proof | 调用前后原 entry identity/knowledge、repair invocation计数、session RPC计数 | 单看最终 DB 行或新 query SQL成功不能证明旧 unknown 没被洗掉。M03。 |
| 单 source/runtime/receiver | 原 runtime、source instance、connection scope、typed RPC和 kind、receiver owner identity及 child PID | 现有低层可复用，Source service 需将实际绑定映入 receipt/小事实观察。V01…V03/M04。 |
| raw/typed CPU与最后析构 | 实际 decode/Vec-work、partial-failure retire、final payload disposal 的原 task identity/start/join；同步 receiver 进入事件 | 旧候选部分 `spawn_blocking_guarded` 不证明 decoder/partial Vec 已离开接收循环；先选当前 Node-owned 槽，再给 M06 事件断言。 |
| close/claim 顺序 | source seal、producer/IO/CPU/SQL/worker join、metadata confirm、claim release 独立事件，全部绑定原 source/claim identity | 旧 close tuple不足以独立证明所有原对象都已 join；M07 不能从 result=Ok 或 count=0推出原任务已收束。 |

不要求公开凭据、正文、路径敏感内容或可伪造的 authorization proof；测试输出只保 opaque identity 和脱敏摘要。实际源码 manifest/PE/执行数量/cleanup证据由后续 runner 独立建立，本文件不代建空成功证据。

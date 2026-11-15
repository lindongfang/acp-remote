# WP4 原生释放持久化 v2 独立审查

审查日期：2026-11-12  
审查范围：仅为 `deliver-codex-session-loop` 的 WP4 原生释放持久化实现及其 v2 证据。  
审查结论：**不通过 WP4 完整性审查。** 现有实现已经覆盖登记、actor/认证代际隔离、阶段迁移和终态写失败的主要安全路径，但 `native.release.register` 没有落实规范要求的请求级幂等；此外，schema 10 的证据只覆盖空库建到 10，未覆盖真实 schema 9 到 10 的升级与回滚，且 schema 版本提升后仍有多处测试硬编码 9。

本报告是 WP4 范围的独立代码与证据审查，不替代 tester，也不是产品验收签字。局部定向测试成功不能认定任务 4.1 已完成，不能认定 `deliver-codex-session-loop` 整体可归档，也不能据此授权提交、推送、发布或归档。

## 绑定源码与证据

| 项目 | 本次核对结果 |
| --- | --- |
| SOURCE 记录 | `testing/runs/deliver-codex-session-loop/2026-11-12-wp4-native-release-regression-v2/SOURCE.json` |
| 绑定 `sourceSetSha256` | `f9693f4061118a7965cd2821244420322a668b0b7afcf689f94dceec99d971d3` |
| `SOURCE.json` SHA-256 | `29d5d1cda89459cd534ce4fe243927beb24f83e04f926cb4ca9085315b4766b7` |
| SOURCE 文件清单 | 512 个文件；当前工作树缺失 0、哈希不一致 0 |
| 汇总哈希复核 | 按 `path + NUL + sha256 + LF` 重新计算，结果与绑定的 `sourceSetSha256` 一致 |
| `MANIFEST.json` | 75 个文件；缺失 0、哈希不一致 0；其记录的 SOURCE 哈希与上述值一致 |
| 当前 Git 身份 | 分支 `codex/session-foundations-replay-planning`，HEAD `65ec7815bb0280f4519989d93a7b4ed3b4b37994`，与 `SOURCE.json:5-8` 一致 |

当前工作树包含大量未提交改动。本次只读检查了相关实现、规格、测试和已有输出，没有 reset、clean、checkout，没有修改实现或测试，没有操作真实数据库、原生会话或凭据，也没有运行 tester 命令。

严重性定义：P1 为会直接违反当前合同、应在接受 WP4 前修复；P2 为会削弱迁移或回归结论、应补齐后重新审查；P3 为低风险说明。本次未发现 P0 级数据泄漏或凭据问题。

## Findings

### F1 [P1] `native.release.register` 未按外层 `requestId` 幂等

当前合同要求节点登记不透明请求 ID，并让同 ID、同内容重试返回原事实，同 ID 异内容发生冲突：`openspec/changes/deliver-codex-session-loop/design.md:15`、`implementation-contract.md:7-9`。会话协议也把 `requestId` 作为命令封套身份：`app/src/management_ws/session_protocol.rs:185-238`。

WP4 的注册路径只把该 ID 用作响应关联，没有持久化或查询它：

- `app/src/management_ws/session_commands.rs:1199-1215` 收到 `native.release.register` 后直接调用注册服务，仅把 `requestId` 放入响应。
- `app/src/node/session_services.rs:271-296` 每次成功注册都调用 `random_id()` 生成新的 operation ID。
- `app/src/session_storage/native_release_schema.rs:3-18` 的表只以 `operation_id` 为主键，没有 `request_id` 列、请求摘要或请求级唯一约束。
- `app/src/management_ws/session_commands.rs:666-690` 的通用入口和 `app/src/management_ws.rs:976-990` 的 pending 集合都没有请求去重；`management_commands` 的生产使用位于 `app/src/trust.rs:325-443`，只覆盖 `device.revoke`，没有被此路由复用。

具体失败序列是：客户端发送 `requestId=R/target=T`；服务端已经提交注册并返回 operation `O1`，但响应丢失或提交结果未知；客户端用完全相同的 `R/T` 重试；当前代码生成并返回新的 operation `O2`。数据库中同时留下 `O1`、`O2`，客户端也无法通过 `R` 找回 `O1`。同一 `R` 改成另一 target 也不会被识别为内容冲突。

现有“重复 release”测试只验证了同一个 `releaseOperationId` 的再次执行，不是注册请求重试：`app/src/node/session_services/queue_read_development_tests.rs:3992-4003`。低层 `begin`/`finish` 的幂等也发生在 operation 已经存在之后：`app/src/session_storage/native_release.rs:95-183`。因此 `EVIDENCE.md:12` 所称的“幂等重试”不能覆盖 WP4 首个持久化入口。

影响：违反当前管理请求合同；丢响应、`commit_unconfirmed` 或断线重连后可产生重复、孤立的释放操作，并把本应返回原事实的重试变成新受理。应在同一 SQLite 事务中按 actor、认证代际、`requestId` 和请求内容登记或读取原 operation，并补充同 ID 同内容、同 ID 异 target、提交成功但响应丢失后重连重试的 WSS 测试。

### F2 [P2] schema 10 的“增量与回滚”证据没有覆盖 schema 9 升级

`EVIDENCE.md:58` 把两名测试概括为“schema 10 增量、重开与故障回滚”，但源测试明确只使用全新内存库：

- `app/src/session_storage/native_release_schema/tests.rs:1-12` 从 `sqlite::memory:` 的空库调用 `schema::initialize`，没有构造 schema 9 数据库。
- `app/src/session_storage/native_release_schema/tests.rs:33-55` 也从未命库开始，仅在 `ReleaseDdl` 故障后断言 `user_version=0`、对象数为 0；这证明的是 0 到 10 全量迁移的回滚，不是 9 到 10 回滚后保持 9。
- 原始输出 `schema10.stdout.txt:975-979` 与这两个测试相符，不能扩展到已有生产结构的数据保留、双版本头保持 9、schema 9 表与数据不变或故障后再次升级。

实现上的升级步骤看起来位于同一事务中，并在释放表 DDL 后同时更新 `node_meta.schema_version` 和 `PRAGMA user_version`：`app/src/session_storage/schema.rs:939-950`，最终提交在 `schema.rs:954-971`。但代码正确性外观不能代替合同级证据。应增加从精确 schema 9 夹具升级、失败回滚仍为 `(9,9)`、原数据与旧定义保持、成功重开为 `(10,10)` 的测试。

### F3 [P2] schema 版本提升到 10 后，多处现有测试仍硬编码 9

`SCHEMA_VERSION` 已改为 10：`app/src/storage.rs:21`。以下测试调用完整 `schema::initialize` 后仍断言 9，按当前源码执行时必然与 10 冲突：

- `app/src/session_storage/native_dispatch_schema/tests.rs:12-17`
- `app/src/session_storage/approval_owner_schema/tests.rs:123-132`
- `app/src/session_storage/approval_owner_schema/tests.rs:145-151`
- `app/tests/registration_acceptance/c/management_independent.rs:229-247`

这些测试会被 lib/acceptance 测试编译；`schema10.stdout.txt:975-979` 只显示了过滤器选中的 2 名测试，并明确过滤掉其余 962 名测试。`EVIDENCE.md:65-79` 列出的“同版回归”也都是选定入口，不能证明当前完整 Rust 测试集健康。至少应先修正这些版本断言，并用完整检查确认 schema 10 没有引入这些确定性回归。

### F4 [P3] “重启后恢复”证据目前只到仓储层

`durable_release_fact_survives_reopen_and_is_bound_to_actor_generation` 确实关闭并重开 SQLite，并读取 `registered` 事实：`app/src/session_storage/native_release/tests.rs:84-108`。但 `EVIDENCE.md:10-12` 的总结没有说明它没有验证真实 Node/WSS 重启、重启后的 `releasing` 收敛、`released/failed` 查询，或客户端重新认证后的完整路由。

这一项不作为已证实缺陷，但当前证据不足以支持服务级“重启后仍能恢复并继续操作”的完整表述。应把它保留为未覆盖范围，后续由 tester 在真实 Node/WSS 生命周期上执行。

## 已核查的正向实现与证据

以下结论来自代码、规格和已有原始输出的核对；本次没有重跑测试。

| 范围 | 结论与证据 |
| --- | --- |
| actor 与认证代际持久绑定 | 表保存 `actor_id`、`actor_auth_generation`、target 和阶段：`native_release_schema.rs:3-18`；读操作同时筛 operation、actor、generation：`native_release.rs:253-269` |
| 当前认证复核 | 当前 actor 会复核节点、签发者、trusted、公钥和 generation：`app/src/access_policy.rs:231-252`；服务在注册、执行和状态读取前取得绑定 generation 的 actor：`app/src/node/request_authority.rs:48-70`、`app/src/node/session_services.rs:271-307`、`session_services.rs:355-373` |
| 跨 actor 隔离 | 兄弟 actor 的 status 和 release 均被 WSS 路由拒绝：`queue_read_development_tests.rs:3835-3867`；低层 reopen 测试还验证另一 actor 和旧 generation 不可读：`native_release/tests.rs:99-118` |
| registered → releasing | `begin` 在 `BEGIN IMMEDIATE` 事务内执行条件更新，已处于后续阶段时返回当前事实：`native_release.rs:95-128`、`native_release.rs:191-227` |
| releasing → released/failed | `finish` 只从 `releasing` 迁移，终态不可互相改写，相同终态可幂等读取：`native_release.rs:130-183` |
| 原生成功与落库顺序 | 先持久化 `releasing`，再执行原生释放；只有原生成功后才尝试写 `released`，终态写失败不会返回成功：`app/src/node/session_services.rs:309-337` |
| 终态落库失败的安全表达 | 路由测试制造终态写故障，断言命令失败、结果为 `null`、状态仍为 `releasing`；移除故障后同一 operation 再执行得到 `released`：`queue_read_development_tests.rs:3914-3975` |
| 已释放 operation 重试 | WSS 路由对同一 operation 的第二次 release 返回相同的 `released` 结果：`queue_read_development_tests.rs:3987-4003` |
| 定向输出真实性 | `repository.stdout.txt:976-980` 为 2/2 通过；`actor-binding-green.stdout.txt:975-986` 为 1/1 通过；`schema10.stdout.txt:975-979` 为 2/2 通过 |
| 共享视图 0-test 记录 | `shared-view.stdout.txt:975-977` 确为 `running 0 tests`、`0 passed`；`EVIDENCE.md:81` 明确说它不作为通过证据，并以 `shared-view-real.stdout.txt:975-983` 的 3/3 为准。该点没有发现把无效 0-test 误记为通过 |

终态 `released` 仅确认原生关闭路径返回成功；若其后释放元数据事务失败，实现保留 `releasing` 并返回 `necessary_storage_unavailable`。这是保守表达，未发现“原生已关但误报成功”的缺陷。资源容量在本次故障窗口内是否仍保持不可复用，现有定向测试没有直接断言，见下方未覆盖面。

## 未覆盖面

1. 未验证同 `requestId` 的 `native.release.register` 重试、不同 target 冲突、成功提交后响应丢失或 `commit_unconfirmed` 后的重连恢复；F1 已由源码直接证明当前行为不满足合同。
2. 未验证真实 Node 停止并重启后从 `registered`、`releasing`、`released`、`failed` 各自继续查询或收敛，也未验证 WSS 客户端重新认证。
3. 未通过真实认证 WSS 将同一 actor 撤销或轮换为新的 auth generation 后尝试旧 operation；旧 generation 只有仓储层测试。
4. 未验证同一 release operation 的并发双请求、并发 `finish`、一方成功一方遇到存储故障时的汇合结果。
5. 未验证精确 schema 9 → 10 升级、失败回滚至 `(9,9)`、旧表和数据保留、`ReleaseDdl` 之外 metadata/pragma/commit 故障。
6. 未验证终态落库失败后，在重新派发或创建资源时容量是否仍保持不可复用；也未验证共享实例其他目标不受影响。
7. 未运行完整 Rust 测试、Clippy、Web 完整套件或依赖/供应链检查；这些不能由目标过滤器输出替代。
8. 未运行真实 Codex/DeepSeek Harness、可信 Web 与真实 Windows Job 组合验收，未操作真实用户数据库或凭据。

## 验收边界

本次可以确认 WP4 的核心数据库事实、actor/generation 隔离、阶段条件迁移、原生成功后的保守终态记录，以及“0-test 共享视图被明确排除”的证据处理。WP4 仍因 F1 失败，不能标记完整；schema 和回归证据还因 F2、F3 不足以支撑“同版完整通过”。

本审查不替代 tester，也不对 CL-030、CL-031、任务 4.1、4.2、5.x、6.4-6.7、6.10 或整体归档作通过判断。任何局部回归成功都应保留为局部结果，整体验收须由对应 tester 和最终独立审查完成。

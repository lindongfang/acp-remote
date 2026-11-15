# R1 schema 11 方案只读复核

2026-11-13 reviewer 增量结论：R1 方向可行，限定为保留 schema 10 并以 additive migration 新增 Source 两表、两索引；当前方案尚不具备实现门。具体 blocker 是独立 `TEST-DELTA.md` 与 R1 受影响测试矩阵尚未由 tester 固定并经 reviewer 核准。R2/R3 未纳入本结论，也不构成阻止 R1 继续细化的理由。

## 我核对的当前事实

- Root 当前 `app/src/storage.rs` 的 `SCHEMA_VERSION` 为 10；当前 `schema.rs` 在同一 `BEGIN IMMEDIATE` 事务中先验证、按版本增量迁移，再更新两个版本头并验证后提交。
- schema 10 是 native release 的真实当前版，`schema.rs` 以 `current < 10` 建表、将 metadata 与 `main.user_version` 更新到10；native release schema/测试不可被 Source 的旧 schema 10 语义覆盖。
- 旧 Source `source_sessions_schema.rs` 固定 SHA-256 `de8a8122773418f8547d1c0cf3896260a8009fa4e05a3f07f96d28aff584f4b8`，定义确为两表、两索引；其 validator 仍以 `<10` 分界，因此移植到新 11 时必须完整调整验证/部分结构拒绝/空身份检查阈值，不可只改版本常量或迁移 DDL。
- 当前 `native_release_schema/tests.rs::exact_nine()` 是从当前 initialize 结果删除 release 对象、再把版本头改成9来制造 fixture。升至11后，该 fixture必须同时移除所有10之后新增的 Source 对象并验证精确旧布局；否则会成为混合版本伪fixture。计划已正确指出不能只把期望10机械替换11。

## 可接受的 R1 边界

计划所列的11版本迁移、四个精确 DDL、main-qualified 建表/索引、metadata 与 pragma 双头、完整布局校验及事务提交符合 additive schema 的边界。保留原 release DDL、版本10数据和 owner/close 链的要求正确。Source 表不写正文、raw/typed 响应或凭据的范围也清楚。

需要在 TEST-DELTA 中具体化并在实现门前核验：

- schema 0 fresh 创建、schema 9→10→11、精确 10→11、schema 11 reopen，以及 metadata/user_version 不一致与未知/部分布局拒绝。
- 新 DDL 每个定义前后故障、metadata/pragma、验证失败、真实 deferred-FK COMMIT 失败和 COMMIT 报告丢失；分别断言旧版精确回滚或已提交11完整布局保留。不能复用 release-only fault 名称造成 Source fault 位置覆盖不清。
- 空身份检查须在 schema 11 包含 Source 表且要求空；schema 10 逻辑不能意外查询尚不存在的 Source 表。
- profile/source/harness/workspace 外键与 Source 表索引/触发器/附加结构拒绝，以及 schema 10 release 行、索引、身份/撤销事实在 10→11 后完全保留。
- 确认 `exact_nine()` 的 Source 对象移除不会掩盖 release schema 9 回滚路径；另有独立精确 schema 10 fixture 验证升级11和保留 release 数据。

建议按功能显式保留单一 Source migration fault enum/ordinal 映射，或提供无歧义 Source fault 入口；测试要逐DDL之前/之后观察数据库快照和双版本头。SQLite DDL 与双头更新继续处于同一立即事务。不要把测试方便性变成启动时自动修复部分 Source schema。

## 写集判断与实现门

计划的 R1 产品写集 `storage.rs`、`session_storage/mod.rs`、`session_storage/schema.rs`、新 `source_sessions_schema.rs` 及开发测试/确受影响 release fixture 与上述范围相符。具体实现前还需固定精确叶清单及与当前 515 叶 Rust 基线的前后身份，避免旧候选对漂移 schema/owner 叶整文件覆盖。开门条件是 tester 独立提交 R1 `TEST-DELTA.md`（至少包含上列判据、真实命令/list限定名、自然退出及数据库 fixture 清理），reviewer核对后再实现；无需等 R2/R3 测试冻结。

方案输入：`REBASE-PLAN.md` SHA-256 `d3545ffdbd7683f91495c288666f398866f0c91943f96f2ce969944fe76818e6`。本复核为只读方案审查，不是编码授权、测试通过、安装许可或完整 WP4 签字。

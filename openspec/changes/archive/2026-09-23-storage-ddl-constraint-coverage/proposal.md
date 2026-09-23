<!-- 本文件定义变更动机、范围、能力与影响；行为细节放入 specs，技术方案放入 design.md，
     协作安排放入 plan.md。本变更 skip_specs，行为契约引用既有合同。 -->

## Why

`crates/storage-sqlite/tests/enum_coverage.rs` 用 core 枚举的 `ALL` + `as_str()` 逐值断言 §7.3/§7.4 的枚举列，但四个 DDL 约束列没有进入这张表：

- `owned_device.revoke_reason`、`owned_node.revoke_reason` 的 `IN (...)` 取值集完全未被断言；行为面也只测过 `RevokeReason::UserRequested` 与 `Compromised`，`KeyChanged` 从未经过 `revoke_token()` 落库。
- `owned_export.cache_policy`、`imported_import.cache_policy` 用的是等值 CHECK（`cache_policy = 'no-content-cache'`），现有解析器只认 `IN (...)`，因此完全未被断言。

风险在于：`revoke_token()` 的穷举 `match` 只能保证**新增**枚举变体触发编译检查，不能证明**已有**变体的 token 拼写正确；`cache_policy` 的单值 CHECK 与 `core::model::CachePolicy` 之间也没有任何自动关联。本次补上 DDL 断言与行为回归，使这两类漂移在测试期暴露。

## What Changes

- 扩展 `crates/storage-sqlite/tests/enum_coverage.rs`：为 `owned_device.revoke_reason` 与 `owned_node.revoke_reason` 增加 DDL 允许值断言。
- 为 `owned_export.cache_policy` 与 `imported_import.cache_policy` 的等值 CHECK 增加断言，并与 `core::model::CachePolicy` 当前取值比对（以范围最小的方式扩展现有取值解析，或提供等效断言）。
- 新增 `RevokeReason::KeyChanged` 撤销行为回归测试：通过存储端口对设备与节点执行撤销，确认事务成功并从库内读回 `key_changed`；该测试真正经过 `revoke_token()` 映射，而不是手写一份与 DDL 相同的字符串列表。
- 新增非法 `revoke_reason` 被数据库真实 CHECK 约束拒绝的测试。
- 不包含：修改 DDL、修改 core 枚举或端口签名、修改合同文档、重构既有 26 条断言与辅助函数（除支持等值约束所需的最小扩展）。

## Intent and Constraints

```agentic-intent
sources:
  - "用户请求（2026-09-23 本会话）：「请修复 storage-sqlite 对四个 DDL 约束列的测试覆盖缺口：owned_device.revoke_reason、owned_node.revoke_reason、owned_export.cache_policy、imported_import.cache_policy」，并要求先读 AGENTS.md 与权威文档、检查代码/测试/git 状态、不假设历史提交仍是 HEAD、不覆盖现有修改。"
  - "用户要求（2026-09-23 本会话）：「扩展 crates/storage-sqlite/tests/enum_coverage.rs，使两个 revoke_reason 列的 DDL 允许值都得到断言。撤销原因的数据库 token 属于 storage-sqlite，不要仅为测试给 core::ports::RevokeReason 增加 ALL、as_str 或其他公开 API。」"
  - "用户要求（2026-09-23 本会话）：「覆盖两个 cache_policy 列的单值 CHECK，验证它们与 core::model::CachePolicy 当前取值一致。现有解析器只处理 IN (...)，请以范围小、易审查的方式支持等值约束或提供等效断言。」"
  - "用户要求（2026-09-23 本会话）：「增加 RevokeReason::KeyChanged 的撤销行为回归测试：通过存储端口执行撤销，确认事务成功，并从数据库读回 key_changed。确保测试真正经过 revoke_token() 映射，而不只是手写一份与 DDL 相同的字符串列表。」"
  - "用户要求（2026-09-23 本会话）：「检查是否需要测试非法 revoke_reason 会被数据库拒绝；若新增该测试，应针对真实 CHECK 约束，避免只重复实现逻辑。」"
  - "用户要求（2026-09-23 本会话）：「特别指出：穷举 match 只能保证新增枚举变体引发编译检查，不能证明已有变体的 token 拼写正确；DDL 断言与行为测试应共同覆盖这一点。」"
  - "用户要求（2026-09-23 本会话）：完成后运行相关定向测试和 npm run verify；无法运行的检查如实写明；不自行提交、推送或创建 PR。"
  - "用户决策（2026-09-23 本会话）：同意本变更（storage-ddl-constraint-coverage）Main E2E 记 not-applicable，替代验证为 4 个目标列的定向 cargo 测试 + npm run verify（npm run check + cargo fmt/clippy/test）。用户回复原话：「同意」。"
constraints:
  - "补丁限于本问题：不改产品、协议或安全语义，不顺手重构无关代码。"
  - "撤销原因的数据库 token 属于 storage-sqlite；不得仅为测试给 core::ports::RevokeReason 增加 ALL、as_str 或其他公开 API。"
  - "DDL 断言与行为测试必须共同覆盖已有变体的 token 拼写，不能只依赖穷举 match 的编译检查。"
  - "行为测试必须真正经过 revoke_token() 映射，不能只手写一份与 DDL 相同的字符串列表。"
  - "非法值测试必须针对真实 CHECK 约束，避免在测试里重复实现判定逻辑。"
  - "先读 AGENTS.md、docs/CORE_PORTS_AND_STORAGE.md §7.3/§7.4 与 §9，检查 git 状态与现有测试，不假设历史提交仍是 HEAD，不覆盖现有未提交修改。"
  - "本地入口 npm run verify（npm run check + cargo fmt/clippy/test）；cargo-deny 与 gitleaks 只在 CI 运行，本地不得声称通过。"
non_goals:
  - "不修改 crates/storage-sqlite/src/migrate.rs 的 DDL，不改 core 枚举/端口签名，不改 docs/CORE_PORTS_AND_STORAGE.md 等权威文档。"
  - "不重构 enum_coverage.rs 的既有 26 条断言与 ManualPattern 设计（除支持等值约束所需的最小扩展）。"
  - "不引入新的测试或生产依赖。"
  - "不改 Sync/Node Link wire schema、fixtures、compatibility 封闭词表。"
success_criteria:
  - "owned_device.revoke_reason 与 owned_node.revoke_reason 的 DDL 允许值集合在 enum_coverage.rs 中被断言。"
  - "owned_export.cache_policy 与 imported_import.cache_policy 的单值 CHECK 被断言，且与 core::model::CachePolicy 当前取值一致。"
  - "通过存储端口对设备与节点执行 RevokeReason::KeyChanged 撤销，事务成功且库内读回 key_changed。"
  - "写入非法 revoke_reason 时数据库以真实 CHECK 约束拒绝。"
  - "定向 cargo 测试与 npm run verify 全绿；本地无法运行的检查如实说明原因。"
decision_bounds:
  - "可自主：等值 CHECK 的断言方式（扩展取值解析或等效断言）、测试与辅助函数的组织位置、非法值测试的构造方式、revoke_reason DDL 断言期望值的来源（测试内字面量或由存储层导出）。"
  - "需用户决策：改变上述不变量、引入生产代码或公开 API 改动、修改 DDL 或合同文档、改变 E2E 模式或验收判据。"
assumptions:
  - "四个列的 DDL 现状（§7.3/§7.4）即期望行为，本次不修改 DDL；若定向测试发现 DDL 与 core 枚举不一致，应停下并报告，而不是顺手改 DDL。"
  - "revoke_token() 的三个 token 当前与 DDL 允许值一致；不一致即为待报告缺陷。"
```

## Capabilities

本变更不改变任何规范层面的可观察行为：production 代码零改动，仅补齐既有行为契约的测试断言。因此在该变更的 `.openspec.yaml` 中设置 `skip_specs: true`，既有行为契约直接引用：

- `docs/CORE_PORTS_AND_STORAGE.md` §7.3（`owned_device`/`owned_node` 的 `revoke_reason` CHECK）与 §7.4（`imported_import.cache_policy` CHECK）、§7.2（`owned_export.cache_policy`）。
- `openspec/specs/storage-schema-v2-migration/spec.md`（DDL 文本与版本契约）与 `openspec/specs/admin-state-persistence/spec.md`（撤销与重启恢复语义）。
- `crates/storage-sqlite/tests/enum_coverage.rs` 的既有判据：§7.3/§7.4 的每个枚举列必须与 core 枚举逐值一致。

### New Capabilities

- 无（`skip_specs: true`）。

### Modified Capabilities

- 无（`skip_specs: true`）。

## Impact

- **Rust（预期唯一改动面）**：`crates/storage-sqlite/tests/enum_coverage.rs`、`crates/storage-sqlite/tests/admin_store.rs`；`crates/storage-sqlite/tests/support/mod.rs` 如需最小辅助（预计不需要）。
- **合同与文档**：无。DDL、端口签名、协议 wire 与安全语义均不变。
- **门禁**：`npm run check` 不受影响；`check:contract-drift` 比对的仍是不变的 §7 DDL 与 §5 端口。
- **生产代码**：预期零改动；若实现需要给 storage-sqlite 增加公开面以获取 token 期望值，须在 design.md 决策中说明并保持范围最小。

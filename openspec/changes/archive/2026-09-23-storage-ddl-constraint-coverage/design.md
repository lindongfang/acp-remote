<!-- 承接 proposal 的动机、范围与影响，说明如何实现以及技术决策的理由。
     本变更 skip_specs，既有行为契约引用 docs/CORE_PORTS_AND_STORAGE.md §7.2/§7.3/§7.4。 -->

## Context

现状事实（2026-09-23 只读核对；HEAD `d9cd3c4`，工作区干净）：

- `crates/storage-sqlite/tests/enum_coverage.rs` 的 `ddl_enum_lists_match_the_core_enums` 维护 26 条 `(table, column, expected)`；`enum_values()` 依靠 `ManualPattern` 只匹配 `\b<ident>\s+IN\s*\(`，取左括号后到第一个 `)` 之间按逗号切分并去引号。
- DDL（`crates/storage-sqlite/src/migrate.rs`）：`owned_device.revoke_reason` 与 `owned_node.revoke_reason` 为 `IN ('user_requested','key_changed','compromised')`；`owned_export.cache_policy` 与 `imported_import.cache_policy` 为 `CHECK (cache_policy = 'no-content-cache')`（等值形式，现有解析器看不见）。
- `core::ports::RevokeReason` 是普通枚举（三个变体，无 `ALL`/`as_str`）；`revoke_token()`（`src/admin/trust.rs`）是唯一落库映射，被 `revoke_device` 与 `revoke_node` 共同使用。
- `core::model::CachePolicy` 是 `token_enum!`，`ALL` 只含 `NoContentCache => "no-content-cache"`。
- `crates/storage-sqlite/tests/admin_store.rs` 已有撤销行为测试，覆盖 `UserRequested` 与 `Compromised`，**未覆盖 `KeyChanged`**；该文件已具备设备/节点夹具与 `raw_pool`/`raw_write_pool`/`scalar_i64` 辅助。
- `docs/CORE_PORTS_AND_STORAGE.md` §7 是 DDL 权威文本，`scripts/check-contract-drift.mjs` 逐条比对 §7 与 `migrate.rs`；因此本次不得修改 DDL。

## Goals / Non-Goals

**Goals:**

- 四个目标列都有 DDL 级取值断言；`revoke_reason` 与 `cache_policy` 的断言都锚在 core 枚举或合同文本上，不产生第三份自由手抄来源。
- `RevokeReason::KeyChanged` 有经过真实端口与 `revoke_token()` 的行为回归。
- 非法 `revoke_reason` 被库内真实 CHECK 拒绝有据可查。
- 生产代码、公开 API、DDL 与合同文档零改动。

**Non-Goals:**

- 不改 `migrate.rs`、`core` 枚举/端口、`docs/CORE_PORTS_AND_STORAGE.md`、wire schema 与 fixture。
- 不为测试给 `core::ports::RevokeReason` 增加 `ALL`/`as_str` 或任何公开 API。
- 不重构既有 26 条断言与 `ManualPattern` 的既有形状（除支持等值约束所需的最小扩展）。
- 不新增测试或生产依赖。

## Decisions

### D1 `revoke_reason` 的 DDL 断言：两条新 case，期望值用合同文本的显式字面量

- 在既有 case 表加入 `("owned_device", "revoke_reason", ...)` 与 `("owned_node", "revoke_reason", ...)`，期望值为测试文件内本地函数返回的三个 token（`user_requested`/`key_changed`/`compromised`，对齐 §7.3）。
- 否决「把 `revoke_token()` 公开给测试以生成期望值」：会为测试扩大 adapter 公开面，且 DDL 断言的期望应锚在合同文本，而不是与 `revoke_token()` 共享同一份映射——否则「DDL 与映射同时拼错」就无法被发现。
- 否决 `RevokeReason::ALL`：用户明令禁止仅为测试给 core 增加公开 API。
- 该断言与 `revoke_token()` **无共享来源**，这是它相对行为测试的独立价值。

### D2 `cache_policy` 等值断言：最小扩展 `ManualPattern` 识别 `= '<literal>'`

- 保持 `enum_values(sql, column) -> Vec<String>` 签名不变；内部查找改为「先按 `<col> IN (` 匹配，整体失败后再按 `<col> = '<literal>'` 匹配」，等值形式返回单元素集合。
- 期望值来自 `tokens(CachePolicy::ALL, CachePolicy::as_str)`（当前单值 `no-content-cache`），因此一次断言同时覆盖「DDL 是单值」与「与 `core::model::CachePolicy` 当前取值一致」。若未来 core 增加第二取值而 DDL 仍单值，该用例立即失败，提示需要合同级变更。
- 兼容性：既有 26 条 IN 用例路径与行为不变；等值分支只在 IN 分支整体失配后启用；两者都失配时仍 panic（保留「列不存在或形状变了」的失败信号）。
- 否决「不写解析、直接断言 SQL 文本包含 `CHECK (cache_policy = 'no-content-cache')`」：脆（绑死空格与格式）且无法与 core 枚举比对。
- 否决「为等值 CHECK 另写一套匹配函数」：两套近邻匹配逻辑更难审查，违背「范围小、易审查」。

### D3 `KeyChanged` 行为回归：落在 `admin_store.rs`，设备与节点各一条

- 复用该文件既有辅助（`device_record`/`approve_device`/`node_record`/`put_node` 等）构造 active 设备行与节点角色行，再调用 `TrustStore::revoke_device` / `TrustStore::revoke_node`（`revoke_token()` 的唯一调用路径），断言返回 `Ok`，随后用 `raw_pool` 从库内读回 `revoke_reason = 'key_changed'`。
- 「真正经过 `revoke_token()`」的可验证性：撤销请求只带 `RevokeReason::KeyChanged`，落库 token 只能由 `revoke_token()` 产生；若映射拼错，UPDATE 撞 CHECK → 事务回滚 → `Ok` 断言失败。
- 放在 `admin_store.rs` 而不是 `enum_coverage.rs`：撤销夹具与辅助都在前者，避免复制设备/节点夹具。
- 与 D1 互补：D1 证明 DDL **允许集合**正确，D3 证明**实际写入值**拼写正确且被该集合接受。

### D4 非法 `revoke_reason` 被真实 CHECK 拒绝

- 新增独立测试：用 `raw_write_pool` 对已存在的 `owned_device` 行执行 `UPDATE ... SET state='revoked', revoked_at=?, revoke_reason='<非法值>'`。语句同时满足 `(state='revoked') = (revoked_at IS NOT NULL)`，使失败的唯一可能来源是取值 CHECK。
- 控制组：同一形状的语句改用合法 token（`compromised`）必须成功，证明失败来自取值而不是 SQL 形状或其他约束；随后再执行非法值语句并断言返回数据库错误且信息含 `CHECK`。
- 仅对 `owned_device` 做：节点列与设备列共享同一 CHECK 形状，其取值集已由 D1 断言，保持补丁小。
- 否决「只靠 D1 文本断言」：DDL 文本被断言 ≠ SQLite 真的强制执行该约束；断言真实执行错误才证明约束在库内生效。
- 否决「先经端口合法撤销再 UPDATE 非法值」：会把测试绑在端口实现细节上，且引入无关前置状态。

## Risks / Trade-offs

- [等值分支误匹配其他 `= '...'` CHECK] → 只在 `<col>` 词边界后紧跟 `=` 与单引号字面量时匹配；等值分支待 IN 分支整体失配后才启用；未匹配仍 panic；既有 26 条 IN 用例即回归网。
- [D1 期望值与 DDL 同时拼错] → D3 用真实写入再读回，二者来源独立，联合可发现。
- [断言绑定 SQLite 错误措辞] → 只断言「错误类型为数据库错误且信息含 `CHECK`（大小写不敏感）」，不做整句比较，减少版本差异导致的脆性。
- [发现 DDL 与 core 枚举实际不一致] → 按 proposal 的 assumption 停下并报告，不在本变更内改 DDL（会触及合同与漂移门禁）。

## Open Questions

无。

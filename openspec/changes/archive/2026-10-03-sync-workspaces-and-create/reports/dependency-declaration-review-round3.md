# Dependency Declaration Review — DPR-plan-1（Round 3，结论：FAIL）

- Round 3 · `phase: plan` / `stage: plan` · 独立只读 reviewer 子 Agent
- Plan Revision：`sha256:8172eeac5a43630f3703d614de5065db55a3434afbe0579091c08f88e6e9fa3d`
- Verdict：**FAIL**（Round 2 的 8 项全部落实；新发现 1 BLOCKER + 1 MAJOR + 2 MINOR + 2 SUGGESTION）

## 发现的问题（均已应用，逐项修复说明见 Round 4 报告的核对表）

1. **BLOCKER** — Round 2 为修 BLOCKER 而新增的 Dependency Handoffs 行声明了一个**不成立的红窗口**（`manifest.json`），且该行自身 Downstream/Upstream 单元格自相矛盾；若 merger 采信，会把本应判绿的集成基线判为「预期红」。
2. **MAJOR** — Merge Strategy 的汇总顺序（`WP1 → WP2 → WP3 → WP4`）与 Shared File Ownership 的 Merge Order（`WP2 → WP1`、`WP4 → WP3`）互相矛盾，同一批共享文件被声明了两种相反顺序。
3. **MINOR** — `docs/CORE_PORTS_AND_STORAGE.md` 的共享区域登记不完整，遗漏 WP4 的 §9/§11.3/文件头三区；文件头版本链是 WP3/WP4 共写区域，原「章节不重叠」断言不成立。
4. **MINOR** — `fixtures/sync/v1/` 与 `fixtures/sync/v1/manifest.json` 两行区域嵌套，不满足「区域不相交」。
5. **SUGGESTION** — WP2 写范围包含无冻结改动落点的 `crates/server/src/node_link/resource.rs`。
6. **SUGGESTION** — `check-contract-drift.mjs` 无 partial mode，「（§5 部分）」「（§7 部分）」措辞与脚本能力不符。

## 附：本轮独立核实为真的关键事实

- 唯一保留的红窗口（WP3 交付后、WP4 落地前的 `crates/storage-sqlite`）**属实**：`session_store.rs:454` 为位置参数调用，`storage-sqlite/tests/` 内 `SessionUpdate {` 全字段字面量恰为 15 处。
- Shared File Ownership 覆盖全部 6 条多写者路径。
- 三条冻结接口变更的构造点、`SnapshotResource`、`CommandName` 均无遗漏属主或外溢。
- 无依赖环；Owner 与 Reviewer 两两不同。
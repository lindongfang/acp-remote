# Dependency Declaration Review — DPR-plan-1（Round 2，结论：FAIL）

- Round 2 · `phase: plan` / `stage: plan` · 独立只读 reviewer 子 Agent
- Plan Revision：`sha256:1635eb7fba7193a6ea427a7782a106cd78b4cacf4b829e53dd1e5adfe8c83598`
- Verdict：**FAIL**（Round 1 的 5 项全部落实；新发现 1 BLOCKER + 1 MAJOR + 5 MINOR + 1 SUGGESTION）

## 发现的问题（均已应用，逐项修复说明见 Round 4 报告的核对表）

1. **BLOCKER** — `fixtures/sync/v1/manifest.json` 是多写者文件，但计划把它单独划给 WP1，与「WP2 的向量必须在自己的分支登记」的门禁事实冲突，使 WP2 分支与集成窗口的 `npm run check` 必红且无处派修。
2. **MAJOR** — WP3 声明的分支级验证在 WP4 交付前结构性不可满足（WP3 增字段必然打断 `storage-sqlite` 的位置参数调用与 15 处 `SessionUpdate` 全字段字面量），计划未登记该红窗口。
3. **MINOR** — WP4 的文档区域漏掉 v5→v6 的四处版本链文字（§9 判据 1/28、§11.3「过新」取值、文件头版本记录）。
4. **MINOR** — `crates/sync-protocol/tests/**` 无属主。
5. **MINOR** — `docs/MODULE_ARCHITECTURE.md` §4.1 值对象清单无属主。
6. **MINOR** — `resource:R1` 无法解析（Runtime Resources 表无 ID）。
7. **MINOR** — `crates/sync-protocol/src/` 是多写者目录但未登记。
8. **SUGGESTION** — `crates/node-link-protocol/src/command.rs` 的命令计数注释会变陈旧且无属主。

## 附：本轮独立核实为真的关键事实

- WP4 的 `code:WP3` 属实（`storage-sqlite` 依赖 core，且以位置参数调用 `SessionSummary::try_new`）。
- 三条冻结接口变更的构造点全部有属主；`crates/app`、`crates/agent-host` 无相关构造点（`agent-host` 命中的是 `acp_protocol::update::SessionUpdate`，另一类型）。
- `SnapshotResource` 6→8、sync `CommandName` 11→12 无 match/enum 外溢。
- 无依赖环；Owner 与 Reviewer 两两不同。
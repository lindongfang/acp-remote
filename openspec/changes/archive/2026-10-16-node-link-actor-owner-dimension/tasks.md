# Tasks

## 1. 幂等键的两个节点维度分开填（WP1）

- [ ] 1.1 把 `crates/server/src/node_link/command.rs` 的 `node_actor` 从自由函数改为方法 `fn node_actor(&self, access_node: &NodeId) -> Actor`，owner 维取 `self.node_id`、access 维取形参（design D1）；在 doc 注释里写清两维各自来源与「两半不得相同」的理由（引用 spec 的幂等键条款）。完成判据：`grep -n "node_actor" crates/server/src/node_link/command.rs` 的每一处都是 `self.node_actor(...)` 形态；漏改的调用点因签名不匹配而**编译失败**（这是 D1 的防漏设计，必须真的观察到）。

- [ ] 1.2 更新全部八个调用点（`command_status`、命令提交、终态观察等，含 `poll_pending`），并把 `poll_pending` 里经 `pending_snapshot()` 取得的 `node` 变量改名为 `access_node`（design D1 的连带清理）。完成判据：`cargo build -p server --offline` 通过；全仓不再有把对端节点称作 `node` 又传给 owner 维的命名。

- [ ] 1.3 重跑并逐条核对 `node_link` 既有用例：凡按旧 `a/a` 文本形态断言 `owned_command.actor_id` 或用它构造 actor 的期望值必须更新；**不得**为了让用例通过而放宽断言。完成判据：`cargo test -p server --offline --lib node_link` 全绿，且在报告里列出每一处期望值改动及其理由。

- [ ] 1.4 新增判别力用例（design D4 的形状）：同一 `requestId` 下两条来自**不同 Access 对端**的命令必须各自落库、互不覆盖、互不构成 `idempotency_conflict`；并断言其中一行 `actor_id` 形如 `"{owner}/{access}"` 且两半不同。完成判据：用例跑绿；把 `node_actor` 临时改回两半同值时该断言**变红**（红—验证必须实跑，报告红绿两侧的输出）。

- [ ] 1.5 覆盖 design  Risks 里登记的已知行为：旧库里的 `a/a` 形态行在新口径的 `command.status` 回查下会 `not_found`。用一条用例把这个行为**钉住**（而不是掩盖它）。完成判据：用例显式断言该 `not_found`，并在注释里写明这是 design D2 记录在接受范围内的代价。

- [ ] 1.6 在模块文档（`crates/server/src/node_link/command.rs` 顶部或 `conn/session.rs` 的相关处）写回幂等键的真实口径：传输三要素不变、持久化表示的两半各自来源、以及**不迁移旧行的决定与理由**（design D2/D3）。完成判据：文档与 design.md 的 D1–D4 逐条一致，且明确写出 `FILE_FORMAT_VERSION` 不 bump 的原因。

## 2. 合并前核验（Project Verify，只在主检出）

- [ ] 2.1 合入 WP1 后在主检出跑 `cargo test -p server --offline` 与 `cargo test -p app --offline --lib`。完成判据：两条命令的真实退出码与摘要；`node_link` 与 `app` lib 用例零失败。

- [ ] 2.2 跑 `npm run check`（尤其 `check:drift`：本变更不改端口签名与 DDL，必须仍全绿）与 `npm run check:rust`（fmt + clippy `-D warnings` + 全测试）。完成判据：十个合同子检查全 OK、`check:rust` 无失败目标；若某条因本机环境跑不了，如实说明是哪条与原因，不得声称通过。

## Workflow follow-up

- WP1 的 patch 合入后按 plan.md 的 Code Review 一节派独立检视（与 WP1 作者是不同 spawn），输入是 patch + `agent://WP1` + `history://WP1`。
- 检视通过并满足 Project Verify 后归档本变更（`openspec archive node-link-actor-owner-dimension`），并复验归档结果。

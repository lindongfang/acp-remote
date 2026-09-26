# WP5 fix2 Handoff — RV2-WP5 的 F1/F2（`node-link-owner` / WP5 catalog 与 resource）

## Shared Report

- **task_id**: `RV2-WP5-F1`（状态表回收不完整，MINOR/P2）/ `RV2-WP5-F2`（`let _ = attachment` 未使用绑定，SUGGESTION）——对应 review 报告 `reports/rv2-wp5.md` 的 Findings。
- **role**: coder；**phase**: fix；**stage**: work-package；**evidence_id**: `RV2-WP5`
- **agent_context**: 任务级 coder 子 Agent（第二轮 fix），**不继承** WP5 实现或 review 会话，只按主 Agent 下发的 fix 清单与 `rv2-wp5.md` 的 Findings 原文作业。工作目录 `D:\Project\acp-remote-wt\node-link-owner`（worktree，分支 `agentic/node-link-owner`）；权威规划根 `D:\Project\acp-remote`（报告与日志写在该处 `openspec/changes/node-link-owner/reports/`，该目录不入版本控制）。工具集含 shell 与 Git：本轮 diff、门禁与红向证据均由本 Agent 亲自执行。
- **base / target_revision**: base = `27ad3f800c7821846b8991ae7d61b67f59e8ba24`（RV2-WP5 的 Target Revision；开工时 HEAD 与之一致、`git status --porcelain` 为空）；**target = `5e2d17c7ac4074b1d473dffab664eed48f48f21e`**（本轮唯一提交后的 HEAD，`git status --porcelain` 为空、无 staged 文件）。
- **scope**（严格按派单允许写入面 `crates/server/src/node_link/**`）：
  - `crates/server/src/node_link/resource.rs`（F1 的生产改动 + F2 的形态改动 + `subscribers` 方法文档）；
  - `crates/server/src/node_link/resource/tests.rs`（F1 的两条新用例）。
  - **未改**：`catalog.rs`、`node_link/mod.rs`、`conn/**`、`docs/**`、`schemas/**`、`compatibility/**`、`fixtures/**`、协议 crate、`openspec/**`（proposal/specs/design/plan/tasks/verification）、`crates/app/**`；无清单外编辑（本轮没有编译连带）。
- **changes**（1 个提交，2 个文件，+92/−14）：
  1. **F1（P2）回收判据提前**：`ResourceRoute::subscribers` 现在**先**判「该 key 的句柄是否还在 `ConnectionRegistry`」，不在则登记 `ended`（遍历结束后统一 `states.remove`），**再**做订阅判定。此前活跃判定排在「本会话订阅」与「attachment 代际」两道 `continue` 之后，因此「attach 后从未 subscribe」与「re-attach 清掉订阅」（§12.4：重新 attach 会清掉该会话旧订阅，Access 必须重新 subscribe）这两类已结束连接的条目**没有任何回收路径**，状态表会按历史连接数单调增长、每次扇出的扫描成本线性上升。方法文档同步改写（明确「回收判据在订阅判定之前」及这两类连接的成因）。
  2. **F2（SUGGESTION）去掉未使用绑定**：`on_ack` 的 `let Some(attachment) = self.attachment_for(…) else { … }; let _ = attachment;` 改为 `if self.attachment_for(…).is_none() { … }` 提前返回；错误码、判定位置与语义逐条不变（纯形态调整，见「红向/等价证据」第 ③ 条）。新增一行注释说明这是**存在性判定**（「没有 attachment」与「epoch 不符」同码，一律不记账）。
- **checks**（原始输出见 `reports/wp5-catalog-resource.log` 的「wp5-fix2 轮次」分节）：
  - `cargo fmt --all -- --check` exit=0（恢复后复跑同样退 0）。
  - `cargo clippy --locked -p server --all-targets --all-features -- -D warnings` exit=0。
  - `cargo test --locked -p server --all-features` exit=0（server lib **250** 通过 / 0 failed，其余 6 个 test binary 全 ok）。base 的 server lib 是 **248**，本轮 **+2**（`node_link::resource::tests` 17 → 19），**既有用例一条未删、断言未改**。
  - `npm run check` exit=0（`check:schemas` 118 valid/25 invalid、`check:docs` 378 links/4114 section refs、`check:boundaries` 12 crate、`check:drift` §7 36 条 DDL + §5 15 trait/93 方法、`check:agentic` 16 items 全 PASS）。
  - 提交前钩子（`.husky/pre-commit`）三步全通过（fmt → `npm run check` → workspace clippy），commitlint 通过（钩子提示本地未装 gitleaks，CI 的 `secrets` job 仍判定）。
  - **红向/等价证据**（每一段都在日志里留下原始输出，且每条都在改坏后从 `/tmp` 备份逐字节恢复并复跑）：
    1. **删掉回收动作**（`for key in ended { states.remove(&key); }` → `let _ = ended;`）：`node_link::resource::tests` 变红 **3 条**——两条新用例（`未订阅的已结束连接也必须被回收` @ `tests.rs:1204`、`订阅已被 re-attach 清掉的已结束连接也必须被回收` @ `tests.rs:1245`）+ 既有的 F3 用例（`已结束连接的条目必须被回收` @ `tests.rs:1173`），其余 16 条仍绿。
    2. **只把活跃判定放回订阅判定之后**（回收动作保留 = 修复前的顺序）：恰好**两条新用例**变红（同上两行），既有 F3 用例仍绿、其余全绿——证明两条新用例判定的正是「活跃判定提前」这一个改动，而不是泛泛的「回收存在与否」。
    3. **F2 改回原形态**（`let Some(attachment) = … else { … }; let _ = attachment;`）：`node_link::resource::tests` **19 条结果完全不变**（含既有的「没有 attachment 的 ACK → `sequence_invalid`」断言），即 F2 行为等价。
  - **未执行（如实记录）**：`cargo-deny` 与 `gitleaks` 只在 CI 运行、本地无等价物；本轮未跑 workspace 全量 `cargo test`（改动面只到 `server` 的两个文件，上一轮的 workspace 全量结果见同目录日志的上一节）；未跑 `[PV5]`/E2E 与候选门禁（均不由本角色执行）。
- **issues**: 两项 Findings 全部落地；无阻断。**一条需要主 Agent / reviewer 知晓的、未擅自扩大的残余**：
  - **F1 的判据窗口**：`handles()` 快照仍取在 `lock(&states)` **之前**（与修复前同形，也是派单指定的注入点）。理论上「快照之后才注册、且在拿到 `states` 锁之前就完成 attach」的连接，其条目可能被这一轮误回收。窗口只有两次相邻调用之间的几条指令、且对端必须在这个窗口内跑完一个完整 RTT，实际不可达；即便命中，后果是它的下一次 `resource.subscribe` 得到 `nodelink.resource.attach_generation_stale`（可重连恢复），**不是静默丢事件**。要彻底关掉需要把 `handles()` 挪进 `states` 临界区（新的锁嵌套顺序），属超出本次派单的并发设计选择，本 Agent 未擅自改动，登记给主 Agent 裁决是否另开一条。
- **result**: **PASS**（本轮 fix 范围内全部适用条件满足、门禁全绿；**不代表** RV3 独立复核、`[PV5]`、E2E 或合并已完成）
- **evidence_paths**:
  - 本报告：`openspec/changes/node-link-owner/reports/wp5-fix2-handoff.md`
  - 被处理的 review：`openspec/changes/node-link-owner/reports/rv2-wp5.md`
  - 本轮原始输出（含两段红向证据与 F2 等价核对）：`openspec/changes/node-link-owner/reports/wp5-catalog-resource.log` 的「wp5-fix2 轮次」
- **resource_cleanup**: 未新增端口、数据库、容器、证书或外部账号；未安装依赖、未改 `Cargo.toml`/`Cargo.lock`/任何 schema 与 compatibility 资产。临时备份只写在系统临时目录（`/tmp/resource.final.rs`、`/tmp/wp5-fix2.log`，非仓库内）；提交后 `git status --porcelain` 为空，无未跟踪文件、无 staged 文件。

## 提交（供 reviewer 逐条核对）

| SHA | 标题 | 文件 |
|---|---|---|
| `5e2d17c7ac4074b1d473dffab664eed48f48f21e` | `fix(server): 回收 Node Link 资源状态表里未订阅连接的条目` | `crates/server/src/node_link/resource.rs`、`crates/server/src/node_link/resource/tests.rs`（2 files, +92/−14） |

（经 `.husky/pre-commit` 与 commitlint；提交后 `git status --porcelain` 为空。分支 `agentic/node-link-owner` 未 push。）

## 各条落点（reviewer 复检索引）

| ID | 修复落点 | 用例 / 证据 |
|---|---|---|
| RV2-WP5-F1 | `resource.rs::ResourceRoute::subscribers`（活跃判定移到 `for (key, state)` 循环体首部，`ended`/`states.remove` 回收逻辑不变）+ 方法文档 | `resource/tests.rs::a_finished_connection_that_never_subscribed_is_reclaimed`（attach 后不 subscribe → 注销句柄 → 扇出 → `states` 空、且不再收到事件）、`::a_finished_connection_whose_reattach_cleared_its_subscription_is_reclaimed`（attach → subscribe → 再 attach（断言 `subscriptions` 已空）→ 注销句柄 → 扇出 → `states` 空）；红向证据 ①② |
| RV2-WP5-F2 | `resource.rs::on_ack` 的归属步骤（`attachment_for(…).is_none()` 提前返回，替换 `let Some(attachment) = … ; let _ = attachment;`） | 无新用例（纯形态）：既有 `resource/tests.rs::ack_progress_is_monotonic_and_bound_to_the_attachment` 已覆盖「没有 attachment 的 ACK → `nodelink.protocol.sequence_invalid`」；等价证据 ③（改回原形态后 19 条结果不变） |

## 决策与边界（供 reviewer 判 applicability）

1. **只改判定顺序，不改回收机制**：F1 的回收仍然是「扇出时惰性回收 + 遍历中登记、遍历后 `remove`」，`forget_if_closed` 作为第二条清理路径原样保留。派单指定「凡 key 不在 `registry.handles()` 的一律登记回收（放在订阅判定之前）」，实现与该字面逐条一致；`let handles = self.registry.handles();` 的位置未动。
2. **新增用例只断言外部可观测结果 + 一条前提断言**：两条用例都断言「扇出后状态表为空」和「已结束连接不再收到事件」；其中「re-attach 清订阅」用例额外用一条内部断言（`state.subscriptions` 全空）**确立用例前提**（证明场景真的落到了「无本会话订阅」这一分支），不引入新的公开形状。
3. **不判定 `target_revision` 之外的行为**：本轮没有触碰 `catalog.rs`、`conn/**`、协议层与文档，因此 `rv2-wp5.md` 登记的其它残余风险（`ResourceRoute::new` 第三参数在 WP7 的接线、F5 日志无自动化回归、`resource.ack` 无限流、`catalog.subscribe` 的 `knownRevision` 非空分支缺路由级用例）均未变动，仍按原登记交 WP6/WP7 处理。

## handoff_index

```yaml
handoff_index:
  - task_id: "RV2-WP5-F1"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "5e2d17c7ac4074b1d473dffab664eed48f48f21e"
    evidence_type: CHECK
    evidence_id: RV2-WP5
    report_path: "reports/wp5-fix2-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "subscribers() 的活跃判定（key 是否在 registry.handles()）移到订阅判定之前，遍历中登记 ended、遍历后 states.remove；新增两条路由级用例覆盖「attach 后未 subscribe 即断开」与「re-attach 清订阅后断开」，断言扇出后 states 为空且不再收到事件；红向证据两段：删掉回收动作 → 3 条回收用例变红（含既有 F3），仅把活跃判定放回订阅判定之后 → 恰好这 2 条新用例变红而既有 F3 仍绿；cargo test -p server --all-features 250 passed（base 248，+2），既有用例零删除"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV2-WP5-F2"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "5e2d17c7ac4074b1d473dffab664eed48f48f21e"
    evidence_type: CHECK
    evidence_id: RV2-WP5
    report_path: "reports/wp5-fix2-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "on_ack 的 attachment 存在性判定改为 attachment_for(…).is_none() 提前返回（去掉未使用的 let _ 绑定），错误码 sequence_invalid、判定位置（owner 判定之后、epoch 判定之前）与「不记账」语义逐条不变；行为等价由既有用例 ack_progress_is_monotonic_and_bound_to_the_attachment（含「没有 attachment 的 ACK 必须被拒绝」）与「改回原形态后 19 条 resource 用例结果不变」的对照证据共同支持"
    source_evidence: NOT_APPLICABLE
```

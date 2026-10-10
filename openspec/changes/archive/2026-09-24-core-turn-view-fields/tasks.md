## 1. Dependency and Resource Setup

- [x] 1.1 在新的任务级最小上下文中核实仓库绝对路径、`git rev-parse refs/heads/main` 与 `HEAD` 是否一致、工作区是否无被跟踪文件改动、工具链版本（`rust-toolchain.toml`、Node）、`变更校验` 的开关值与 `command`、以及本变更声明的四道 PV 命令是否可用。
- [x] 1.2 确认实现所需的契约与文件所有权：`crates/core/src/model/json.rs`、`crates/core/src/broker.rs`、`crates/core/src/model/event.rs`（注释）、`crates/core/src/ports.rs`（仅注释）归 WP2 单写者；`docs/CORE_PORTS_AND_STORAGE.md` 与 `docs/MODULE_ARCHITECTURE.md` 归 WP1；`crates/storage-sqlite/tests/**` 归 WP3；`crates/agent-host/tests/**` 归 WP4；确认 WP1∥WP2、WP3∥WP4 的并行批次与「同一 `target/` 串行执行 cargo」约定。
- [x] 1.3 在 WP3 开始前接入已验收上游（WP2 的注入工具与提交前归属定稿接口），核对下游实际基线与包含关系（`cargo test -p storage-sqlite` 能编译出引用 `core::Broker`/`SessionStore` 的集成用例）。

## 2. Implementation

- [x] 2.1 把注入时机、会话版本规则与 `TurnAccepted.turn` 的语义写进权威文档：`docs/CORE_PORTS_AND_STORAGE.md` §5.1（`prompt` 返回值语义）、§6（提交前注入 turn/version 与版本规则）、§9（注入与漂移断言作为判据）；`docs/MODULE_ARCHITECTURE.md` §4.1/§4.7（适配器产 ACP 派生字段、broker 补身份与版本的分工）；完成条件：`npm run check:docs` 与 `npm run check:drift` exit 0，措辞与实现/规范一致且不夸大。
- [x] 2.2 在 `core::model::json` 实现**只在上位插入一个顶层成员**的纯文本能力（复用既有 `json_members`/`json_string` 文本工具，不改写既有成员、不重排键、不丢未知字段），并提供「目标键是否已存在 / 取值是否一致」的判定；不得引入任何新依赖（core 依赖闭包必须保持冻结 allow-list）；完成条件：单元测试覆盖空对象、含既有成员、含嵌套对象、目标键已存在且一致、目标键已存在且冲突、深度上限边界。
- [x] 2.3 确认 turn 归属是**单源**的：`commit_chunk` 在批组装时按 `event.turn.or(running)` 解析一次，提交漏斗 `Broker::commit_owned` 只按该值注入 `turnId`、不重新推导；完成条件：core 用例断言「落盘 `owned_event.turn_id`（fake 侧记录）、view 的 `turnId`、turn 行 id 三者一致」且同批其它要求类型也带上该值；另有用例固定「无归属 → 两者同时缺失」的降级。
- [x] 2.4 imported 交付路径**不注入、不重写、不补齐**：`turnId`/`version` 由拥有该会话的节点注入后随事件到达，`payloadDigest` 覆盖 Owner 给出的视图字节，本端必须逐字节转发（含缺失时不伪造）；完成条件：core 用例覆盖「Owner 已带 `turnId`/`version` → 发布字节逐字相同」「缺失 → 不伪造」，且无正文索引里的摘要等于调用方给出的值（Check Plan Change 1）。
- [x] 2.5 实现版本注入与漂移断言：只对含 `session.mode.changed`/`session.config.changed` 的提交推导预期版本（`expected_version` 为 `Some` 时用它，否则回读会话当前版本；含 `StateChange` 则 +1，否则不变），注入十进制字符串；提交后比对 `CommitOutcome.version`，不一致即 fail-closed（`PortError::Corrupt`、不发布、不报告成功）；幂等命中（`replayed`）不比对；imported 路径不注入；完成条件：core 用例覆盖「状态变更后版本正确」「纯事件提交不递增」「fake 存储返回不同版本 → 显式失败且不发布」「回放分支不写行、不改字节」（Check Plan Change 2）。
- [x] 2.6 把 `TurnAccepted.turn` 的语义写实：`core::ports` 注释与文档（`docs/CORE_PORTS_AND_STORAGE.md` §5.1）说明该字段是适配器侧占位/审计值、core 一律不采信；保持端口签名不变；完成条件：core 用例覆盖「适配器返回不同占位值（含全零）→ turn 行仍只有一行、id 与 view 的 `turnId` 都等于 core 权威值」（Check Plan Change 3）。
- [x] 2.7 补齐全部行为回归用例并与规范逐条对应：无归属不伪造、`turnId` 冲突一致保留/不一致无副作用失败、ACP 三要素逐字节不变、未知字段与嵌套结构保留、幂等重放返回首次持久化字节且不二次注入；完成条件：`cargo test -p core --all-features` 全部通过，且每条断言都有对应的规范 Requirement/Scenario 引用。
- [x] 2.8 执行交付前 Project Verify 并留证：`cargo fmt --all -- --check`、`cargo clippy --locked -p core --all-targets --all-features -- -D warnings`、`cargo test --locked -p core --all-features`、`node scripts/check-crate-boundaries.mjs`（确认 core 依赖闭包未变）
- [x] 2.9 在 `crates/storage-sqlite/tests/**` 新增真实 SQLite 用例：创建会话 → 纯事件提交（带已注入 `version` 的 `session.mode.changed`）版本不变 → 含状态变更的提交版本恰好 +1 → 过期 `expected_version` 被拒；并修掉实测发现的缺陷（`Update` 分支未回填 `origin_epoch`，使状态+事件同批被拒；按 §5.2「存储层只校验已有 epoch 时必须一致」修 1 行）；不得修改 DDL/`migrate.rs`
- [x] 2.10 在 `crates/agent-host/tests/view_contract.rs` 新增跨 crate 契约用例：用真实适配器（fake ACP 子进程的 `chunked-updates`/`permission-request`/`elicitation` 场景）收集 `EndpointEvent`，逐类型断言「不含 `turnId`/`version`、其余 §10.3 最低字段齐备、`EndpointEvent.turn` 为 `None`」，并断言 core 独占类型（`turn.queued`/`turn.started`/`turn.delta_compacted`/`agent.message.completed`）不被适配器产出；同步 `agent-host/src/lib.rs` 的边界注释（原「已知缺口」已关闭）；不得修改 `agent-host` 的行为实现；守卫清单必须覆盖 `TURN_SCOPED ∪ VERSION_SCOPED` 的全部条目（本次真实产出的类型走断言，未被本用例产出的类型必须列入显式豁免清单并写明原因，使得「某类型悄悄消失」必然变红），并补 `crash-on-prompt` 场景覆盖 `turn.failed`

## 3. Branch Validation

- [x] 3.1 执行 `npm run check`（十道合同门禁）并留证，确认 `check:boundaries` 仍报 8 个 crate、core 依赖闭包等于冻结 allow-list、`check:drift` 仍与 `migrate.rs`/`ports.rs` 一致
- [x] 3.2 复核交付前检查日志的完整性（命令、退出码、用例计数、逐需求覆盖），确认没有把断言弱化或删除来换绿灯。
- [x] 3.3 复核集成用例确实经过真实存储层（不是再包一层 fake），并复核用例可证伪性（存在能让它变红的实现改动思路）。
- [x] 3.4 复核跨 crate 用例确实使用适配器的真实 `Endpoint` 与 core 的真实 broker（不是重新声明一份映射），并复核断言可证伪性。
- [x] 3.5 只读检视文档改动：与 `specs`/`design`/代码事实是否一致、是否把计划写成现状、与既有权威来源（§10.3、§5.2、§6）是否冲突
- [x] 3.6 只读检视注入实现与失败关闭路径（是否真的在提交前、是否可能双源、冲突/漂移是否真的无副作用、重放是否二次注入、是否可能静默改写既有字段），并**亲手做至少 3 条变异自检**（改坏实现 → 对应用例必须变红 → 还原 → 再跑变绿），不接受实现者自述
- [x] 3.7 只读检视集成用例的接缝真实性与断言强度
- [x] 3.8 只读检视跨 crate 用例的接缝真实性与断言强度
- [x] 3.9 对上述修复批次做复核轮：逐条确认已闭合、未被修复引入新问题，并核对同类实例（同批同类）是否一次扫清（用全仓 grep 复核，不接受只改点名实例）

## 5. Integration Readiness

- [x] 5.1 （仅一次，不随单元复制）单独创建独立集成 Agent 并显式交接 `roles/integrator.md` 全文、计划与契约、源提交及证据、独立集成 worktree、目标分支与授权边界，记录实际 ID 与上下文方式；缺少独立执行能力时该任务 BLOCKED。
- [x] 5.2 复核该单元预定模式（integrated）与 WP 组成，核对 3.x 的检查与独立 review 证据对当前候选版本仍有效；变化先同步计划与依赖。

## 6. Merge Unit

- [x] 6.1 核实目标仓库与 `refs/heads/main` 当前提交并记录准确引用与核实命令；无法确认时保持 BLOCKED。
- [x] 6.2 基于已核实基线构造本单元候选，固定基线与候选版本，记录组成与构建结果（含 `cargo build --locked --workspace --all-features`）；完成条件：候选提交与构建输出记录在案。
- [x] 6.3 在候选版本执行 `npm run verify` 
- [x] 6.4 只读检视候选新增交互与冲突解决（`core` 的注入语义与 `agent-host`/`storage-sqlite` 的测试改动是否只加断言不改行为、`docs` 与代码是否一致、是否引入被禁依赖），修复后独立复核 
- [x] 6.5 按 not-applicable 路径核对理由、依据、替代检查安排与 `downgrade_approval` 记录，确认没有必须运行 E2E 的任务
- [x] 6.6 确认候选证据完整后以条件更新或串行机制防竞态，在授权范围内合入本地 `refs/heads/main` 并记录实际提交；基线变化时重开受影响候选任务。
- [x] 6.7 核对实际主分支结果与候选一致性并在 `main` 上重跑 `npm run verify` 
- [x] 6.8 独立检视合并新增差异；无新增差异时由主 Agent 记录依据与原 review ID 

## 7. Final E2E

- [x] 7.1 在最终主分支固定版本执行替代验证四项：`cargo test --locked -p core --all-features`、`cargo test --locked -p storage-sqlite --all-features`、`cargo test --locked -p core -p agent-host --all-features`、`npm run verify`，并额外跑 `npm run check` 复核合同门禁
- [x] 7.2 汇总替代验证的全部断言、版本与证据，核对覆盖了 not-applicable 的 `alternative_checks` 四项与资源清理（临时 SQLite 文件、子进程、`CARGO_TARGET_DIR`），并核对 `fixtures/**`、`compatibility/acp/v1/matrix.json`、`schemas/**` 的哈希在执行前后不变
- [x] 7.3 运行 `变更校验`，仅 PASS 自动勾选；此行只确认不适用判据已按计划固化，不执行测试或汇总。

## 8. Final Verification

- [x] 8.1 按 `变更校验`（无 skill 发现能力时读 `变更校验`）执行最终验收，核对用户意图、需求、设计、计划、任务与最终主分支证据；完成条件：验收结论为 PASS 且该检查 PASS；其余任务未完成或存在未闭环 FAIL/BLOCKED 时不得完成。

<!-- 执行安排：把 specs 的行为要求与 design 的技术方案转成可派发的工作包。
     plan 只写「谁在哪个隔离工作区做什么、按什么顺序合入、怎么验」，不重复 specs/design 的内容。 -->

# Plan

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状；每个工作包**一次 spawn**（本轮两个包有依赖，按 Merge Order 串行派发）。
- 隔离：`task.isolation`（patch 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>`（报告）+ `history://<id>`（过程）。
- 合入：主 Agent 串行 `git apply`；本变更涉及 `crates/server/Cargo.toml` 的新依赖，因此**合入后由主 Agent 在主检出更新 `Cargo.lock`**（隔离工作区没有可用的 cargo/网络，锁文件不由子 Agent 产出）。
- 提交信息手写并按 `commitlint.config.mjs` 的 type/scope 词表：`fix(server)` / `fix(node-link)` / `fix(sync)` 视落点而定（scope 是模块边界）。

## Work Packages

### WP1 —— 传输层拆分与 node_link 写任务

- 写范围：`crates/server/Cargo.toml`（新增 `futures-util`）、`crates/server/src/transport/net/ws.rs`（读写半边拆分；`recv` 的 1009 关闭改为返回判定）、`crates/server/src/node_link/conn/session.rs`（写任务、`write_now` 改为投递、模块文档改写）、`crates/server/src/node_link/conn/tests.rs`（回归：对端不读 ⇒ 有界收尾；写卡住期间判定仍被检查）。
- 依赖：无（首个包）。
- 验证：本地可跑的最小集在隔离区**不可执行**（无 `target/`）；由主 Agent 按 PV1/PV5 验证。

### WP2 —— sync 侧同构

- 写范围：`crates/server/src/sync/conn/session.rs`、`crates/server/src/sync/conn/tests.rs`。
- 依赖：**WP1 必须先合入**（`sync` 复用 WP1 在 `transport/net/ws.rs` 定下的读写半边与写任务形状）。
- 验证：同 WP1（PV1 的复现用例的 sync 版本 + PV2/PV5）。

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 0 | （主 Agent）复现探针 | tasks 1.1/1.2 的证据要在改动前取到：证明「写卡住 ⇒ `select!` 停摆」 | 无（只读探针，用完删除） |
| 1 | WP1 | 定下传输层形状；`node_link` 是主路径 | PV1（`node_link` 版）、PV2、PV3 |
| 2 | WP2 | 与 WP1 写范围不重叠，但依赖 WP1 的形状 | PV1（`sync` 版）、PV2、PV3 |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 主检出：一次性探针（真实 loopback listener + 不读任何字节的客户端，写形态取 32 KiB 小帧） | 修复前：3 秒内写未完成，且窗口内**没有**新的心跳/看门狗记录；修复后：连接在有界时间内被收尾（≤ `SLOW_CONSUMER_GRACE` + 一个 tick）且窗口内**仍有**判定记录。探针实验后删除，不入库 |
| PV2 | `cargo test --locked -p server` 与 `cargo test --locked -p app`（主检出） | 全绿；两侧新增回归用例包含在内 |
| PV3 | `cargo clippy --locked --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`（主检出） | 无告警、无格式差异 |
| PV4 | `npm run check`（主检出） | 十道合同脚本全绿（本变更不改合同资产） |
| PV5 | 反例探针（主检出，临时改动后恢复） | ① 把写期限调到极大（或删掉期限）⇒ PV1 的复现用例必须红；② 把写改回内联 ⇒「写卡住期间判定仍被检查」断言必须红。两次实验后都还原并复跑 PV2 |
| PV6 | CI 连跑两次「合同门禁 + Rust 检查」 | 两次全绿（本地快机不能替代 CI 慢跑器的证据） |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 写任务的所有者/取消/关闭顺序（`AGENTS.md` §8）；帧序与「至多一帧在写」；1009 关闭的判定与动作是否分离得干净；写期限的处置是否真的不重试半截帧；既有断言有无被放宽 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 与 WP1 是否**逐条同构**（差异必须被说明）；`sync` 侧饱和期心跳的口径差异未被顺手改动；回归用例在两侧等价 | 同上 |

## Main E2E

```yaml
mode: not-applicable
reason: 本变更没有用户可触达的界面或 CLI 入口；被观察的对象是 WSS 连接在「对端不读」时的收尾行为，入口就是真实 listener 上的探针（PV1）与两侧的集成用例。
basis: docs/WORKFLOW.md 要求 E2E 断言可观察行为；本变更的可观察行为（有界收尾 + 判定仍被检查）已在真实 loopback listener 上断言，没有第二层可叠。
alternative_checks:
  - PV1：真实 listener + 不读字节的客户端，修复前后各一次。
  - PV5：两个回退反例，证明新增判据确有判别力。
  - PV6：CI 慢跑器上连跑两次。
```

## Risks

- **[隔离区跑不了 cargo]** 两个工作包都无法在隔离区验证 ⇒ 交付是「代码 + 自证说明」，真实运行由主 Agent 按 PV1/PV2/PV5 执行；报告不得声称已在隔离区通过。
- **[新依赖与锁文件]** WP1 新增 `futures-util` ⇒ `Cargo.lock` 必须在同一批提交里更新（主 Agent 在主检出执行 `cargo metadata`/`cargo check` 后提交），否则 CI 的 `--locked` 会失败。
- **[探针要临时改实现]** PV5 需要临时改 `session.rs` 的期限与写路径 ⇒ 实验后 `git checkout` 还原并复跑 PV2；实验本身不入库。
- **[回归用例的时限]** 「对端不读 ⇒ 有界收尾」若按生产窗口（30 s）跑，用例时长不可接受 ⇒ 用例必须走 `#[cfg(test)]` 的窗口注入（`with_test_windows` 一类既有形状），不得为此把生产常量改小。

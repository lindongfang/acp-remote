# Proposal

## Why

**用户原始要求**：把 CI 上因墙钟余量不足而红的计时用例改成不依赖墙钟、或把预算放宽到有余量（本变更选后者，理由见 design）。

**现状**：`crates/server/src/sync/conn/tests.rs` 与 `crates/server/src/node_link/conn/tests.rs` 各有一条同构用例 `a_stuck_write_still_lets_the_session_judgments_run`（后者存在性由前者注释里的「逐条同构」承诺约束）。它注入 `SILENCE = 600ms` 与写期限/看门狗窗口 `WRITE_WINDOW = 30s`，用 `OutboundPressure::spawn_with_filler` 把出站队列顶到饱和，然后断言连接在一个**有界预算**（`SILENCE + 11_400ms` = 12s）内被收尾：

- 该预算同时承担两件事：证明「收尾没有被无限期拖着」，以及**借预算远小于写窗口推断**「收尾只可能来自静默判定，而不是写期限」。
- 作者注释记录的实测：本机 ≈3.5s、CI 慢跑器 7.54s。

**问题**：这条余量在 CI 上不够。2026-10-16 同一天两次实测超时——16.30s（PR #141 的 push 触发批）与 29.68s（PR #142 的 push 触发批），两次都是**同一次 push 与 PR 事件各触发一条完整工作流、两条并行跑**导致机器负载翻倍时出现，重跑即绿。

- 29.68s 已经贴到 30s 的写窗口：一旦跨过，用例赖以判别「收尾来自静默判定」的前提就被自己的预算破坏（那时连接可能真的是被写期限收尾的，用例会以一种**误报**的方式变红）。
- 它是合并门禁里目前**唯一**会因负载而红的项：今天两次拦下 PR、逼重跑（两条 PR 各自多花约 7 分钟）。门禁一旦出现「重跑就好」的噪声，后续变更就会开始忽略它的红。

**为什么现在做**：修它是纯测试侧的小改动，却是让「六个必需检查」重新可信的前提。

## What Changes

一、两条同构用例（`crates/server/src/sync/conn/tests.rs`、`crates/server/src/node_link/conn/tests.rs`）

- 注入的写期限/看门狗窗口从 `30s` 抬到 `300s`；
- 断言预算从 `SILENCE + 11_400ms`（12s）抬到 `SILENCE + 59_400ms`（60s）；
- 同步改写记录数字与理由的注释：写清合法路径的构成（静默判定第 2 拍 tick ≈1.0–1.1s + 关闭路径按预算走满的排空与 join 各 2s + 收尾）、观测到的实测（本机 ≈3.5s、CI 慢跑器 7.54s、负载翻倍时 16.3s/29.7s）与新余量（约 2× 最差实测）。

判别力不变：预算（60s）仍远小于写窗口（300s），因此「收尾只可能来自静默判定」这层推断仍然成立，而且**再也不会**出现「实测值贴到写窗口」的自相矛盾。

二、明确不做的事

- 不新增产品侧可观察点（例如把收尾成因记进连接记录），也不给 server 测试加日志捕获设施——详见 design 的备选方案一节。
- 不改另外两个 `assert_eventually_within` 调用点（`GRACE + 8s`）：它们走的是「写任务到期立刻唤醒 `select!`」的无 tick 路径，没有实测抖动（同一天那两条工作流里它们都绿）。
- 不改 CI 触发方式与并发策略（见 Non-Goals）。

## Capabilities

### New Capabilities

（无）

### Modified Capabilities

（无 delta 文件：本变更只改测试的时间余量，不改变任何要求的语义——`sync-device-auth` 与 Node Link 对应的「写路径有期限且写卡住时判据仍被检查」要求原样成立。按 schema 约定声明 `skip_specs: true`。）

## Impact

- **只改两个测试文件**：`crates/server/src/sync/conn/tests.rs`、`crates/server/src/node_link/conn/tests.rs`。
- **不改**：任何产品代码（`crates/server/src/sync/conn/{session,registry,mod}.rs` 等）、`.github/workflows/ci.yml`、`openspec/specs/**`、`docs/**`。
- **硬约束**：不改变被测行为、不放松断言所证明的命题（「连接在写卡住时仍会被静默判定收尾」），只放大余量；`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --check`、`cargo test -p server` 与 `npm run check` 必须全绿。
- **非目标**：不修 CI 的重复执行（同一个 push 同时触发 `push` 与 `pull_request` 两条工作流，是这次负载翻倍的直接来源）——那属于 CI 配置的独立议题，见 design 的 Open Questions；不引入 tokio 时间冻结（本用例跑在真实 TCP 上，冻结时钟会让 I/O 就绪不再推进）。
- **成功判据**：
  1. 两条用例在改动后通过，且注释里的预算、写窗口与实测数字三者自洽（预算 < 写窗口，且预算 ≥ 2× 已观测最差实测）；
  2. 同一份改动下把预算临时回退到 12s、在同样的负载形态（并行跑两条完整测试套件）里该用例会红——即改动确实只放大了余量，没有顺带削弱命题（在验证阶段作为一次性对照执行，不留代码）；
  3. `cargo test -p server --lib a_stuck_write_still_lets_the_session_judgments_run` 连续通过；
  4. `npm run verify` 全绿。

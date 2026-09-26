# WP1 Handoff — 合同口径与依赖登记（`node-link-owner` / DU1）

## Shared Report

- **task_id**: 1.2 / 1.3 / 2.1 / 2.2 / 2.3（WP1；阶段 `implement`；交付单元 DU1）
- **role**: coder（WP1 实现 Agent）
- **phase**: implement
- **agent_context**: 任务级 coder 子 Agent，**继承**本工作包实现会话（`fork_turns` 不限；本 WP 是 W0 串行单执行者，无并发同写者）。工作目录为独立 worktree，未与主仓库共享构建目录。
- **target_revision**: `028a4d2a71ae01364012ffec4da9a85c7f8cf905`（本 WP 交付提交，分支 `agentic/node-link-owner`）
- **scope**: 只做 WP1：`docs/MODULE_ARCHITECTURE.md` 的口径（§5 矩阵 `server→acpr-wire` 格 + 表下注记、§4.9 `[现状]`、§3.1 依赖口径 `[决定]`）、`Cargo.toml` 的 `[workspace.dependencies]` 登记、`Cargo.lock` 解析、HTTP/WS/TLS 栈的四判据核验留证、局部验证（`npm run check` / `check:boundaries` / `cargo fmt`）。不改任何权威规划工件（`plan.md`/`tasks.md`/`verification.md` 只读），不写 `deny.toml`（核验结论为无需改动），不实现 WP2+ 的代码。
- **changes**（4 个文件，相对 base `94a64e1f15d26f54a6601985fd13b1440fec8170`；`git diff --stat` 651 insertions / 16 deletions）：
  - `docs/MODULE_ARCHITECTURE.md`：§5 矩阵 `server` 行 `acpr-wire` 格 → ✓，并在表下注记新增「只为 ACPR-CJ1 规范 JSON 与 digest 计算」的限定；§4.9 `[现状]` 追加 2026-09-26 更新（`node_link` 实现中、**尚未落地**；`sync`/`acp_facade` 待后续切片）；§3.1 新增 `[决定]`（2026-09-26）依赖口径段（选型理由、许可证、MSRV、落选候选、解析与编译证据索引）。
  - `Cargo.toml`：`[workspace.dependencies]` 追加 `axum = { version = "0.8", features = ["ws"] }`、`rustls = { version = "0.23", default-features = false, features = ["std","tls12","logging","ring"] }`、`tokio-rustls = { version = "0.26", default-features = false, features = ["logging","tls12","ring"] }`、`rustls-pki-types = { version = "1", features = ["std"] }`、`rcgen = "0.14"`（后者仅 dev 用途），逐条带理由注释。
  - `crates/server/Cargo.toml`：预登记上述依赖 + `acpr-wire = { path = "../acpr-wire" }`（`[dependencies]`）、`rcgen`（`[dev-dependencies]`），一律 `workspace = true`、不写使用代码。
  - `Cargo.lock`：随解析新增 54 个节点。
  - **未改动**：`deny.toml`（新增包未引入 allow 表之外的新 SPDX id，见下）。
- **checks**: PV1 的 W0 轮次 =`npm run check`（10 道子检查逐条记录）+ `cargo fmt --all -- --check`；PV2 = `node scripts/check-crate-boundaries.mjs`（含新增矩阵格的负向对照）。全部通过，日志见 `reports/wp1-deps.log`。**本 WP 不跑 `npm run verify`**（任务明确排除），也不跑 `cargo test`。
- **issues**: 无阻断。1 条需主 Agent/ reviewer 确认的**偏离**（`rustls-pemfile` → `rustls-pki-types` 的 `pem` 模块，理由与一行回退方式见下）；其余为如实登记的本地不可执行项。
- **result**: **PASS**（WP1 计划内检查全部满足；不等于独立 review、E2E 或合并已完成）
- **evidence_paths**:
  - 本报告：`openspec/changes/node-link-owner/reports/wp1-handoff.md`
  - 原始日志（四判据数据、来源 URL、解析与许可证对账、编译核验、局部验证、提交前门禁）：`openspec/changes/node-link-owner/reports/wp1-deps.log`
- **resource_cleanup**: worktree 自建 `target/`（与主仓库的 `D:\Project\acp-remote\target` 物理隔离）与 `node_modules` 保留作为后续 WP 的缓存；未使用端口、数据库、容器、证书或外部账号；未在仓库内留下临时文件（核验脚本与一次解析输出只在系统临时目录）。详见日志第 10 节。

## 1.2 契约与所有权确认（编码起点 / 写入范围 / 无歧义结论）

- **编码起点**：`git rev-parse HEAD` = `94a64e1f15d26f54a6601985fd13b1440fec8170`（= worktree 分支 `agentic/node-link-owner` 的固定起点，与任务输入一致；`git status --porcelain` 空 = 干净）。
- **覆盖范围不等于实际写入范围**——WP1 读取的契约：`proposal.md`、`design.md`（D1/D5）、`plan.md`（WP1 行、Execution Waves W0、Runtime Resources、PV1/PV2、Dependency Handoffs 的 WP2 行）、`tasks.md` 的 1.2/1.3/2.1/2.2/2.3、`AGENTS.md` §4/§5/§7/§10/§12、`docs/MODULE_ARCHITECTURE.md` §3.1/§4.9/§5、`docs/SECURITY_DESIGN.md` §7.1/§9.5/§20、`docs/CONFIG_REFERENCE.md` §1/§3。
- **实际写入范围**（任务白名单）：`docs/MODULE_ARCHITECTURE.md`、`Cargo.toml`、`Cargo.lock`、`deny.toml`（本 WP 未改）＋ 任务 2.2 明确授权的 `crates/server/Cargo.toml` 预登记。`crates/server/src/**`、`crates/app/**`、`crates/server/tests/**`、`.gitleaks.toml`、`README.md`、`AGENTS.md` 一律未动。
- **所有权**：WP1 是 `docs/MODULE_ARCHITECTURE.md`、`Cargo.toml`/`Cargo.lock`/`deny.toml` 的单一写入者（plan 的「文件单一写入者」）；未与其他 Agent 并发（W0 串行）。
- **结论**：**契约明确、无歧义阻塞**。两处需要在实现中定夺、并已在任务授权范围内定夺的点，均在下方写明依据：(a) 「只登记 workspace 版本」还是「同时预登记到 `crates/server/Cargo.toml`」——任务给了二选一；(b) PEM 解析候选 `rustls-pemfile` 的核验结论（偏离，已留证并给回退方式，**请 reviewer 与主 Agent 确认**）。

## 1.3 资源隔离记录

| 资源 | 用法 | 隔离 / 独占 | 释放 |
|---|---|---|---|
| `target/` | worktree 内 `D:\Project\acp-remote-wt\node-link-owner\target`（cargo 按 cwd 生成；`CARGO_TARGET_DIR` 未设置） | 与主仓库 `D:\Project\acp-remote\target` 目录不同，构建互不覆盖 = 等效独立 `CARGO_TARGET_DIR`；W0 无并发执行者 | 保留（后续 WP 复用缓存；`target/` 被 git 忽略） |
| `node_modules` | worktree 内 `npm ci`（154 包） | worktree 自带，与主仓库安装互不影响 | 保留 |
| 网络 | crates.io 索引/API/static.crates.io（核验与解析）、npm registry（`npm ci`） | 无独占需求，本机网络 | 无残留 |
| 端口 / DB 服务 / 容器 / 外部账号 / 证书 | **未使用**（本 WP 不跑任何 listener 或集成用例） | 不涉及 | 不适用 |

**如实记录的偏差**：任务书写「worktree 自带 `target/`」，实测起点提交时 worktree 内**没有** `target/`（切片 4 产物在主仓库），是本次 `cargo check` 才创建的；隔离结论不变（两个目录不同）。依据即此，无其他独占资源。

## 2.1 文档口径（`docs/MODULE_ARCHITECTURE.md`）

- §5 矩阵：`| server | ✓ | ✓ |  |  | ✓ | ✓ |  | ✓ | ✓ |  | — |  | ✓ |`（第 8 列为 `acpr-wire`，由空白改为 ✓）。
- §5 表下注记新增：`server` 对 `acpr-wire` 的依赖**只为** ACPR-CJ1 规范 JSON 与 digest 计算（`payloadDigest`/`snapshotDigest` 前像的唯一实现，`NODE_LINK_PROTOCOL.md` §12.4）；`node-link-protocol` 不再导出 `cj1`；除该模块外不得使用其业务类型。措辞与既有 `storage-sqlite` 那条注记同构。
- §4.9 `[现状]`：保留 2026-09-25 的原始段落，另起一段写明 2026-09-26 更新——`node_link` 已进入实现（交付中、**尚未落地**），`sync`/`acp_facade` 仍待后续切片；「已落地」范围仍只含 `transport::local` 与 `local_admin`。**没有**出现「已落地」的陈述。
- §3.1：新增 `[决定]`（2026-09-26）段落，写完即包含选型理由、许可证、MSRV、落选候选（无占位预告）。
- 一致性：`check:doc-links`（378 links / 4063 section refs / 257 md files）与 `check:drift`（§7 36 条 DDL、§5 15 trait / 87 方法签名）均通过；§5/§7 的矩阵与 DDL 未被改语义（§7 未动）。
- 与 `design.md` 的一致性：D5（`server` 增补 `acpr-wire` 格、只用于 digest）一致；D1 的栈结论一致（栈不变：axum + rustls/tokio-rustls），仅 PEM 解析候选改为 D1 所列 crate 的**上游指定后继**（见 2.2 的偏离条目）。

## 2.2 依赖核验与登记（本 WP 核心）

### 核验结论（`SECURITY_DESIGN.md` §20 四判据）

| 依赖 | 语义适配 | 许可证 | MSRV | 维护状态 | 结论 |
|---|---|---|---|---|---|
| `axum 0.8`（解析 0.8.9，`features = ["ws"]`） | HTTP 路由 + `axum::extract::ws` upgrade；ws 底层 `tokio-tungstenite 0.29`；不依赖 `tungstenite` 压缩 feature ⇒ 压缩拒绝在 upgrade 层显式判定 | `MIT` ✓ | 声明 1.80 ✓ | 2026-04-14 发布，仓库活跃 | **选定** |
| `rustls 0.23` + `tokio-rustls 0.26`（0.23.45 / 0.26.5，**provider = `ring`**） | 纯 Rust rustls API；两处 `default-features = false`（默认 provider 为 `aws-lc-rs`） | `Apache-2.0 OR ISC OR MIT` / `MIT OR Apache-2.0` ✓ | 都声明 1.71 ✓ | 2026-09-14 / 2026-09-04，活跃 | **选定** |
| `ring 0.17.14`（provider） | crates.io 包含预生成汇编/对象，只需 C 编译器；本机实测 `cc` 编出 16 个目标文件 + 静态库 | `Apache-2.0 AND ISC` ✓（两分支都在 allow） | 声明 1.66.0 ✓ | 仓库未归档、`pushed_at` 2026-07-23；发版慢（0.17.14 = 2025-03-11），仍被 rustls 列为受支持 provider | **选定** |
| `aws-lc-rs` / `aws-lc-sys` | ✗ `aws-lc-sys` 的 build-dependency `cmake ^0.1.54`（非 optional），Windows x86_64 还需 NASM；本机 `cmake`/`nasm` 都不存在 ⇒ 固定 Rust 工具链不足以构建 | 许可证本身在 allow 内（全部 AND/OR 分支均允许） | 1.71.0 ✓ | 活跃（2026-09-01） | **落选**（构建前提，非许可证） |
| 纯 Rust `rustls-rustcrypto` | 纯 Rust，无原生编译 | `MIT OR Apache-2.0` ✓ | 1.75 ✓ | ✗ 仅 3 个版本、最新 `0.0.2-alpha`（2024-04-24）后再无发布 | **落选**（成熟度/维护） |
| `axum-server 0.8`（替代候选） | 替调用方拥有 listener，与 D2 冲突；`tls-rustls` 写死 `rustls/aws-lc-rs` | `MIT` ✓ | 1.82 ✓ | 2025-12-06 | **落选** |
| `rustls-pemfile 2.2.0`（design D1 列的 PEM 解析候选） | 功能已被 `rustls-pki-types` 的 `pem` 模块取代（README 明示 + 迁移对照） | `Apache-2.0 OR ISC OR MIT` ✓ | **未声明** | ✗ 上游仓库 **archived**，最新发布 2024-09-30 | **不登记**（见「偏离」） |
| `rustls-pki-types 1.15.1`（`pem` 模块） | rustls 的传递依赖，不新增传递依赖；`pem` 仅受 `alloc` 门控 | `MIT OR Apache-2.0` ✓ | 声明 1.60 ✓ | 2026-07-23，活跃 | **选定** |
| `rcgen 0.14`（dev-only，解析 0.14.7） | 现算 ECDSA P-256 自签证书；实际启用的依赖链只有 `pem`/`ring`/`rustls-pki-types`/`time`/`yasna`（`zeroize`/`x509-parser`/`aws-lc-rs` 可选、未启用；除 `base64 0.22`（`pem` 与 axum 的 `ws` 共用、与既有 0.23 并存）外不引入新 crate） | `MIT OR Apache-2.0` ✓ | 0.14.7 = 1.71 ✓（0.14.8+ = 1.88，MSRV 感知解析停在 0.14.7） | 2026-08-28 | **选定**（替代提交 PEM fixture 方案） |

**crypto provider 三选一结论**：三者中 `ring` 干净满足四判据，**不存在**「三者都无法满足」的情形 ⇒ 无需回到用户决策，也未放宽任何约束（未新增任何 `[licenses] allow` 条目、未使用 `clarify`/`exceptions`、未抬 `rust-version`）。`aws-lc-rs` 若将来要采用，需先单独决定「是否把 CMake（+Windows NASM）加进构建前提」。

**测试自签证书二选一结论**：选 `rcgen`（dev-dependency）而非提交 PEM fixture。理由：(1) 不把私钥材料提交进仓库；(2) `.gitleaks.toml` 目前是空允许清单，为其新增条目属安全策略变更且超出本 WP 写入范围（本机也无 gitleaks 可复核）；(3) rcgen 在 MSRV 感知解析下取 0.14.7（1.71），满足判据。**注**：若后续 `cargo update` 使解析落到 0.14.8+（1.88），该判据会被打破——已列为后续核对点（日志第 11 节第 6 条）。

### 登记（版本口径只在 `[workspace.dependencies]`）

`axum` / `rustls` / `tokio-rustls` / `rustls-pki-types` / `rcgen` 五项；`crates/server/Cargo.toml` 预登记（`workspace = true`）+ `acpr-wire`（path）。**二选一的取舍**：选择预登记，理由是 (a) 只登记 workspace 版本时 `Cargo.lock` 不新增任何节点，`cargo check --locked` 无法给出「可解析可编译」证据，也无法覆盖 PV2 要求；(b) §5 的 `server→acpr-wire` 格只有存在真实依赖边时才被 `check:boundaries` 判定（已用负向对照证明：去掉该格 → 门禁报 `server：依赖 acpr-wire 违反 §5 依赖矩阵（该格为空白）`，exit=1）；(c) WP2 起可直接使用。代价是 `server` 暂时有未被使用的直接依赖（cargo/clippy 不因此告警，WP2 起即被消费）。**这一文件（`crates/server/Cargo.toml`）不在白名单四文件之内，依据是任务 2.2 的明确授权**。

`deny.toml`：**未改动**。54 个新增解析包出现的 license id 全部落在 `[licenses] allow` 内（`Apache-2.0`、`Apache-2.0 WITH LLVM-exception`、`BSD-3-Clause`、`ISC`、`MIT`）；另一个出现的 id `LGPL-2.1-or-later` 只作为 `r-efi 5.3.0` 的 `MIT OR Apache-2.0 OR LGPL-2.1-or-later` 第三分支（OR 由 MIT 通过），与该文件既有说明一致。全图 327 包的对账「不通过 = 0」。**无需 `clarify`/`exceptions`**。

### 解析与编译证据

- `cargo metadata --format-version 1`：exit=0；新增 54 节点；`rust_version > 1.85` 的包 **0** 个；解析图中 **没有** `aws-lc-rs`/`aws-lc-sys`/`cmake`/`rustls-pemfile`。
- `cargo check --locked -p server --all-targets --all-features`：exit=0（首次冷启动 10.32s，132 行 Compiling/Checking；`ring` 由 `cc` 现场编译出 16 个 `.o` + 2 `.a` + 2 `.lib`）。
- `cargo metadata --no-deps` / `--locked` 形态可解析（`check:boundaries`、`check:drift` 均以其为依据并通过）。

### ⚠ 需要主 Agent / 独立 reviewer 确认的偏离

**偏离**：design.md D1 把 `rustls-pemfile` 列为 PEM 解析候选，本 WP 未登记它，改登记 `rustls-pki-types`（`pem` 模块）。事实依据：上游仓库 `rustls/pemfile` 的 GitHub API 返回 `archived: true`；包内 README 明示「The main function of this crate has been incorporated into rustls-pki-types …」并给出迁移对照；该 crate 最新发布 2024-09-30 且未声明 `rust-version`；而 `rustls-pki-types` 本就是 rustls 的传递依赖（选定它不增依赖），`pem::PemObject` 即上游指定替代 API。
**性质**：这是「候选核验未通过（维护状态）后在同一家族内改用上游指定后继」，不是换栈、不放宽判据；但确实与 D1 字面清单不同，故显式上报。**回退方式**：把 `[workspace.dependencies]` 与 `crates/server/Cargo.toml` 的两处 `rustls-pki-types` 换成 `rustls-pemfile = "2"` 即可（无代码依赖，回退成本为一次解析）。请主 Agent 决定「是否同步修订 design.md D1 的候选措辞」，或要求本 WP 改回字面一致。

## 2.3 局部验证（W0 轮次）

| 检查 | 命令（worktree 根） | 结果 | 日志 |
|---|---|---|---|
| 环境准备 | `npm ci` | exit=0（154 包，0 vulnerabilities） | `reports/wp1-deps.log` §9 |
| 合同门禁（10 道逐条） | `npm run check:schemas`／`check:commands`／`check:errors`／`check:features`／`check:assets`／`check:acp`／`check:docs`／`check:boundaries`／`check:drift`／`check:agentic` | 全部 exit=0（逐条记录了 OK 行） | 同上 |
| 合同门禁（总入口） | `npm run check` | exit=0 | 同上 |
| **[PV2]** | `node scripts/check-crate-boundaries.mjs` | exit=0（`crate boundaries OK: 12 个 crate …`）；并做了负向对照（临时去掉矩阵格 → exit=1，报「server：依赖 acpr-wire 违反 §5 依赖矩阵」），证明该格被门禁真实判定 | 同上 |
| 格式 | `cargo fmt --all -- --check` | exit=0 | 同上 |
| 编译 | `cargo check --locked -p server --all-targets --all-features` | exit=0（见 §8） | 同上 |
| 提交前钩子（额外证据，非 PV1） | `.husky/pre-commit`：`cargo fmt --check` → `npm run check` → `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | 三步全部通过（clippy `Finished` 13.50s） | 同上 §12 |

`check:agentic` 在 worktree 中按预期工作（`Installation: PASS`、`openspec validate` 16 passed / 0 failed；worktree 无活动变更时不报错），因此**没有**出现预期的「无活动变更」缺口。**未执行**：`npm run verify` 的 `cargo test --locked --workspace --all-features`（任务明确排除，属 3.1/2.24）。

## 依赖包含关系 / 交接给下游（WP2 等）

- WP2 所需的「依赖口径」已冻结在本提交：`cargo metadata --no-deps` 可解析；`check:boundaries` 的 `server` 行含 `acpr-wire` 格（且被实际判定）；HTTP/WS/TLS 栈可直接 `use`。
- WP2–WP7 可直接消费：`axum`（含 `ws`）、`rustls`/`tokio-rustls`（provider = `ring`，**不要**再打开默认 feature，否则会把 `aws-lc-rs` 拉回图中并需要 CMake）、`rustls-pki-types`（`pem::PemObject` 读证书/私钥）、`acpr-wire`（`cj1` digest）、`rcgen`（dev，TLS `direct` 用例）。
- 失效条件：若本提交的依赖口径或 §5 矩阵被改动，WP2–WP7 的相关检查需按 plan 的 Dependency Handoffs 失效列重开。

## 未执行项 / 待澄清问题（不粉饰）

1. **`cargo-deny` 本地不可执行**（本机未安装，`error: no such command: deny`）：许可证/来源/advisory 的最终判定在 CI 的 `deps`/`advisories` job；本 WP 只做许可证的本地近似对账（比 CI 更严：不按 `[graph] targets` 过滤）。**推送前不得声称这三类已由本地验证通过。**
2. **`gitleaks` 本地不可执行**：`.gitleaks.toml` 未改动（本 WP 不引入私钥材料），CI 的 `secrets` job 仍会在推送后判定。
3. **`npm run verify`（PV1 全量含 `cargo test`）不在本 WP 范围**：W0 只要求 `npm run check` + `[PV2]` + `fmt`；`cargo test --locked --workspace --all-features` 属 3.1 / 2.24，**本 WP 未跑**，不得以「clippy 通过」替代。
4. **`deny.toml` 的 `multiple-versions = "warn"` 提示**：新增 `base64 0.22.1`（axum `ws` 引入）与既有 `base64 0.23.1` 并存，另有 `getrandom 0.2/0.3/0.4`、`windows-sys 0.52` 等版本族。按该键既有说明，warn 不阻塞，不构成本 WP 的失败项。
5. **待澄清（需要决定，非阻塞本 WP 提交）**：2.2 的偏离条目（`rustls-pemfile` → `rustls-pki-types`）是否被接受；若被要求字面一致，回退成本为一次解析 + 两行登记变更，请由主 Agent 决定并同步 design.md D1 措辞（本 WP 无权改 `design.md`）。
6. **未改 `deny.toml`**：不是因为「没看」，而是对账结果为无需新增 SPDX id（证据在日志 §7.3）。

## handoff_index

```yaml
handoff_index:
  - task_id: "1.2"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "028a4d2a71ae01364012ffec4da9a85c7f8cf905"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/node-link-owner/reports/wp1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "契约读取清单、编码起点（git rev-parse HEAD = 94a64e1…）、实际写入范围与『契约明确无歧义』结论均记录在本报告 §1.2；无其他角色证据复用。"
    source_evidence: NOT_APPLICABLE
  - task_id: "1.3"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "028a4d2a71ae01364012ffec4da9a85c7f8cf905"
    evidence_type: RESOURCE
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/node-link-owner/reports/wp1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "资源表（worktree 自有 target/ 与 node_modules、无端口/DB/容器/账号）；如实记录了『起点时 worktree 无 target/』这一与计划措辞的偏差；日志 §10。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.1"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "028a4d2a71ae01364012ffec4da9a85c7f8cf905"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/node-link-owner/reports/wp1-deps.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "§5 矩阵 server→acpr-wire 格 = ✓（真实依赖边存在）+ 表下限定注记 + §4.9 现状 + §3.1 [决定]；check:boundaries exit=0，并以负向对照（去掉该格 → exit=1）证明该格被判定；check:doc-links / check:drift 同版本通过。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.1"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "028a4d2a71ae01364012ffec4da9a85c7f8cf905"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/node-link-owner/reports/wp1-deps.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "W0 轮次只跑 npm run check（10 道子检查逐条 exit=0）+ cargo fmt --all -- --check（exit=0）；不属于完整 PV1（未跑 cargo test），仅作为 W0 轮次的合同门禁证据。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.2"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "028a4d2a71ae01364012ffec4da9a85c7f8cf905"
    evidence_type: CHECK
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/node-link-owner/reports/wp1-deps.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "四判据逐条数据（crates.io 索引/API + GitHub API 来源 URL）、provider 三选一结论、rcgen vs PEM fixture 结论、cargo metadata 解析（54 新增节点、0 个 rust_version>1.85、无 aws-lc-rs/cmake）、许可证对账（不通过=0）、cargo check --locked -p server 通过。无新 SPDX id ⇒ deny.toml 未改动。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.3"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "028a4d2a71ae01364012ffec4da9a85c7f8cf905"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/node-link-owner/reports/wp1-deps.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "node scripts/check-crate-boundaries.mjs exit=0（含 server→acpr-wire 格）+ 负向对照 exit=1；同轮次记录 npm run check（含 check:doc-links / check:contract-drift）与 cargo fmt --all -- --check 全绿。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.3"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "028a4d2a71ae01364012ffec4da9a85c7f8cf905"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/node-link-owner/reports/wp1-deps.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同 2.1 的 PV1 行：W0 只跑合同门禁 + fmt；完整 npm run verify（含 cargo test）留给 3.1/2.24，本行不冒充完整 PV1。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.2"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "028a4d2a71ae01364012ffec4da9a85c7f8cf905"
    evidence_type: REVIEW
    evidence_id: RV1
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "WP1 的独立 review（任务 3.2，report_path=reports/rv1-wp1.md）由不继承实现对话的执行者完成，尚未派发，因此本角色不产生该证据。"
    source_evidence: NOT_APPLICABLE
```

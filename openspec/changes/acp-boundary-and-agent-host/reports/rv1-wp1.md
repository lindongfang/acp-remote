# RV1/RV2 检视报告 · WP1（文档与依赖矩阵口径）

- Review ID：`RV-WP1-round1`（检视 `4b0145e` → `64a7582`）、`RV-WP1-recheck`（复核 `64a7582` → `e6b4dfa`）
- Reviewer：独立 reviewer 子 Agent（新隔离上下文，只读；未参与实现）
- 提交方式：由调度者写入本文件；正文为实现者收到的 reviewer 报告摘要，逐条保留其结论与依据
- 限制（reviewer 自述）：该上下文无 git 工具，无法读取提交区间 diff，结论以目标版本文件内容为据

## 第一轮结论：FAIL

| ID | 级别 | 发现 | 状态 |
| --- | --- | --- | --- |
| WP1-F1 | MAJOR | PV5 判据写「两条断言」而 `tree_` 只跑 1 个用例；断言未覆盖父进程退出与强制终止路径 | 已修：新增 `tree_terminate_while_parent_alive_stops_parent_and_grandchild`，PV5 现为 2 passed |
| WP1-F2 | MAJOR | Check Plan Change 1（`libc`→`nix`）只登记在 verification.md，design/tasks/plan 仍写 `libc` | 已修：三处全部改为 `nix` 并标注原因 |
| WP1-F3 | MINOR | `MODULE_ARCHITECTURE` 文档头仍只列 6 个 crate；§3.1 写「十二个」；`openspec/config.yaml` 仍称两个新 crate 未实现 | 已修：三处同步为 8 个成员 / 十三个 / 已落地 |
| WP1-F4 | MINOR | §5 矩阵缺 `storage-sqlite`/`node-link-client` 列（既有缺口，`app` 落地时会硬失败） | 未处理：属 App/Node Link 切片范围，已登记 |
| WP1-F5 | MINOR | `win32job` 的 MSRV/许可证无核验留证；doc 与 verification 的许可证口径不一致 | 已修：§4.5 记录本地 registry 元数据结论，并补 `nix 0.30.1` |
| WP1-F6 | MINOR | plan 声称「不改 Sync 契约」，而适配器 view 缺 `turnId`/`version` | 已修：plan 加限定并指向 Contract Changes 的缺口登记 |
| WP1-F7 | MINOR | `lib.rs` 的 `cfg` 判据不成立（`launch.rs` 有 `#[cfg(test)]`）；verification 有悬空引用 | 已修：措辞改为「平台分支只在 platform」，悬空引用删除 |

## 复核轮结论：FAIL（报告后已按报告修完）

- MAJOR：`nix` 口径未同步到 design/tasks/plan（同一文件内自相矛盾）；stderr 的权威文档与变更规范仍写「内容进结构化日志」；规范里的 stderr 超限场景（R51–R53）无用例。
- MINOR：测试计数不一致；`AGENTS.md` 仍称两个 crate 未落地；`nix` 的 MSRV/license 无留证；PV3 无独立 Checks 行；plan 的模块名仍写不存在的 `supervisor.rs`/`interaction.rs`/`catalog.rs`/`credentials.rs`；`cfg` 措辞未说明 `#[cfg(test)]` 例外。
- 处理：全部按报告修复，证据见 `reports/wp3-agent-host-supervision.log`、`reports/du1-main-verify.log` 与 `verification.md` 的「RV2 后的修复」表。
- **注意**：这批文档修复尚未再派发独立 reviewer，因此 3.2 的「无未解决阻断项」目前只有实现者证据。

---

## RV3 复核轮（独立 reviewer，新隔离上下文，2026-09-24）

复核对象：HEAD `4c24fbd`（基线 `094009b`）。工具限制：该 reviewer 上下文**没有 bash/git**，结论为静态阅读 + 契约比对，未执行任何命令（门禁由主 Agent 代跑）。

# 独立复核报告 · WP1（文档与依赖矩阵冻结）修复轮

## 0. 范围与方法

- 仓库 `D:/Project/acp-remote`；复核对象为**当前工作树**（任务给定候选 `HEAD 4c24fbd`，基线 `094009b`）。
- **工具限制（影响结论强度，必须记录）**：本上下文只有只读文件工具，**没有 bash/git**。因此：
  - 无法读取 `094009b..4c24fbd` 的提交区间 diff，也无法证明"某处修正是本变更引入的"。全部结论以磁盘上的当前文件内容为判据，配合变更目录内已记录的日志（`openspec/changes/acp-boundary-and-agent-host/reports/*.log`）作旁证。
  - **未执行** `npm run check`、`node scripts/check-crate-boundaries.mjs`。依赖矩阵门禁只做了**静态重放**（见 §4.1）。这两条命令需由 supervisor 实跑。
- 工作区脏文件仅 `.pi/settings.json`（`watchdog_diff` stat：1 file changed, 4 insertions, 1 deletion），因此"当前树 ≈ 候选提交"这一假设对本次判据无实质影响。
- 实际检视的文件（全部只读）：
  - `docs/MODULE_ARCHITECTURE.md`（头状态表、§3/§3.1/§4.1/§4.2/§4.5/§5/§8）
  - `docs/SECURITY_DESIGN.md`（§12.2、§13.3、§14.1）、`docs/DEVELOPMENT_PLAN.md`（§2/§3）、`docs/INITIAL_DESIGN.md`（第 139/626 行）
  - `AGENTS.md`（§4 第 89–104 行、§9 第 256 行）、`README.md`（仓库当前状态表 5–26 行）、`openspec/config.yaml`（context 第 10–12 行）
  - `Cargo.toml`、`Cargo.lock`、`crates/*/Cargo.toml`、`scripts/check-crate-boundaries.mjs`、`deny.toml`、`package.json`、`rust-toolchain.toml`
  - `crates/agent-host/src/{process.rs,platform.rs,host.rs,error.rs,limits.rs,lib.rs}`、`crates/agent-host/src/bin/acpr-fake-acp-agent.rs`、`crates/agent-host/tests/{supervision.rs,session.rs,catalog.rs}`
  - `openspec/changes/acp-boundary-and-agent-host/{design.md,plan.md,tasks.md,verification.md,specs/local-agent-host/spec.md,reports/*}`
  - 本地 registry 元数据：`D:/Application/Rust/.cargo/registry/src/index.crates.io-*/{win32job-2.0.3,nix-0.30.1}/Cargo.toml` 与源码（用于核对被"留证"的事实）

## 1. 逐条结论表

| 编号 | 结论 | 级别 | 证据（文件:行 / 命令+输出摘要） | 说明 |
|---|---|---|---|---|
| 1a §3.1/§4.5 依赖版本与 feature 登记 ↔ `Cargo.toml` | **PASS** | — | `docs/MODULE_ARCHITECTURE.md:135`（`tracing 0.1`、`win32job 2`、`nix 0.30`，`default-features = false`，只开 `signal`/`process`）↔ `Cargo.toml:38-44`（`tracing = "0.1"`、`win32job = "2"`、`nix = { version = "0.30", default-features = false, features = ["signal","process"] }`）；`crates/agent-host/Cargo.toml:26-32` 用 `workspace = true`；`Cargo.lock:842-845/1855-1861` = `nix 0.30.1`、`win32job 2.0.3` | 三处版本/feature 逐项一致；crate 内无第二份版本号 |
| 1b §3.1 core 直接依赖与 `p256` feature | **PASS** | — | `docs/MODULE_ARCHITECTURE.md:135`（"`core` 的直接依赖固定为 `async-trait`/`thiserror`/`p256`/`sha2`，`p256` 只开 `arithmetic`"）↔ `crates/core/Cargo.toml:19-25`（恰为该 4 项）↔ `Cargo.toml:47`（`p256 = { version = "0.13", default-features = false, features = ["arithmetic"] }`）↔ `scripts/check-crate-boundaries.mjs:172-220` 的 `CORE_ALLOWED_CLOSURE`（含 `base16ct`，不含 `ecdsa`/`hmac`/`pkcs8`） | 上一轮 WP1-1（`p256` 继承 `ecdsa`）已在根 `Cargo.toml` 收敛，三处口径一致 |
| 1c §4.5 Job 粒度与「结束=丢弃句柄」↔ 实现 | **PASS** | — | `docs/MODULE_ARCHITECTURE.md:277-281`（每 Agent 一个 Job、句柄由 Daemon 侧 supervisor 持有、结束手段是关闭句柄、`win32job 2.x` 未封装 `TerminateJobObject`）↔ `crates/agent-host/src/platform.rs:5-8,90-105`（`limit_kill_on_job_close` + `drop(lock(&self.job).take())`）↔ `crates/agent-host/src/process.rs:128`（`ProcessTree::prepare` 每次 `Supervisor::start` 各一份）↔ `crates/agent-host/src/host.rs:5`（每个 Agent 一个进程与一个 Job/进程组）；registry 源码 `win32job-2.0.3/src/**` 内 `TerminateJobObject` 零命中 | "结束=丢弃句柄"与代码逐字对应；"每 Agent 一个"由 `ProcessTree` 的构造位置证明，不存在全局 Job |
| 1d §4.2/§8/文档头状态表 ↔ 实际 crate 集合 | **PASS** | — | `docs/MODULE_ARCHITECTURE.md:3`（8 个已实现）、`:224`（§4.2 已落地 surface）、`:534-543`（§8 `acp-protocol AcpError`、`agent-host HostError -> PortError`）↔ `Cargo.toml:3-12`（8 members）↔ `crates/` 目录 8 个 ↔ `crates/acp-protocol/src/error.rs:17`（`enum AcpError`）、`crates/agent-host/src/error.rs:10,147-150`（`HostError` + `impl From<HostError> for PortError`） | 上一轮 WP2 的 `AcpCodecError` 命名偏差已修；§8 列出的其余错误类型名（`EnvelopeError`/`ValueError`/`TableError`）在对应 crate 内逐一点到 |
| 1e nix 的"许可证留证" | **FAIL** | MINOR | 文档称 `nix` 为 `MIT OR Apache-2.0`：`docs/MODULE_ARCHITECTURE.md:135`、`:281`、`openspec/changes/.../design.md:47`、`verification.md:112`；本地 registry 元数据为 **`license = "MIT"`**：`D:/Application/Rust/.cargo/registry/src/index.crates.io-*/nix-0.30.1/Cargo.toml:35`，且该目录只有一份 `LICENSE`，正文为 "The MIT License (MIT)"（`nix-0.30.1/LICENSE:1`） | 这是**事实错误而非措辞**：`verification.md:54` 同一文件写"许可证仍为 MIT"，与 `:112` 的"`nix 0.30.1` 的 `MIT OR Apache-2.0`"自相矛盾。此条正是上一轮 WP1-F5 要闭合的"许可证口径不一致"，实际未被闭合。无供应风险（`deny.toml:76` 的 allow 含 `MIT`），也不会有门禁报错——只改文档 4 处即可 |
| 2a stderr 三处口径（权威行/规范/实现） | **PASS** | — | `docs/SECURITY_DESIGN.md:375`（256 KiB / 只记计数 / 内容不进日志 / 可按上限回取）↔ `specs/local-agent-host/spec.md:148-155`（同义）↔ `crates/agent-host/src/process.rs:63-79`（`StderrRing` 有界 + `dropped` 逐字节计数）、`:593-630`（warn 只带 `dropped_bytes`/`retained_bytes`，EOF info 带 `received_bytes`/`dropped_bytes`，内容从不落日志）、`:214-221`（`stderr_snapshot` 回取）↔ `crates/agent-host/src/limits.rs:32`（`256 * 1024`） | 上一轮 MAJOR（"权威文档与规范仍写内容进日志"）已实际闭合；`stderr_flood_is_bounded_and_counted`（`tests/supervision.rs:208-241`）以 192×4 KiB（≈786 KiB）证伪"无界缓存" |
| 2b 残留"stderr 日志"措辞 | **FAIL** | MINOR | `docs/SECURITY_DESIGN.md:365`（"stderr 作为受限、脱敏的 Agent 日志"）、`docs/MODULE_ARCHITECTURE.md:270`（"stderr 日志、超时、取消和进程树清理"）、`docs/INITIAL_DESIGN.md:139`（"stderr 作为 Agent 日志单独采集"） | 三处摘要句仍可被读成"stderr 内容进日志"，与 `:375`/`:282` 的精确口径存在解释空间；权威行本身无错，故仅 MINOR |
| 2c 规范场景"stderr 不进入协议通道"的可证伪性 | **FAIL** | MINOR | `specs/local-agent-host/spec.md:157-160` 要求"stderr 上的任意文本不作为 ACP 消息被解析"；同名用例 `tests/supervision.rs:245-257` 用的是 `normal` 场景（stderr 为空），只断言 `text.len() <= STDERR_RING_BYTES` 与 `dropped == 0`；洪水用例的 stderr 内容是 `"S".repeat(4096)`（`src/bin/acpr-fake-acp-agent.rs:584-594`），非法 JSON 在 `process.rs:519-522` 会被 warn 后跳过 | 若实现把 stderr 接进协议通道，上述两条用例**仍会全绿**；该场景当前没有能失败的断言 |
| 2d 代码注释的小节指针 | **FAIL** | MINOR | `crates/agent-host/src/process.rs:610` 把"内容永不进日志"归因到 `SECURITY_DESIGN.md` §13.3；§13.3（`SECURITY_DESIGN.md:412-418`）讲的是保留/压缩/删除，对应小节是 §14.1"默认日志允许字段"（`:432-442`） | 已由上一轮 reviewer 登记为后续项（`openspec/changes/.../reports/rv1-wp4.md:22`），本轮未处理 |
| 3a `AGENTS.md` §4 状态表 | **PASS** | — | `AGENTS.md:89-101`：`core`/`acp-protocol`/`sync-protocol`/`node-link-protocol`/`acpr-transcript`/`acpr-wire`/`agent-host`/`storage-sqlite` = 已落地；`node-link-client`/`identity-auth`/`identity-keystore`/`server`/`app` = 待落地 ↔ `Cargo.toml:3-12` 的 8 个成员 ↔ `crates/` 的 8 个目录 | 已落地/待落地与实测成员集合逐名相同，无第三个状态 |
| 3b `README.md` / `DEVELOPMENT_PLAN.md` / `openspec/config.yaml` | **PASS** | — | `README.md:9-20`（8 行 crate 表 + "尚未开始：前端工程，以及 `server`、`node-link-client`、`identity-auth`、`identity-keystore`、`app`"）↔ `docs/DEVELOPMENT_PLAN.md:14`（同样 8 个 + 同样 5 个未落地）↔ `openspec/config.yaml:11`（"现有 8 个已登记成员…5 个与前端尚未实现"）↔ `Cargo.toml:3-12` | 三处 + AGENTS §4 共四处同口径，且未把 `agent-host` 的测试证据说成端到端可用（`DEVELOPMENT_PLAN.md:16` 明确"不因为门禁通过就视为已经实现"） |
| 3c `AGENTS.md` §9 的"尚未落地"名单 | **FAIL** | MINOR | `AGENTS.md:256`："（`acp-protocol`、`agent-host`、`node-link-client`、`identity-auth`、`server::*` 尚未落地，其条目在对应 crate 创建时生效）"；同一文件 `:90`/`:95` 把这两个 crate 标为已落地，`Cargo.toml:6-11` 已把它们加入 members，PV4 记录 `agent-host` 35 个测试通过（`verification.md:46`） | 同一治理文件自相矛盾，且 §9 是"改动要配哪些测试"的规范来源；`verification.md:112` 声称"`AGENTS.md` 的「尚未落地」列表去掉两个新 crate"，实际只改了 §4 |
| 4a §5 矩阵缺列的登记 | **FAIL** | MINOR | 矩阵 `docs/MODULE_ARCHITECTURE.md:424-437`：表头 9 列（`core`/`acp-protocol`/`agent-host`/`sync-protocol`/`node-link-protocol`/`acpr-transcript`/`acpr-wire`/`identity-auth`/`identity-keystore`），13 行；`storage-sqlite`/`node-link-client`/`server`/`app` **无列**。登记只在变更证据里：`openspec/changes/.../verification.md:150`、`:157`（"既有缺口，属 App/Node Link 切片范围内，本变更不改"）；权威文档 §5 与门禁脚本均无说明 | 登记存在但只在变更目录；读 `MODULE_ARCHITECTURE.md` 的人看到的是一张"13 行 × 9 列"的表，§3 又列了 13 个 crate，无法判断缺列是有意还是遗漏 |
| 4b 门禁脚本对矩阵的描述 | **FAIL** | MINOR | `scripts/check-crate-boundaries.mjs:43-46` 注释："只有 8 个 crate 同时是「列」…但 `storage-sqlite` / `agent-host` / `server` / `app` 等只作为行存在"；实测 `MODULE_ARCHITECTURE.md:424` 有 **9** 列且**含 `agent-host` 列** | 注释与矩阵现状不符（列数 8≠9；`agent-host` 已是列）。后果：脚本自身"哪些 crate 还不是列"的唯一提示是错的，掩盖了 4a 的缺口 |
| 4c 缺列的实际后果（是否为潜在硬失败） | **PASS（已确认机制，但缺口未修）** | — | `scripts/check-crate-boundaries.mjs:110-118`：成员依赖的每个工作区 crate 若不在 `columns` 内即 `errors.push("… 不在 §5 的矩阵列里…")` 并 exit 1。当前无成员依赖 `storage-sqlite`/`node-link-client`/`server`/`app`（`crates/*/Cargo.toml` 的 `path` 依赖全量：`sync-protocol→acpr-transcript,acpr-wire`、`node-link-protocol→acpr-transcript,acpr-wire`、`acpr-wire→acpr-transcript`、`storage-sqlite→core,acpr-wire`、`agent-host→core,acp-protocol`），故今天仍绿 | 一旦 `app` 落地并依赖 `storage-sqlite`（`app` 行本来就要允许它），门禁会硬失败——这正是 4a 被"下一切片"登记的后果，读者不会从权威文档预见到 |
| 5 文件所有权表措辞与写范围重叠（上一轮同批问题） | **FAIL** | MINOR | `plan.md:509` WP3 写范围含 `src/supervisor.rs`（**不存在**；实际是 `process.rs`/`host.rs`/`launch.rs`）、`:510` WP4 含 `src/interaction.rs`（不存在）、`:511` WP5 含 `src/catalog.rs`/`src/credentials.rs`（不存在）与 `tests/config_*.rs`（实际 `tests/catalog.rs`）、`:510` 的 `tests/session_*.rs`（实际 `tests/session.rs`）；`verification.md:28`（任务 1.2 的文件所有权段）重复同一批幻影名。`plan.md:522` 的波次表已改为实名，但写成"`session.rs`/`mapper.rs`/`session.rs`"（`session.rs` 重复），且把 `host.rs` 只划给 WP5 轨道，而 `host.rs:355`/`379` 同时 `impl AgentCatalog`（WP5）与 `impl SessionBackendFactory`（WP4） | `crates/agent-host/src/` 实测文件：`bin/ config.rs error.rs host.rs launch.rs lib.rs limits.rs mapper.rs platform.rs process.rs session.rs`。"两条轨道文件不重叠"与 `host.rs` 的实际职责冲突；重叠本身已被承认（`verification.md:158`），但**该表措辞未更新**，与 `verification.md:112` 的"plan 的模块名改为实际文件"不符 |
| 6 plan/tasks 的 `cfg` 判据措辞 | **FAIL** | MINOR | `plan.md:599` 关注点⑦"`cfg` 只出现在 `platform` 与 `bin`（fake child）"；`tasks.md:25`（2.17）完成条件"`rg \"cfg\\(\" crates/agent-host/src` 的结果只落在 `platform/**` 与 `bin/**`"。实测 `#\[cfg`/`cfg!` 分布：`platform.rs:62,121,177`（平台分支）、`platform.rs:21`（`cfg!(windows)`）、`launch.rs:98`（`#\[cfg(test)\]`）——**`bin/` 零命中**；证据日志自身记录了 `launch.rs` 这一条（`openspec/changes/.../reports/wp5-agent-host-config.log:28-32`） | 实质要求（平台分支只在 `platform`）成立，但 plan/tasks 里的绝对判据与已留证输出直接冲突（`launch.rs` 命中、`bin` 无命中）。上一轮声称"措辞改为「平台分支只在 platform」"只落在 `docs/MODULE_ARCHITECTURE.md:281` 与 `design.md`，未同步 plan/tasks |

## 2. 未解决阻断项清单

**BLOCKER：无。** 上一轮的 1 个 BLOCKER（stdout 超限不结束 Agent）与 stderr 相关 MAJOR（口径三处不一致、超限场景无用例）在本轮检视中均已闭合（§1 的 2a、以及 `tests/supervision.rs:398` 的超限用例存在）。

未解决项（全部 MINOR，按建议处理顺序）：

1. **F1** `nix` 许可证留证错误（`docs/MODULE_ARCHITECTURE.md:135`、`:281`、`design.md:47`、`verification.md:112` 与 `:54` 互相矛盾）→ 4 处改为 `MIT`。
2. **F2** `AGENTS.md:256` 仍称 `acp-protocol`/`agent-host` 尚未落地，与同文件 `:90`/`:95` 冲突。
3. **F3** `plan.md:509-511` + `verification.md:28` 的文件所有权仍写幻影模块名，且 `plan.md:522` 的"不重叠"与 `host.rs` 实际职责冲突（`verification.md:158` 已承认重叠，措辞未更新）。
4. **F4** §5 缺列只登记在变更 `verification.md:150/157`，权威文档 §5 与门禁脚本注释都没有说明，且脚本注释（`scripts/check-crate-boundaries.mjs:43-46`）与矩阵现状（9 列、含 `agent-host`）不符。
5. **F5** `plan.md:599` / `tasks.md:25` 的 `cfg` 判据与已留证输出（`reports/wp5-agent-host-config.log:29-32`）冲突。
6. **F6**（P2）残留"stderr 日志"措辞三处；**F7**（P2）规范场景"stderr 不进入协议通道"无能的证伪用例；**F8**（P2）`process.rs:610` 的 §13.3 指针应为 §14.1。

## 3. 可证伪依据（每条 PASS 是靠什么排除的，而不是复述实现）

- **1a**：反例是"文档写的版本/feature 与清单或锁文件不同"。逐项比对三份独立来源（文档行、`Cargo.toml`、`Cargo.lock` 的解析版本）后无差异；`nix` 的 `default-features = false` + `["signal","process"]` 若在任一来源缺失即会暴露。进一步用 registry 源码确认 `win32job-2.0.3` 真的没有 `TerminateJobObject`（全 `src/**` 零命中），即"不能走 TerminateJobObject 所以走丢句柄"这个**因果前提**为真——若该符号存在，§4.5 的理由链就是编造的。
- **1b**：反例是 `CORE_ALLOWED_CLOSURE` 与文档行不一致，或根 `Cargo.toml` 仍开 `ecdsa`。三处（doc、`crates/core/Cargo.toml`、`scripts/check-crate-boundaries.mjs:172-220`）同时包含 `p256`+`sha2` 且都不含 `ecdsa`/`hmac`/`pkcs8`；`Cargo.toml:47` 的 `default-features = false` 是它成立的唯一依据，一旦被移除，allow-list 的双向比较就会红。
- **1c**：反例是"多个 Agent 共用一个 Job"或"结束靠 TerminateJobObject"。排除方式不是读注释，而是看**构造点**：`ProcessTree::prepare` 只出现在 `process.rs:128` 的 `Supervisor::start` 内，`Supervisor` 每个 Agent 一份（`host.rs:5` 的"每个 Agent 一个进程与一个 Job/进程组"），且 Windows 分支的 `terminate()`（`platform.rs:103-105`）只做丢句柄；若存在全局 `static Job` 或 FFI 调用，`unsafe_code = "forbid"`（`Cargo.toml:55`）会先编译失败。
- **1d**：反例是"文档列了 8 个已实现而 members 只有 7 个（或反之）"。用 `Cargo.toml:3-12` 的 `members` 数组、`crates/` 目录枚举、`crates/acp-protocol/src/error.rs:17`、`crates/agent-host/src/error.rs:147-150` 四处交叉，任何一处不匹配即失败；§8 的其余错误名逐一到 crate 内查存在性（`EnvelopeError`×2、`ValueError`、`TableError`）。
- **2a**：反例是"日志里出现 stderr 内容"或"缓冲区无界"。排除方式是把 `stderr_loop`（`process.rs:593-630`）的**全部日志点**读完（只有两处，字段全为计数），并确认 `StderrRing::push` 在超限时 `pop_front` 且 `dropped += 1`（`:66-72`）、上限与文档同值（`limits.rs:32` = `256*1024`）。用例侧的反例是可构造的：把 `push` 的 `while` 循环删掉 → `stderr_flood_is_bounded_and_counted` 的 `dropped > 0` 与 `text.len() <= RING` 两条断言必红。
- **3a/3b**：反例是"某张表把 `identity-auth`/`server` 等写成已实现，或漏掉刚落地的两个 crate"。判据不是表内自洽，而是与**机器可读事实**对齐：`Cargo.toml` 的 members 是 8 个具名成员、`crates/` 是 8 个目录、13 − 8 = 5；四张表（AGENTS §4、README、DEVELOPMENT_PLAN §2、config.yaml）给出的"未落地"集合都是同一 5 个 + 前端，因此不可能同时"写反"。另用"是否出现未实现能力的完成态宣称"反向检查：`DEVELOPMENT_PLAN.md:16`、`MODULE_ARCHITECTURE.md:209`（"不得声称端到端可用"）都在，未发现越界宣称。
- **4c**：反例是"今天就有成员依赖非列 crate"（那门禁现在就会红）。用 `crates/*/Cargo.toml` 的 `path = "../"` 全量枚举（5 条边、9 个目标名）与 §5 的 9 个列名做集合包含检查，全部落列；再用 `scripts/check-crate-boundaries.mjs:110-118` 的分支语义推出"`app → storage-sqlite` 会红"，并对照 `app` 行已有 9 个 `✓`（即 `app` 一旦落地就必须依赖这些目标）——这是缺口"今天无害、下一切片必爆"的可证伪形式。
- **未执行项**（不得当作已验证）：`node scripts/check-crate-boundaries.mjs`、`node scripts/check-doc-links.mjs`、`npm run check`、任何 `cargo` 命令（本上下文无 shell）。§1 中凡引用 `crate boundaries OK: 8 …` 之处均注明来源为变更目录内的日志（如 `openspec/changes/acp-boundary-and-agent-host/reports/wp5-agent-host-config.log:25`），不是本轮复算。

## 4. 需要 supervisor 实跑的命令（本轮无法执行）

```text
node scripts/check-crate-boundaries.mjs      # 期望 exit 0：8 个 crate / 8 个已在矩阵登记
npm run check                                # 十道门禁；重点看 check:docs 与 check:boundaries
```

结论：**WP1 仍存在未解决项（4 个 MINOR + 3 个 P2），但没有 BLOCKER**；上一轮的两条 MAJOR（`libc`/`nix` 口径、stderr 三处口径）与 §5 矩阵登记问题中"文档头/§3.1/`config.yaml` 状态"这一组已实际闭合；未闭合的是**留证事实（nix 许可证）**、**AGENTS §9 状态**、**§5 缺列的披露位置**与**文件所有权/`cfg` 判据措辞**。
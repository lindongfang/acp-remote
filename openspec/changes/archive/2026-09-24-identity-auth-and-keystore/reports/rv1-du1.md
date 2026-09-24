<!-- 由独立 reviewer 子 Agent 产出（任务 6.4 DU1 候选独立 review，target 候选 ed98f6d / launch HEAD 27938ca），主 Agent 原样落盘（仅歧义 `§` 引用补全被引文档名；报告正文未改动）。 -->
# rv1-du1.md — DU1 候选独立 review 报告（任务 6.4）

```
task_id: 6.4（DU1 候选独立 reviewer）
role: 独立 reviewer（只读子 Agent；未继承任何产品实现/集成对话）
phase: candidate（候选 `ed98f6d` 的合入前独立检视；**未合入、非最终验收**）
agent_context: 新建隔离上下文；工具集仅 read/grep/ls/watchdog_diff（无 shell、无 git、不能执行命令、不能改文件）；cwd = D:\Project\acp-remote
target_revision: candidate = ed98f6d（feat/identity-auth-and-keystore，DU1 候选）；launch HEAD = 27938ca；baseline = refs/heads/main = 30f0d78（均由编排者声明）
scope: 只读检视「候选作为一个整体的新增交互与冲突解决」+ plan.md/tasks.md 6.4 明列范围：①合同定型↔实现；②Cargo.toml 成员与依赖；③.gitleaks.toml 规则与注释诚实性；④文档口径（MODULE_ARCHITECTURE/README/DEVELOPMENT_PLAN/AGENTS）；⑤两 crate 公共形状与纯状态机边界；⑥复核 [PV2] 差异结论。**不做** RV1–RV9 已完成的逐条交付前复核
changes: 本次 review 自身零改动（未创建/修改/删除任何文件，含不写 tasks.md/verification.md）；被检视候选为编排者交付的 61 文件清单（新增 identity-auth 34 文件、identity-keystore 34 文件、9 个修改文件、变更目录资产；compatibility/schemas/fixtures 0 处改动——该清单来自编排者，我无法用 git 自证）
checks: 逐条见下文 §2；均为只读核对（读源码/合同/文档/Cargo.lock/日志），**未执行任何命令**
issues: 0 × P0、0 × P1、5 × P2（文档口径 ×2 组、合同公开面登记 ×1、正常路径 expect ×1、doc-comment 残片 ×1）；无一命中 plan.md 的阻断标准
result: PASS（候选人可作为合入对象；**不等于已合入，也不等于最终验收**）
evidence_paths: 见 §5 文件清单；候选证据 reports/du1-pv1.log、du1-pv2.log、du1-pv3.log、du1-pv4.log、du1-pv5.log、du1-integration.md 均已存在并与其登记口径一致
```

---

## 1. 检视范围与限制

**本轮定位**：不做各 WP 的交付前逐条复核（RV1–RV9 已完成，RV7/RV8/RV9 均 PASS）。本轮只回答一个问题：**这两个新 crate 作为一个整体合入后，新增的交互面（两 crate 之间、以及它们与既有合同/文档/Cargo 元数据之间）是否自洽、是否有冲突或未收口的相反口径。**

**限制（必须照实声明）**：

1. **未执行任何命令**。`npm run verify` exit 0、workspace 513 passed、identity-auth 79、identity-keystore 36、DPAPI 5、`check` 九项等数字，全部**转抄**自 `reports/du1-*.log`；我做的只是读日志并交叉核对其中可自洽的部分（见 §2.6）。
2. **无 `30f0d78..ed98f6d` 提交区间 diff**。`watchdog_diff` 只返回「工作区相对 launch HEAD」的 delta，本轮实际返回 `No working-tree changes against reviewer-launch HEAD 27938ca2d716. Committed changes are not included.` 因此我的对象是**工作区内容 = 候选内容**，「候选相对 main 改了什么」的文件清单由编排者提供，我无法自证；「`27938ca` 的代码与 `Cargo.*` 等于 `ed98f6d`」同样是编排者声明。
3. **Linux 运行时与 CI 专属判定不在本地可证范围**：`cargo-deny`（`deps`/`advisories`）、`gitleaks`（`secrets`）本地无等价物；`#[cfg(unix)]` 权限位、非 Windows 失败关闭的**运行时**行为只能由 CI Linux runner 证明（候选自己已在 `verification.md` 如实登记该残余）。
4. **未复核 RV1–RV9 的逐条结论**（按交接要求），仅引用其存在与结论。

---

## 2. 逐重点结论与证据

### 2.1 重点 1：合同定型 ↔ 实现（`docs/IDENTITY_AND_AUTH_CONTRACT.md` ↔ 两个 crate）

**结论：签名级一一对应，未发现签名漂移或口径相反；发现两处「公开面/类型归属登记不完整」与一处重复段落（见 §3 的 P2-1/P2-3）。**

逐项核对（合同 → 实现）：

| 合同条目 | 实现位置 | 结论 |
|---|---|---|
| §4.1 `begin_pairing(&PairingId, &PairingSpec, &RequestedCapabilities, Option<&str>, &Timestamp, &Timestamp) -> Result<PairingDraft, PairingError>`（`IDENTITY_AND_AUTH_CONTRACT.md:110-118`） | `crates/identity-auth/src/pairing.rs:29-40` | 一致；`expires_at` 上界只复核不延长（`pairing.rs:46-50`，`PAIRING_MAX_SECONDS = 300`，`types.rs:28`） |
| §4.1 `verify_claim(&PairingRecord, Option<&ClaimedPairing>, &ClaimFields) -> Result<ClaimOutcome, PairingError>`（`:120-127`） | `pairing.rs:149-155` | 一致；幂等 `Repeat`/冲突 `NotClaimable` 与合同注释同口径（`pairing.rs:157-166`） |
| §4.1 `settle` 与「**不**在落定里置位已批准」（`:129-141`） | `pairing.rs:274-330`（批准分支显式注释）+ `pairing.rs:405`（`mark_pairing_approved`） | 一致；`settle` 只返回 `PairingSettlement::approved(...)`，置位由调用方在提交成功后调用 |
| §4.1 `due_pairings` 的「先清除、再按终态决定是否返回」与装配要求（`:145-160`） | `pairing.rs:330-344` | 一致（`clear_secret` 先于 `is_terminal()` 判断） |
| §4.1 `unrecoverable_after_restart` / `pairing_status` / `pairing_sas`（`:161-176`） | `pairing.rs:349` / `:363` / `:92` | 一致 |
| §4.1 `ClaimedPairing::to_claim()`（`:180-184`） | `pairing.rs:452` | 一致（组装 `PairingPeer` + `PairingClaim`） |
| §4.3 secret 生命周期（`:204-210`） | `state.rs:93-108`、`pairing.rs:331-344`、`state.rs:105` | 一致；明文只在 `State.secrets`，落库只有 `PairingSecret::digest()`（`pairing.rs:74`） |
| §4.4 SAS 派生（`:214-216`） | `transcript.rs:309-312` + `types.rs:282-288`（前 4 字节 u32be % 1_000_000、`{:06}`） | 一致 |
| §5.1 三入口 `hello`/`verify_proof`/`complete_auth`（`:258-305`） | `handshake.rs:72` / `:163` / `:305`（`pub(crate) complete` 于 `pairing.rs:432`） | 一致；对外只有一个名字 `complete_auth` |
| §5.1 `PeerTrust` 六字段 + `host_binding`/`node_kind`（`:283-296`） | `types.rs:645-676` | 字段名/类型逐项一致，含 `PeerTrust::unknown`（`types.rs:665`） |
| §5.1 `verify_proof` 的「快照主体 == 提交主体」附加检查（`:305-309` 附近） | `handshake.rs:189-191`（`UntrustedPeer`） | 一致 |
| §5.2 `MAX_CHALLENGES = 1024` + 清扫 + 最早过期淘汰（`:343-347`） | `state.rs:19`、`state.rs:150-168` | 一致 |
| §7 keystore trait 七方法 + `KeyPurpose{NodeIdentity}`/`SecretPurpose{ProviderCredential}`（`:428-442`） | `port.rs:17`、`:36`、`:151-186` | **逐字一致**；`DeviceIdentity` 已删除 |
| §7 熵源端口 `EntropySource::fill` + `EntropyError::Unavailable`（`:459-470`） | `port.rs:189-201` | 一致 |

**未发现**任何「合同仍写着未实现形状」的块（0.2 的目标形状草案已删除，合同 `:186-190` 保留了删除说明）；也未发现实现超出合同登记的握手/配对入口。

### 2.2 重点 2：`Cargo.toml` 成员与依赖

**结论：一致，无差异。**

- `[workspace] members`（`Cargo.toml:2-14`）含 `crates/identity-auth` 与 `crates/identity-keystore`（W1 → W2 顺序，相邻）；workspace 成员 = 10，与 `crates/` 目录实测一致（`acpr-transcript`、`acpr-wire`、`core`、`storage-sqlite`、`sync-protocol`、`identity-auth`、`identity-keystore`、`node-link-protocol`、`acp-protocol`、`agent-host`）。
- `[workspace.dependencies]` 与 `Cargo.lock` 实际解析值一致：`getrandom = "0.4"` → lock `0.4.3`（`Cargo.lock:561-562`）；`windows-dpapi = "0.2"` → lock `0.2.0`（`Cargo.lock:2009-2010`）；`hmac = "0.13"` → lock `0.13.0`（`Cargo.lock:639-640`）；`p256 = "0.13"`（`default-features=false` + `arithmetic`）→ lock `0.13.2`；`sha2 = "0.11"` → lock `0.11.0`。
- 两个 crate 的依赖集合与 `docs/MODULE_ARCHITECTURE.md` §5 矩阵（`:450-462`）一致：
  - `identity-auth`（`crates/identity-auth/Cargo.toml:13-27`）：`core` ✓、`sync-protocol` ✓、`node-link-protocol` ✓、`acpr-transcript` ✓（矩阵 `identity-auth` 行只有这四格）；**无** runtime/DB/HTTP/子进程/`acpr-wire`（grep `acpr_wire` = 0 命中）；
  - `identity-keystore`（`crates/identity-keystore/Cargo.toml:11-31`）：`identity-auth` ✓（矩阵只允许这一格）；**无** `core`（grep `acp_core` = 0 命中）。
- `Cargo.lock` 的两条成员条目与其 `Cargo.toml` 声明一致（`Cargo.lock:747-760`：acpr-transcript/async-trait/core/hmac 0.13.0/node-link-protocol/p256/serde_json/sha2 0.11.0/sync-protocol/thiserror；`Cargo.lock:763-774`：async-trait/getrandom 0.4.3/identity-auth/p256/serde_json/sha2/thiserror/windows-dpapi）。

**跨 crate 传递边说明（非问题）**：`identity-keystore` 通过 `identity-auth` 的如实转出（`crates/identity-auth/src/lib.rs:31-36`）间接使用 `core::model` 值对象（如 `PeerPublicKey`），因此 `core` 在它的**传递**闭包内。矩阵只约束**直接**边（`scripts/check-crate-boundaries.mjs` 用 `dependency.path` 判定 `workspaceEdges`），合同 §2 也把「如实转出」写成设计意图，故不构成漂移。

### 2.3 重点 3：`.gitleaks.toml` 规则与注释的诚实性

**结论：规则与注释诚实，未过度声明。**

- 规则（`.gitleaks.toml:44-52`）：`id = "acpr-keystore-plaintext-entry"`、`regex = '''(?i)QUNQS[A-Za-z0-9+_-]{16,}={0,2}'''`、`keywords = ["QUNQS"]`，并显式说明「不能写字面 `ACPK`，编码后的文本里不存在它，规则会永不评估」。`QUNQS` 与 `ACPK`（`0x41 0x43 0x50 0x4B`）的 base64 前缀自洽（`"ACP"` → `QUNQ`，第 4 字节 `K` 的 6 位头 = `S`）。
- 注释明确划出边界（`.gitleaks.toml:36-43`）：「只覆盖 `ACPK` 的 base64/base64url 编码形态；**原始二进制**条目与**被 DPAPI 包裹后的密文**都无法用模式识别」「（规则）不承担『条目被误提交』这一发现责任」，并以「未验证项（如实登记）」声明本地无 gitleaks、命中能力只能由 CI `secrets` job 证明。
- 同一口径在 `README.md:114-118`、`:154-157` 与 `docs/adr/0008-ci-supply-chain-tooling.md:110-114` 三处一致复述（含「不是『条目不会泄漏』的保证，只是最后一层减速带」）。**未见**任何声称覆盖 DPAPI 密文的表述。

### 2.4 重点 4：文档口径

**结论：被点名的 §3/§3.1/§4.8/§4.12/§5 全部与代码/Cargo 实际一致；未落地 crate 未被写成已落地；切片顺序与验收表述未被改写。但**同一文档的 §4.1 与另一份权威文档 `CORE_PORTS_AND_STORAGE.md` 里，仍有**三处**把已落地的两个 crate 写成「未实现」（P2-2）。

逐项：

- `docs/MODULE_ARCHITECTURE.md:3`（状态行）把 `identity-auth`/`identity-keystore` 列入已实现 ✓；`:5` 修订记录写明本轮改了什么 ✓。
- §3 代码块（`:107-121`）仍是「建议的 Rust Workspace」（含尚未落地的 `node-link-client`/`server`/`app`），并另有 `:125` 说明平台 crate 例外——属计划而非现状陈述，未见把未落地写成已落地；§3.1 末条「workspace 成员随实现增量增长……不得为凑齐列表创建只有占位实现的空 crate」（`:157`）与现状（10 成员）一致。
- §3.1「身份边界的依赖口径」（`:139`）：`identity-auth` 依赖集合与 `getrandom`/`windows-dpapi` 版本（`0.4.3`/`0.2.0`）与 `Cargo.toml`+`Cargo.lock` 逐项一致 ✓；`hmac 0.12 → 0.13` 的变更记录与理由（`:135`）与 `Cargo.lock:639-640` 一致 ✓。
- §4.8（`:321-338`）声明的纯状态机边界与实现一致（见 2.5）；§4.12（`:404-433`）明确「下表只有 Windows/DPAPI 一列已落地；macOS 与 Linux 后端尚未实现」，与 `crates/identity-keystore/src/platform/`（仅 `mod.rs`/`windows.rs`/`unsupported.rs`）实测一致 ✓；`Scope::User` 只硬编码在 `platform/windows.rs:19`、公开入口不接受 scope 参数 ✓；三条已知代价（winapi 0.3 停维护、无 `CRYPTPROTECT_UI_FORBIDDEN` 故总是传附加熵、附加熵绑定条目）与代码一致（`platform/windows.rs:17-22`、`entry.rs:206-218`）✓。
- §5 矩阵（`:450-462`）的 `identity-auth`/`identity-keystore` 两行与 `Cargo.toml` 实测一致；矩阵只有 9 列的既有不对称已在表下披露（`:464`）✓。
- `README.md:18-19`（两 crate 行，含「12 个 transcript domain」——实现实测 6 Sync + 6 Node Link = 12，`transcript.rs:1` 与 `lib.rs:44-49`）、`:22`（「尚未开始：前端工程，以及 `server`、`node-link-client`、`app`」）、`:28`（已落地 + 「仍未实现的是 Daemon/CLI 接线与 `server::*` 入站适配器」）均正确 ✓。
- `docs/DEVELOPMENT_PLAN.md:4`/`:14`（§2 基线：切片 3 已落地两个 crate、尚未落地 `server`/`node-link-client`/`app`/前端）、§3 的切片顺序 1→7 未被改动、切片 3 的验收表述仍是计划口径 ✓。
- `AGENTS.md:88-102` 模块状态表：`identity-auth`/`identity-keystore` = 已落地，`node-link-client`/`server`/`app` = 待落地 ✓；`:104` 保留「待落地只是边界不代表已实现」的口径 ✓。

### 2.5 重点 5：候选整体交互（两 crate 公共形状自洽性）

**结论：自洽，未发现跨 crate 接线缺陷。**

- `identity-keystore` **只**使用 `identity-auth` 的公开项：`store.rs:19-22`、`entry.rs:22`、`error.rs:7`、`entropy.rs:9`、`ephemeral.rs:14-18`、`platform/windows.rs:14` 全部经 crate 根重导出（`lib.rs`）；未见 `identity_auth::` 之外的内部路径、未见 `acp_core`/协议 crate import（grep = 0 命中）✓。
- 错误与秘密类型边界：内部 `StoreError`（`identity-keystore/src/error.rs:11-30`）经 `into_port()` 收敛为封闭的 `KeystoreError`（`identity-auth/src/port.rs:135-151`），`Io` → `Unavailable`、`Corrupt|HeaderInvalid|IdentityMismatch` → `EntryCorrupt`，`anyhow`/`std::io::Error` 不穿端口 ✓。`SecretBytes` 不实现 `Debug`/`Serialize`/`Clone` 且 `Drop` 清零（`port.rs:92-131`，含两个 `compile_fail` 文档测试）；`PairingSecret` 的 `Debug` 只输出 `<redacted>`（`types.rs:266-271`）；`KeyHandle` 的 `Debug` 只输出 `<opaque>`（`port.rs:76-81`）✓。两个 crate 的 `src/` 内 **无** `println!/eprintln!/dbg!/log::/tracing::`（grep = 0 命中）✓。
- 「平台失败关闭 + 不得静默生成新身份」：`FileKeystore` 的**全部 7 个** trait 入口第一条语句是 `self.availability.require()?`（`store.rs:265/272/281/303/315/325/338`），`list()` 亦过闸门（`store.rs:159-161`）；非 Windows 的 `wrap_secret/unwrap_secret` 恒失败（`platform/unsupported.rs:18-25`）；`new()` 由 `Availability::detected() == cfg!(windows)` 决定（`store.rs:79-88`、`lib.rs:34-36`）——因此「平台不可用 → 失败关闭且零写入」可被任何平台真实执行（`with_availability` 白盒缝）。`public_key`/`sign` 在条目缺失/损坏时返回明确错误，**从不**重新生成身份 ✓。
- `identity-auth` 保持纯状态机：`crates/identity-auth/src/**` 内 `cfg(`、`SystemTime`、`std::fs`、`std::env`、`tokio`、`Instant` **零命中**；时间只经 `acp_core::ports::Clock`（`authority.rs:56`）；随机性只经 `EntropySource`（`authority.rs:69`、`types.rs:131/232`、`handshake.rs:341`）；持锁不跨 `await`（`pairing_sas` 先 `state().material(...).cloned()` 取值、语句结束后才 `.await`，`pairing.rs:101-107`；`hello` 显式 `drop(state)`，`handshake.rs:132-146`）；锁中毒取回内层数据而非 panic（`authority.rs:78-84`）✓。
- 与合同 §8「失败路径都要有测试」相称的入口均可测：`reset_memory`/`challenge_cache_len`/`failure_count`/`has_secret` 已在合同 §5.1 登记（`:305-312`）✓。

**已考虑但判定不构成问题**（如实登记，避免遗漏疑虑）：`identity-keystore` 公开导出 `EphemeralKeystore`（`lib.rs:26`，无 `Default`、必须显式构造、不落盘），它不经 `Availability` 闸门——但合同 §7 第 4 条明确「端口必须允许『非硬件保护』的实现存在」并要求「默认不启用」，实现满足该两条件，change `design.md:109/114` 也登记了该类型，故**不**判为漂移；组合根误装配的风险属尚未存在的 `app`（切片 4）范围。

### 2.6 重点 6：复核 [PV2] 的差异结论

**结论：与 §5 矩阵对应关系成立。**

- `reports/du1-pv2.log:1-11`：命令 `node scripts/check-crate-boundaries.mjs`、revision `ed98f6d`、baseline `30f0d78`、输出 `crate boundaries OK: 10 个 crate 的依赖方向与 §5 矩阵一致（10 个已在矩阵登记）`、`EXIT=0`。脚本语义核对：`registered = workspaceEdges.keys().filter(name => allowed.has(name))`（`scripts/check-crate-boundaries.mjs:241-242`），`allowed` 来自 §5 矩阵的表行；workspace 10 个成员在 §5 都有行（含 `storage-sqlite`），故 `10 / 10` 自洽。
- 脚本同时断言「依赖不在矩阵列里」与「单元格为空」两类错误（`:141-160`）、`core` 的 runtime/DB/HTTP/子进程/wire 硬约束（`:113-128`）与 `core` 普通依赖闭包 allow-list（`:170-227`）；`identity-auth` 行不含 `acpr-wire` 格、`identity-keystore` 行只含 `identity-auth` 格——即「`identity-auth` 不得依赖 `acpr-wire`、`identity-keystore` 不得依赖 `core`」这两条被脚本真实覆盖（我另以 grep 独立复核两 crate 源码无对应 import）。
- 日志自洽性交叉核对（限于可读内容）：`reports/du1-pv1.log` 全篇 **0** 个 `FAILED`/`error[`/`warning:`/`panicked`，末行 `EXIT=0`，`Totals: 9 passed, 0 failed (9 items)`（`:61`）、`crate boundaries OK: 10`（`:36`）；`reports/du1-pv3.log` 的各二进制 15/20/26/2/9/5 + doc-tests 2 = **79**；`reports/du1-pv4.log` 3/5/6/10/12 = **36**；与 `verification.md`/`du1-integration.md` 的登记逐项相等 ✓。

---

## 3. 发现项

**P0：无。P1：无。**

### P2-1 合同 §5.2 存在逐字重复的 `[决定]` 块（normative 段落重复）
- **位置**：`docs/IDENTITY_AND_AUTH_CONTRACT.md:343-347` 与 `:350-354`（同一段「**挑战缓存有硬上限**：`MAX_CHALLENGES = 1024`……」两处，其间只有空行）。
- **证据**：grep `^\`\[决定\]\`（2026-09-24 实现）` 只命中 `:343` 与 `:350`，两段正文逐字相同；`verification.md` 的 RV6-P2-b「§5.1 有成对重复的 `[决定]` 条目」当时已删除，**这一对在 §5.2 未被发现**。
- **影响**：无相反口径，但合同是冻结文本，重复段落会让「哪一段是权威」不可判定，也与本变更「消除重复规范块」的既往修复口径不符。
- **最小修复**：删除 `:350-354`（含其后空行），保留 `:357` 起的 `[决定]` 列表。

### P2-2 三处权威文档仍把本轮已落地的两个 crate 写成「未实现」（同一根因，其中一处所在文档不在本变更声明的同步面内）
- **位置与原文**：
  - `docs/MODULE_ARCHITECTURE.md:215`（§4.1）：「仍**未**实现的只是 Daemon/CLI 接线与 `identity-auth`/`identity-keystore`；在它们落地前不得声称这些管理能力已端到端可用。」
  - `docs/CORE_PORTS_AND_STORAGE.md:11`（文档头修订行）：「**仍未实现的是 Daemon/CLI 接线与 `identity-auth`/`identity-keystore`**……」
  - `docs/CORE_PORTS_AND_STORAGE.md:1312`（§11 索引段）：「仍未实现的是 Daemon/CLI 接线与 `identity-auth`/`identity-keystore`——……」
- **证据（相反口径的对照）**：同文件状态行 `MODULE_ARCHITECTURE.md:3`「…`identity-auth`/`identity-keystore` 已实现…」、§4.8（`:321` 起）、§4.12（`:404` 起的 `[现状]` 块）；`README.md:28`「`identity-auth`/`identity-keystore` 两个身份 crate **已落地**……仍未实现的是 Daemon/CLI 接线与 `server::*` 入站适配器」；`docs/DEVELOPMENT_PLAN.md:14`；`AGENTS.md:98-99`。
- **影响**：读者若不追到 §4.8/§4.12 会被直接告知这两个 crate 不存在；`CORE_PORTS_AND_STORAGE.md` 不在本变更声明的文档同步面（`design.md:134-136` 只列 `MODULE_ARCHITECTURE`/`Cargo.toml`/`README`/`DEVELOPMENT_PLAN`/`AGENTS`/`.gitleaks.toml`），因此这是**交付物完整性**上的缺口，而不是笔误。无代码/合同/证据语义依赖该句（**不构成合入阻断**）。
- **最小修复**：三处统一改为与 `README.md:28` 同口径的一句话——「仍未实现的是 Daemon/CLI 接线与 `server::*` 入站适配器（`identity-auth`/`identity-keystore` 已落地，见 `MODULE_ARCHITECTURE.md` §4.8/§4.12）」。若父 Agent 以 `AGENTS.md` §11「影响设计的变更已经同步到权威文档」为完成定义判据，本项可升级为 P1 并在本交付单元内一并修掉。

### P2-3 合同 §2 的类型归属清单不完整，且「keystore 边界」一条与实现归属相反
- **位置与证据**：
  - §2 末条（`:50`）：「只存在于 `identity-keystore` 边界：`KeyPurpose`、`SecretPurpose`、`KeyHandle`、`SecretBytes`（§7）」——但这四者全部**定义在** `crates/identity-auth/src/port.rs`（`KeyPurpose:17`、`SecretPurpose:36`、`KeyHandle:57`、`SecretBytes:97`），§7 自己也写「端口 trait 由 `identity-auth` 定义、由 `identity-keystore` 实现」（`:421`），实现侧 `identity-keystore` 是 import 方（`store.rs:19-22`）。
  - §2「只存在于 `identity-auth`」条（`:48`）未登记 `FeatureList`（`crates/identity-auth/src/transcript.rs:138`，经 `lib.rs:46` 公开导出）、`ClientKind`（`types.rs:304`）、`PairingProof`（`types.rs:196`）——而 §5.1 的签名块直接用 `FeatureList`（`:235`），工作区另有 `acpr_wire::FeatureList`（`acpr-wire/src/lib.rs:211`）与 `sync_protocol::auth::ClientKind`（`sync-protocol/src/auth.rs:18`）**同名类型**，读者无法从合同判断 §5.1 的 `FeatureList` 指哪一个（依赖矩阵又禁止 `identity-auth` 用 `acpr-wire`）。
  - §5.1（`:305`）声称登记「`Authority` 的**全部**公开入口」，但 `Authority::new`（`crates/identity-auth/src/authority.rs:33`）未列入；我以 `pub fn|pub async fn` 全量枚举 `authority.rs`/`handshake.rs`/`pairing.rs` 逐条对照，除 `new` 外其余公开方法均已登记。
- **影响**：类型归属是「切片 4–7 的 adapter 从哪里 import」的直接依据，也是「公开面不得静默膨胀」的判据；属可维护性/可读性缺口，无行为影响。
- **最小修复**：§2 补三个类型名并括注「本 crate 自己的 `FeatureList`/`ClientKind`，与 `acpr-wire`/`sync-protocol` 的同名类型不是同一个」；末条改为「`identity-auth` 定义、`identity-keystore` 实现的端口边界：…」；§5.1 清单补 `new`（一行）。

### P2-4 正常路径上的 `expect`（已由候选如实登记）
- **位置**：`crates/identity-auth/src/types.rs:252-257`，`PairingSecret::digest()` 内 `.expect("SHA-256 的 base64url 文本必然是规范 Digest 形状")`；调用点在正常路径 `crates/identity-auth/src/pairing.rs:74`（`begin_pairing`）。
- **证据**：`AGENTS.md` §7「正常运行路径不得使用 `unwrap()`、`expect()` 或无说明的 panic」；同仓库既有 crate 的生产路径零 `expect`（`crates/agent-host/src`、`crates/acp-protocol/src` grep 均 0 命中）。候选已在 `verification.md` 的自检行与 `plan.md` 的自检口径（收窄为「**可失败路径**零命中，命中处逐条登记理由」）中逐处登记，**不是隐瞒项**。
- **影响**：该 `expect` 客观上不可能触发（SHA-256/32 字节 → base64url 43 字符必然满足 `Digest`），属「不可变式断言」；风险是实现与项目规则文本（§7）之间留下一个需人工解释的口径。
- **最小修复**：让 `digest()` 不经过文本解析路径（例如按固定 32 字节直接构造 `Digest`），或改为 `Result<Digest, PairingError>` 并由 `begin_pairing` 传播 `?`。

### P2-5 `KeyHandle` 文档注释残留半句话
- **位置**：`crates/identity-auth/src/port.rs:53-55` —— `/// Debug 只表明类型、不打印内容（`AGENTS.md` §7「日志不得记录密钥」）：` 之后是两行空文档注释再接 `#[derive(...)]`，冒号后无内容。
- **影响**：纯文字残留（rustdoc 呈现为截断句），无功能影响；与 RV7/RV8 清理的同类残片性质相同。
- **最小修复**：删除悬空冒号与两行空注释，或补回被删掉的那句理由。

---

## 4. 未覆盖 / 不可确认项

1. **未执行任何命令**：无 shell、无 git、不能运行 `npm run verify` / `cargo test` / `cargo metadata` / `node scripts/*.mjs`。所有退出码、用例数、门禁数、NUL 计数均为转抄，未独立复算。若需独立复算，supervisor 应至少执行：`npm run verify`、`node scripts/check-crate-boundaries.mjs`、`cargo test --locked -p identity-auth --all-features`、`cargo test --locked -p identity-keystore --all-features`、Windows 本机 `cargo test --locked -p identity-keystore --all-features dpapi -- --nocapture`。
2. **无提交区间 diff**：`watchdog_diff` 实测返回「No working-tree changes against reviewer-launch HEAD 27938ca2d716」，因此我检视的是工作区内容；`30f0d78..ed98f6d` 的文件清单、以及「`27938ca` 的代码与 `Cargo.*` 等于 `ed98f6d`」均由编排者声明，**我无法自证**。相应地「候选相对 main 只改了这 61 个文件」「没有误改切片顺序」这类**差异判断**我只能就现状文本给结论，不能证明改动前后一致。
3. **Linux 运行时行为**：非 Windows 失败关闭、`#[cfg(unix)]` 的 0700/0600 真实 `chmod` 行为，本机不可执行；本地证据只有「编译 + lint + 注入缝（`FileKeystore::with_availability`）」。
4. **CI 专属判定**：`cargo-deny`（`deps`/`advisories`：`getrandom`/`windows-dpapi` 及其传递依赖 `anyhow`/`log`/`winapi 0.3` 的许可证、来源、advisory）与 `gitleaks`（`secrets`：`acpr-keystore-plaintext-entry` 是否真的会在合成样例上命中）**本地无等价物**，不在可证范围；候选亦未声称通过。
5. **未复核 RV1–RV9 逐条结论**、未复核 specs 的 90 条覆盖映射逐条（R1–R90）——按交接要求不做；本轮只交叉核对了日志中的用例数与 R 编号所在测试文件的注释对齐关系。
6. **未检视**（本轮范围外）：`crates/identity-auth/tests/**` 与 `crates/identity-keystore/tests/**` 的用例内容、`proposal.md`/`specs/**` 全文、`reports/rv1-wp*.md`/`rv2-*`/`rv3-*`/`rv4-*`/`rv5-final.md`/`rv6-final.md`/`rv7-final.md`/`rv8-final.md`/`rv9-final.md` 正文（仅引用其存在与结论）、以及 `reports/**` 中的 mutation/自检日志。
7. **环境事故**：`reports/du1-integration.md` §7 登记的「双写者并发污染 → 单写者重建」我无法复核（无文件历史）；我只确认了当前 8 份 `du1-*.log` 存在、头部含 revision `ed98f6d` 与 baseline `30f0d78`、末行 `EXIT=0`，以及 `du1-pv1.log` 内 0 个失败/告警关键字。

---

## 5. 实际读过的文件清单

**源码（逐行/大段读）**
- `crates/identity-auth/src/lib.rs`、`port.rs`、`authority.rs`、`handshake.rs`、`pairing.rs`、`types.rs`、`state.rs`、`error.rs`、`authorization.rs`、`transcript.rs`
- `crates/identity-keystore/src/lib.rs`、`store.rs`、`entry.rs`、`error.rs`、`entropy.rs`、`ephemeral.rs`、`platform/mod.rs`、`platform/windows.rs`、`platform/unsupported.rs`

**构建与工具配置**
- `Cargo.toml`（workspace）、`crates/identity-auth/Cargo.toml`、`crates/identity-keystore/Cargo.toml`、`Cargo.lock`（grep 具体条目）、`.gitleaks.toml`、`scripts/check-crate-boundaries.mjs`

**权威文档**
- `docs/IDENTITY_AND_AUTH_CONTRACT.md`（§1–§9 全文）
- `docs/MODULE_ARCHITECTURE.md`（§3–§5、§4.1/§4.8/§4.12/§5、状态行与修订记录）
- `docs/DEVELOPMENT_PLAN.md`（§2–§3 与切片 3）
- `docs/adr/0008-ci-supply-chain-tooling.md`（gitleaks/残余风险段）
- `docs/CONFIG_REFERENCE.md`（§8 `identity.*` key，grep）
- `README.md`（「仓库当前状态」段与密钥扫描段）、`AGENTS.md`（§4 模块状态表、§7/§10/§11 引用）

**变更资产与证据**
- `openspec/changes/identity-auth-and-keystore/plan.md`（Scope/Contract Changes/Coverage Index/Work Packages/Execution Waves/Verification Strategy/Code Review/Main E2E/Completion Criteria）
- `openspec/changes/identity-auth-and-keystore/tasks.md`（6.1–6.8、7.1）
- `openspec/changes/identity-auth-and-keystore/verification.md`（Target/Checks/Check Plan Changes/Dependency Handoffs/Runtime Resources/Review Findings/Merge History/Final Assessment）
- `openspec/changes/identity-auth-and-keystore/design.md`（D1–D9、Risks、Migration Plan，grep + 大段读）
- `openspec/changes/identity-auth-and-keystore/proposal.md`（grep）
- `openspec/changes/identity-auth-and-keystore/reports/du1-integration.md`（全文）、`du1-pv1.log`（grep + 交叉核对）、`du1-pv2.log`（全文）、`du1-pv3.log`、`du1-pv4.log`（逐二进制用例数）
- `openspec/changes/identity-auth-and-keystore/reports/` 目录清单（确认 evidence 存在性、`rv1-du1.md` 尚不存在）

**旁证（用于同名类型/既有惯例对照）**
- `crates/sync-protocol/src/auth.rs`、`crates/sync-protocol/src/common.rs`、`crates/acpr-wire/src/lib.rs`（grep）

---

## 6. 结论

**result: PASS** —— 候选 `ed98f6d` 在「合同定型 ↔ 实现」「Cargo 成员与依赖」「gitleaks 诚实性」「文档口径（被点名章节）」「两 crate 公共形状与纯状态机边界」「[PV2] 与 §5 矩阵对应」六个方向全部自洽；`plan.md` 的**阻断标准**（规格场景缺覆盖、[PV2] 依赖差异、秘密材料可观察、平台不可用静默生成新身份、配对 secret 明文落库、跨 `await` 持锁）**一条都未被触发**。发现 5 条 P2（文档/登记/文字级），均不影响行为或合同语义，**建议在本交付单元内一并修掉**（其中 P2-2 涉及三处、跨两份文档）。

**Merge verdict: OK with notes**（无 P0/P1；P2-1/P2-2/P2-3 建议修后合入，P2-4/P2-5 可随同一次收口）。
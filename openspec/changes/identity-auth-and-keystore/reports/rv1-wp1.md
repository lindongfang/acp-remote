<!-- 由独立 reviewer 子 Agent 产出（未继承实现对话，只读），主 Agent 原样落盘。 -->
<!-- review run: 0c414ea8-8e83-4e4b-9bc6-844adb98258a；target revision: 6861046（RV1 轮次）；落盘时间：2026-09-24 -->

## 结论：**PASS**（未发现阻断项；6 项非阻断发现，其中 1 项为证据索引缺口，需在 6.2/6.3 前收口）

```
task_id          : RV1-WP1-2026-09-24
role             : reviewer（只读，未继承实现对话）
phase            : branch —— WP1（任务 1.4、2.1–2.3）交付前独立检视
agent_context    : 新建子 Agent，按调度者指令切换为独立只读 reviewer；未参与实现，未收到任何实现对话
target_revision  : 686104647e49（= 6861046，feat/identity-auth-and-keystore HEAD；工作区干净）
base_revision    : 30f0d78b803b94346ebd8972078febceec8af8d3（refs/heads/main 起点）
scope            : docs/IDENTITY_AND_AUTH_CONTRACT.md（0.1→0.2 定型）、docs/MODULE_ARCHITECTURE.md
                   §3/§3.1/§4.8/§4.12/§5、Cargo.toml 的 [workspace.dependencies]、
                   crates/identity-auth/Cargo.toml（+ 与合同定型直接相关的实现对照）
result           : PASS（P0=0，P1=0，P2=6；PASS 只对本轮 WP1 范围成立，不为 WP2/WP3 的实现背书）
evidence_paths   : reports/{w0-baseline-check,w0-baseline-tests,wp1-boundaries,wp2-*,wp3-*,wp4-boundaries,
                   du1-pv1,bv-pv3,pv5-windows-dpapi}.log、verification.md、openspec/changes/.../{plan,design,tasks}.md
resource_cleanup : 无。本轮未运行任何命令、未创建/修改/删除任何文件、未切分支、未提交
```

---

### 1) 版本与隔离声明

- **隔离**：我是本轮新建的 reviewer 子 Agent，未继承父 Agent 的实现对话，也未参与 WP1 的实现或实现讨论；我收到的输入只有调度者的任务文本与项目上下文（AGENTS.md、`roles/reviewer.md`）。**我不能证明宿主未注入其它上下文**，只能声明我所见范围。
- **版本可读性**：`watchdog_diff` 在启动时报告 `No working-tree changes against reviewer-launch HEAD 686104647e49`。即目标修订 = 6861046 且在检视期间工作区干净、可稳定读取；**但该工具不提供提交区间 diff**，因此我**无法**直接读取 `30f0d78..6861046` 的逐文件 diff（见第 5 节受限项）。我用「目标版本内容 + 变更内文档（plan/design/tasks/verification）+ 已存在的检查日志」交叉核对来补足，并在下文逐条标注哪些结论是机械可证的、哪些只是旁证。
- **本轮不执行命令**：PV1–PV5 的结论全部来自阅读 `reports/*.log`，不来自我自己的执行；我未运行 cargo/npm/脚本。

### 2) 我自己读过的文件（实际检查范围）

- 规则与角色：`openspec/schemas/agentic/roles/reviewer.md`（全文）、`AGENTS.md`（§1–§12 全文）。
- 需求/契约：`openspec/changes/identity-auth-and-keystore/{proposal.md,design.md（D1–D9/Risks/Migration）,plan.md（Coverage Index、WP 表、Project Verify、Code Review、E2E 块）,tasks.md,verification.md}`、`specs/platform-keystore/spec.md`（全文）、`specs/identity-handshake/spec.md`（快照/收尾写集相关需求）。
- 目标状态（WP1 范围）：`docs/IDENTITY_AND_AUTH_CONTRACT.md`（全文）、`docs/MODULE_ARCHITECTURE.md`（§3、§3.1、§4.8、§4.12、§5、§6 首段）、`Cargo.toml`、`crates/identity-auth/Cargo.toml`、`crates/identity-keystore/Cargo.toml`、`deny.toml`、`rust-toolchain.toml`、`package.json`。
- 目标状态（对照实现）：`crates/identity-auth/src/{lib.rs,port.rs,handshake.rs,pairing.rs,types.rs}`（相关片段）、`crates/core/src/model/identity.rs`（PairingTarget/PairingRecord）、`Cargo.lock`（hmac/getrandom/uuid/windows-dpapi）。
- 引用与门禁口径：`scripts/check-crate-boundaries.mjs`、`scripts/check-doc-links.mjs`（前 160 行）、`docs/CONFIG_REFERENCE.md`（§8 与修订记录）、`docs/SECURITY_DESIGN.md` §9/§13/§14/§20 标题、`docs/CORE_PORTS_AND_STORAGE.md` §3.5/§5.3/§9/§11.x 标题、`README.md`（仓库当前状态）、`reports/` 目录清单与 `{wp1-boundaries,du1-pv1}.log`。

### 3) 逐条发现

> 说明：F1–F3 的**位置在 WP1 交付的合同文档**、**证据在 WP2 的实现**，属跨波次接缝；修哪一侧由主 Agent 决定（我不修改任何文件）。严重度按 `roles/reviewer.md`：MINOR = 非阻断局部问题。

**F1｜MINOR（P2）——合同 §4.1「已定型」入口签名与本变更实现不一致，且定型不足以表达 §4.2 的幂等规则**
- 位置：`docs/IDENTITY_AND_AUTH_CONTRACT.md:120–150`（2026-09-24 定型块）vs `crates/identity-auth/src/pairing.rs:29–37 / :146–151 / :264–269`。
- 触发条件：任何按合同写切片 4/5 装配代码的人。
- 预期：`fn begin_pairing(...) -> Result<(PairingDraft, PairingWrite), PairingError>`、`fn verify_claim(pairing: &PairingRecord, fields: ClaimFields) -> Result<(PairingClaim, PairingClaimWrite), PairingError>`、`fn settle(pairing, peer: Option<&PairingPeer>, decision, at) -> Result<PairingSettlementWrite, PairingError>`（:124–:142）。
- 实际：`begin_pairing(&self, pairing_id, spec, requested, display_name, created_at, expires_at) -> Result<PairingDraft, _>`（:29–:37）；`verify_claim(&self, pairing, existing: Option<&ClaimedPairing>, fields) -> Result<ClaimOutcome, _>`（:146–:151）；`settle(&self, pairing, decision, at) -> Result<PairingSettlement, _>`（:264–:269）。`pairing.rs:3–5` 的模块文档明确写「持久化事实……以领域值返回，由调用方组装 `core::ports` 的写集」——**没有任何入口返回 `PairingWrite`/`PairingClaimWrite`/`PairingSettlementWrite`**（全 crate 仅注释提及这些名字）。
- 影响：① 合同作为「切片 4–7 的接口权威」（design Goals；`AGENTS.md` §1）与实际公共形状不符，照合同写会编译不过；② 合同 §4.2 第 2 条要求「重复的**相同**认领内容幂等」，而 `PairingRecord`（`CORE_PORTS_AND_STORAGE.md` §3.5）不含已固定对端，定型签名里**没有**容纳实现所需的 `existing` 快照，说明定型未真正对齐实现。
- 修复建议（最小）：把 §4.1 定型块改为实现的签名，并把「写集由调用方按 §11.6 组装」写进该块；若坚持「状态机产生写集」，则要改 `identity-auth` 并重跑 PV3、同步 design D3。
- 说明：如果只按调度者给的 (a) 口径（合同 ↔ design）核对，则合同与 design D3/D4 是**一致**的（D3 同样描述返回写集），本发现是「合同/design ↔ 代码」的偏离。

**F2｜MINOR（P2）——合同 §2 类型归属清单与实现不一致（3 个类型名不存在、1 个归属写错）**
- 位置：`docs/IDENTITY_AND_AUTH_CONTRACT.md:47`（「只存在于 `identity-auth`」清单）与 `:76–:118`（「目标形状」代码块）。
- 证据：`HandshakeCompletion`（:226）在实现里叫 `Completion`（`types.rs:784–791`，返回 `consume_pairing: Option<PairingId>` 而非审计+写集）；`ClaimVerification` 不存在，实现用 `ClaimFields`/`ClaimOutcome`/`ClaimedPairing`（`types.rs:586–593`，`ClaimedPairing::to_claim()` 在 `pairing.rs:417–420`）；`SettlementRequest` 不存在，`settle` 直接收 `PairingDecision`。同一行把 `PairingTarget` 列为 identity-auth 独有，实际它定义在 `crates/core/src/model/identity.rs:395–400`，`crates/identity-auth/src/lib.rs:29` 只是如实转出；而 §4.1 的目标形状块把它写成带载荷的 enum（`Device{canonical_origin}` / `Node{endpoint,kind}`），与 core 的纯 token enum 不同。
- 影响：合同 §2 不再是可核对的「谁拥有什么」清单；`PairingTarget` 归属写错正好会诱导切片 4/5 在 `identity-auth` 里新建同名类型（合同本身要避免的重复建模）。
- 修复建议：同步 §2 清单与 §4.1 目标形状块（删除不存在的类型名；`PairingTarget` 移入「已有并直接复用」）。

**F3｜MINOR（P2）——合同 §5.1 `PeerTrust` 字段少于实现，且三个入口未给签名**
- 位置：`docs/IDENTITY_AND_AUTH_CONTRACT.md:185–241`（`PeerTrust` at :233）。
- 预期：`{ peer, public_key, credential, scopes, grants }`（:233–:241）。
- 实际：`crates/identity-auth/src/types.rs:646–661` 另有 `host_binding: Option<String>`（Origin/Host 与 endpoint 绑定校验用，`handshake.rs:285–295`）与 `node_kind: Option<NodeKind>`（节点角色用）。实现入口是 `Authority::hello`（`handshake.rs:72`，含 `trust: &PeerTrust` at :75）、`verify_proof`（:160）、`complete_auth`（:298）；合同仅在注释里写「1. hello / 2. proof 校验 / 3. 收尾」，未给名字与签名（与 §4.1 的写法不对称）。
- 影响：合同 §5.1 的「已定型」声明不足以让 adapter 不读代码就写出调用；绑定校验所依赖的快照字段在合同里看不到。
- 修复建议：按实现补齐 `PeerTrust` 字段与三个入口签名（「验签公钥**只**来自持久化信任、不得取握手消息自带公钥」的硬约束表述保持不变——该约束在合同 :244 与实现 :160 起的检查顺序里都成立）。

**F4｜MINOR（P2）——`MODULE_ARCHITECTURE.md` §3.1 的密码学原语版本口径漏更新 `hmac 0.12→0.13`**
- 位置：`docs/MODULE_ARCHITECTURE.md:133`（`p256 0.13` + `sha2 0.11` + **`hmac 0.12`** + `base64 0.23`）vs `Cargo.toml:59`（`hmac = "0.13"`）。
- 触发条件：任何按文档核对/回退依赖版本的人。
- 旁证（方向确认）：`verification.md:23` 明写本变更的依赖改动包含「`hmac` 0.12→0.13、新增 `getrandom`/`windows-dpapi` 登记」；`Cargo.lock` 中 `identity-auth` 依赖 `hmac 0.13.0`（:753），`hmac 0.12.1` 只剩 `rfc6979`/`hkdf`/`sqlx` 等传递依赖使用。
- 违反的规则：同一节 `:134` 自述「上述四个密码学原语的**版本口径只维护在上一行**：版本号以 `Cargo.toml` 的 `[workspace.dependencies]` 为准……**变更版本必须在同一改动里更新这里并给出验证证据**」；而 `:135` 的版本记录只有 2026-09-23 的 `sha2`/`base64` 条目，没有本次 `hmac` 条目。
- 影响：权威架构文档给出的唯一版本口径是错的；`check:contract-drift` 不覆盖本文件，门禁不会发现。
- 修复建议：`:133` 改成 `hmac 0.13`，并在 `:135` 下追加 2026-09-24 条目（理由：`sha2 0.11` 的 `digest 0.11` 栈需 `hmac 0.13`；证据：`reports/wp2-workspace-tests.log`、`reports/du1-pv1.log`）。
- 归因保留：计划 Contract Changes 只要求 §3.1「登记 `getrandom` 与 DPAPI wrapper 的版本口径并修正密码学原语注释」，未要求改 `hmac` 行，因此这是**计划文本与 §3.1 自身规则之间的缺口**，不是计划明确点名要做而没做的事。

**F5｜MINOR（P2，证据索引）——`verification.md` 引用的候选轮次日志在磁盘上不存在**
- 位置：`verification.md` 的 Checks 表 3.1 行（「最终以 `reports/bv-pv1.log`/`du1-pv1.log` 为准」）与「候选轮次 / 3.1–3.7」行（`bv-pv3.log`、`bv-pv4.log`、`bv-pv5.log`、`bv-pv2.log`、`du1-pv1.log`）。
- 证据：`ls openspec/changes/identity-auth-and-keystore/reports/` 实际只有 20 个文件，其中 `bv-*` 只有 `bv-pv3.log`；`bv-pv1.log`、`bv-pv2.log`、`bv-pv4.log`、`bv-pv5.log` **不存在**（`du1-main-verify.log` 属 6.7 阶段，尚可缺）。
- 影响：4 条证据路径不可解析。缓解事实：候选 PV1/PV2 仍可由现存的 `reports/du1-pv1.log` 支撑（我在其中读到 `npm run verify` 全量输出：`check:boundaries OK: 10 个 crate`、`doc links OK: 368 relative links`、`check:rust` 编译通过）；WP1 的 PV2 轮次由 `reports/wp1-boundaries.log` 支撑（`crate boundaries OK: 8 个 crate`，与「成员尚未加入」一致）。但 PV4/PV5 的「候选轮次」只能退回 WP 轮次的 `wp3-identity-keystore.log`/`pv5-windows-dpapi.log`，与表格声称的独立候选轮次不符；WP1 的 PV1 原始输出（表格引用「doc links 367」）已被 WP4 轮次覆盖（表格自述），不可再复核。
- 修复建议：6.2/6.3 前重新生成候选轮次日志并把引用改为真实存在的路径（或删除不存在的引用并如实标注「WP1 轮次日志已被覆盖，仅可由 `wp1-boundaries.log` + `du1-pv1.log` 复核」）；不得以 WP 轮次日志充当候选轮次证据。

**F6｜MINOR（P2，跨波次收口）——合同头部状态行在目标版本已与仓库状态矛盾**
- 位置：`docs/IDENTITY_AND_AUTH_CONTRACT.md:3`（「状态：编码前合同（v1 目标形状）。`identity-auth` 与 `identity-keystore` 两个 crate 均未落地……**不代表已实现**」）与 `:76`（「目标形状的入口（实现时新增到 `identity-auth`…）」）。
- 反证：`README.md:28`「`identity-auth`/`identity-keystore` 两个身份 crate **已落地**」；`docs/MODULE_ARCHITECTURE.md:5` 修订记录「§3.1 的依赖口径记录两个身份 crate 已落地」；`AGENTS.md` §4 模块表两行均为「已落地」。
- 归因（如实）：WP1（W0）写这行时两 crate 确实尚未落地，而 WP4 的写范围不含本合同，因此无人收口——是波次接缝，不是 WP1 当时写错。
- 影响：同一分支内文档互相矛盾，影响最终验收/读者对交付状态的判断；不影响代码行为。
- 修复建议：收口状态行与 `:76` 的引导语（改为「已定型/已落地」），或在 v0.2 注记里写明定型时点。

**未发现的问题（已核对且成立，供复核者复用）**：
- 「调用方提交写集」措辞与 design D3 一致（合同 `:120`「每个入口只做纯计算与内存态变更，持久化事实由调用方提交（§4.2 的单事务写集），状态机不访问存储」；`:158` 认领/落定单事务写集）；`KeyPurpose::DeviceIdentity` 已删除且全仓零残留（全仓 grep 仅剩注释/文档说明；`port.rs:14–25` 只有 `NodeIdentity`；合同 `:314`、`:318` 与 `crates/identity-auth/tests/ports.rs:17` 一致）✔
- 依赖口径 ↔ §5 矩阵 ↔ `check:boundaries` 实际断言一致（`crates/identity-auth/Cargo.toml` 只依赖 `core`/`sync-protocol`/`node-link-protocol`/`acpr-transcript` + `async-trait`/`thiserror`/`p256`+`ecdsa`/`sha2`/`hmac`，dev-dep `serde_json`；§5 的 `identity-auth` 行 `acpr-wire` 格为空、`identity-keystore` 行只得 `identity-auth`；脚本对「依赖不在列」与「该格为空」都硬失败，且两行都在矩阵里被逐边比对）✔
- `[workspace.dependencies]` 登记口径：`getrandom = "0.4"`、`windows-dpapi = "0.2.0"`（`crates/identity-keystore/Cargo.toml` 放在 `[target.'cfg(windows)'.dependencies]`，与「只在 `cfg(windows)` 参与编译」的注释一致）；许可证口径与 `deny.toml` 的 allow（`MIT`/`Apache-2.0`）和 `[graph] targets`（windows-msvc + linux-gnu）不冲突；`getrandom 0.4.3` 确为 `uuid 1.26.1`（`v4`）既有依赖，注释属实 ✔
- `identity-auth` 零平台 `cfg`（`crates/identity-auth` 下 `cfg(windows|unix|target_os)` 零命中）✔
- 文档引用（check:doc-links）归属：门禁规则保守（只认同一子句内紧邻指名的文档；未归属只统计不判定），其 PASS 不能认证 `du1-pv1.log` 中注明的 **1406 条未归属 § 引用**；我抽查了 WP1 改动文本涉及的引用——`SECURITY_DESIGN.md` §9.2/§9.3/§9.4/§13.1/§13.2/§14.1/§14.2/§20、`CORE_PORTS_AND_STORAGE.md` §3.5/§5.3/§9/§11.2/§11.6/§11.7、`CONFIG_REFERENCE.md` §8（确含 `identity.keystore`/`identity.fail_closed_on_missing_keystore`，:159–164）、`MODULE_ARCHITECTURE.md` §4.12/§5——**全部可解析** ✔
- `docs/CONFIG_REFERENCE.md` 未被改动（旁证：文件版本仍 0.7/2026-09-23，修订记录无 2026-09-24 条目，且 §8 的两把键与本变更口径一致；机械证明见第 5 节）✔
- `rust-version` 未抬高：`Cargo.toml:18–19` 仍为 `edition = "2024"` + `rust-version = "1.85"`，与 `MODULE_ARCHITECTURE.md:131`、`rust-toolchain.toml` 注释一致；未发现为通过检查放开 `unsafe`/lint 或抬 MSRV 的痕迹 ✔
- 已知限制描述诚实：`plan.md`「未执行项」段、`verification.md` 的 2.11/2.15 行、`MODULE_ARCHITECTURE.md` §4.12 都明确写「`cargo-deny`（deps/advisories）与 `gitleaks` 本地无等价物，只在 CI 运行；DPAPI wrapper 的 advisory/许可证判定只在 CI」——**没有**声称本地通过 ✔

### 4) 覆盖索引（本工作包行）与 Check ID 核对

**Coverage Index 中与 WP1 相关的行**（`tasks` 数组含 `2.1` 的行 = 覆盖索引里唯一指向 WP1 的入口；`1.4/2.2/2.3` 没有独立 R 行，由 PV1/PV2 承担，属索引设计如此）：

| R 行 | 需求 | WP1 提供的契约前提 | 核对结论 |
| --- | --- | --- | --- |
| R72–R76 | 「长期密钥的生成、读公钥与签名是同源操作」（checks PV4/PV5） | 合同 §7（:309–:331）`pub enum KeyPurpose { NodeIdentity }`（:314）、`IdentityKeystore` 形状（:327–:338）、删除 `DeviceIdentity`（:318） | **成立**：与 `port.rs:14–25`、`:155–:190` 一致；`DeviceIdentity` 全仓零残留 |
| R88–R90 | 「非硬件保护实现默认不启用」（checks PV4） | 合同 §7 第 4 条 + `CONFIG_REFERENCE.md` §8 的 `identity.keystore` / `identity.fail_closed_on_missing_keystore` | **成立**：`CONFIG_REFERENCE.md:159–164` 两键齐备且默认值口径一致 |
| R66–R68 | 「展开表的唯一机器来源与漂移可见」（tasks `2.7`，checks PV3） | 合同 §6.1（:283「其唯一机器权威是 `compatibility/commands/v1/commands.json`」）+ `MODULE_ARCHITECTURE.md` §4.8 | **前提成立**；其可观察证据属 PV3（`reports/wp2-identity-auth-expansion.log`），不在 WP1 证据内 |

**Check ID 逐项**（WP1 的交付前检查 = plan 的 [PV1]/[PV2]，任务 2.3、3.1）：

| Check | WP1 轮次证据 | 核对结论 |
| --- | --- | --- |
| PV1（3.1） | `reports/wp1-boundaries.log`、`reports/du1-pv1.log` | **部分不可读**：`du1-pv1.log` 已被 WP4 轮次覆盖（表格自述），表格另引用的 `reports/bv-pv1.log` 不存在 → 见 F5。当前 `du1-pv1.log` 内容是候选/WP4 轮次的 `npm run verify` 全量输出，不能当作 WP1 轮次证据 |
| PV2（3.1 / 2.3） | `reports/wp1-boundaries.log` | **可读且自洽**：`crate boundaries OK: 8 个 crate 的依赖方向与 §5 矩阵一致`，符合「W0 成员尚未加入」的预期；这也是 (b) 口径一致的直接证据 |
| PV3/PV4/PV5 | — | **不属 WP1 的交付前检查**（分别 WP2/WP3）；本轮我只读了其日志路径存在性，不为其结论背书 |
| RV1（3.2） | 本报告 | 尚未落盘为 `reports/rv1-wp1.md`；`verification.md` 的 Review Findings 表仍写「尚未进行独立 review」，与事实一致 |
| 阻断标准抽查 | — | plan 的 Code Review 阻断项中，与 WP1 有关的三条（**任何 [PV2] 依赖差异、是否放宽 `unsafe`/lint 或抬高 MSRV、`cfg` 只出现在 `identity-keystore`**）经本轮静态核对**均未触发**；其余阻断项（验签公钥来源、先成功后写库、非规范 base64url、秘密进日志、配对 secret 明文落库、跨 `await` 持锁）属 WP2/WP3 范围，本轮**未审、不背书** |

### 5) 未覆盖 / 无法确认项与所需证据

1. **base→target 提交 diff 不可读**（工具限制：只提供工作区增量，且工作区在目标修订上干净）。因此以下两点只能给旁证，建议在 6.1/6.6 用 git 复核：
   - `docs/CONFIG_REFERENCE.md` 未被本变更改动 → 复核命令：`git diff --stat 30f0d78..6861046 -- docs/CONFIG_REFERENCE.md`（期望无输出）；旁证见上（版本 0.7/2026-09-23、无新键）。
   - `rust-version` 未抬高 → 复核命令：`git diff 30f0d78..6861046 -- Cargo.toml`（期望 `rust-version = "1.85"` 行不变）。
2. **`hmac 0.12→0.13` 的“同一改动内更新文档”缺口**（F4）：我无法用 diff 证明 §3.1 那一行在本变更内“本应改而未改”，只能证明「文档值 ≠ Cargo.toml 值 + 文档自述必须同步 + 本变更含该 bump」。所需补充证据：`git log -p --follow -- docs/MODULE_ARCHITECTURE.md`（或 6.1 的 diff）中该行的改动历史。
3. **F1–F3 的归因需要主 Agent 决策**：合同（WP1）与实现（WP2）不一致，修文档或修代码都能闭环；若决定改代码，必须重跑 PV3/PV4 并按 plan「Failure and Recovery」检查 WP4/DU1 证据失效。我未做任何修改。
4. **CI-only 判定**：`cargo-deny`（licenses/sources/bans/advisories）与 `gitleaks` 无本地等价物，`getrandom 0.4`/`windows-dpapi 0.2.0` 的许可证与 advisory（含传递依赖 `winapi 0.3`）结论在本轮**不可验证**；`verification.md`/`plan.md`/`MODULE_ARCHITECTURE.md §4.12` 的描述诚实（均记为「只在 CI」），但需在 6.3/7.1 或 CI 结果上补齐后才能声称这两类判定通过。
5. **未审的相邻范围**：WP2/WP3 的实现细节（transcript 装配是否只取表、`from_der`、一次性消费顺序、持锁跨 `await`、DPAPI 条目格式与原子写、秘密是否可能进 `Debug`）不在本轮 WP1 范围，属 RV1-WP2/RV1-WP3；本轮仅做了与合同定型直接相关的类型/签名/依赖对照，**不构成对它们的通过结论**。
6. **“1406 条未归属 § 引用”未逐条人工核对**：这是 `check-doc-links` 的设计边界（`AGENTS.md` §10 已声明它不能代替通读）；我只抽查了 WP1 改动文本涉及的引用（全部可解析）。若要求更强保证，需要一次针对本合同/架构文档的全量引用通读，属可选加强项。
<!-- 由独立 reviewer 子 Agent 产出（RV2 轮，target revision fe52694），主 Agent 原样落盘。 -->
## 结论：**FAIL（有阻断项）**

- RV1-WP4 的 MAJOR（`MODULE_ARCHITECTURE.md` §3.1 的 `hmac` 版本口径）**已修复**，RV1-WP1 的 6 条 P2 在**字面位置**上均已修复。
- 但本轮复核在同一批文档里发现 **3 条 P1 残留**（合同 §4.1 仍保留 0.2 的目标形状块并含不存在的类型名与 core 无法表达的 `PairingTarget`；合同 §4.1 仍写「两条路径都清除内存 secret」，与 §4.3/实现/spec 相反；§4.3 的「`expires_at` 硬上界」在实现里对 `rejected` 终态没有落点），以及 8 条 P2。
- 结论仅对应 target revision **fe52694**；不评价 WP2/WP3 的 Rust 实现总体正确性（除 §4.3 这一条按任务第 3 项要求做的合同↔实现比对）。

```
task_id          : RV2-DOCS-2026-09-24
role             : reviewer（独立只读，未继承实现对话）
phase            : branch / recheck（RV1 修复轮后的 WP1+WP4 文档与规划资产复核）
agent_context    : 新建子 Agent；输入仅为本轮 prompt 与项目上下文；未参与实现或实现讨论
target_revision  : fe526941614d（= fe52694；父 6861046 = RV1 target）
base_revision    : 任务给定 6861046（未读该区间的逐提交 diff，见 §5.1）
scope            : docs/IDENTITY_AND_AUTH_CONTRACT.md、docs/MODULE_ARCHITECTURE.md、README.md、
                   docs/DEVELOPMENT_PLAN.md、AGENTS.md、Cargo.toml/Cargo.lock、openspec/changes/
                   identity-auth-and-keystore/{design,plan,tasks,verification}.md、specs/*/spec.md、
                   reports/（变更目录与仓库根两份）
result           : FAIL（P1×3，P2×8）
evidence_paths   : 见 §2/§3；resources：无（未创建、未修改、未删除任何文件）
```

---

## 1) 隔离与版本声明

- **隔离**：我是本轮新建的 reviewer 子 Agent，未继承父 Agent 的实现对话，未参与本变更的实现。我**不能证明宿主未注入其它上下文**，只能声明我所见范围。只读了 `openspec/schemas/agentic/roles/reviewer.md`（全文）与项目上下文 `AGENTS.md`。
- **版本**：`watchdog_diff` 报告 `No working-tree changes against reviewer-launch HEAD fe526941614d`（无 staged/unstaged/untracked delta）→ 我读到的**文件内容即 target revision fe52694 的内容**，检视期间稳定。
- **限制（必须声明）**：`watchdog_diff` 不含已提交区间，我**没有读到 `6861046..fe52694` 的逐提交 diff**。因此凡「这条是本轮改的还是历史遗留」的判断我都不下结论，只报告**当前文件内容**的事实（见 §5.1 的补证命令）。
- **只读边界**：本 Agent 的工具集只有 read/grep/find/ls/watchdog_diff，**没有任何 shell**，因此我**没有执行** `npm run check`、`node scripts/check-doc-links.mjs`、`npm run verify` 或任何 git/cargo 命令。任务允许的只读命令我只能读其**完成后的日志文本**，不能自行复算退出码。

## 2) 已读取范围

| 类别 | 实际读取 |
|---|---|
| 规则/流程 | `openspec/schemas/agentic/roles/reviewer.md`、`AGENTS.md`（§4/§9/§10 定向 + 全文关键段） |
| 变更资产 | `proposal` 未读（本轮不需要）；`design.md`（全文）、`plan.md`（全文）、`tasks.md`（全文）、`verification.md`（全文）、`specs/{identity-pairing,identity-handshake,scope-expansion,platform-keystore}/spec.md`（标题全量 + pairing/handshake 正文定向） |
| 权威文档 | `docs/IDENTITY_AND_AUTH_CONTRACT.md`（§1–§7 全读）、`docs/MODULE_ARCHITECTURE.md`（§3/§3.1/§4.8/§4.12/§4.13/§5 及修订记录）、`README.md`（crate 表、权威文档表、合同检查、gitleaks 段）、`docs/DEVELOPMENT_PLAN.md`（头部+§2）、`docs/adr/0008-ci-supply-chain-tooling.md`（定向） |
| 机械对照 | `Cargo.toml`（`[workspace.dependencies]` 全文）、`Cargo.lock`（hmac/sha2/p256/base64/getrandom/windows-dpapi/digest/ecdsa 版本）、`crates/{identity-auth,identity-keystore}/Cargo.toml`、`crates/identity-auth/src/{lib,types,port,pairing,handshake,authority,state}.rs`、`crates/identity-auth/tests/pairing.rs`、`crates/core/src/model/identity.rs`（`PairingTarget`/`PairingState`）、`crates/identity-keystore/src/entry.rs` + `platform/` 目录、`.gitleaks.toml`、`.gitignore` |
| 证据 | 变更目录 `reports/` 两次全量 `ls`；根 `reports/`（`D:\Project\acp-remote\reports\`）全量 `ls` + 读取 `rv2-pv1.log`（全量结构/计数）、`rv2-pv2.log`、`rv2-linux-clippy.log`、`rv2-selfcheck-rg.log`、`du1-pv1.log`（头部）、`du1-main-verify.log`（头部）；变更目录 `reports/du1-pv1.log`（头部）；`reports/rv1-wp1.md`、`rv1-wp4.md`（全文） |

## 3) 逐条发现

### Correct（已复核成立，供后续复用）

1. **`hmac` 版本口径已修**：`docs/MODULE_ARCHITECTURE.md:133` 现为 `p256 0.13` + `sha2 0.11` + **`hmac 0.13`** + `base64 0.23`，与 `Cargo.toml:59`（`hmac = "0.13"`）、`Cargo.lock:639-640`（0.13.0）一致；另三个原语与 lock 的 0.13.2/0.11.0/0.23.1 口径一致（manifest 侧为 major.minor，属该节既有写法）。`getrandom` 明确写「实际取值 `0.4.3`」↔ manifest `0.4`/lock 0.4.3 ✓；`windows-dpapi 0.2.0` ↔ lock 0.2.0 ✓（不算漂移）。
2. **§3.1 的 2026-09-24 变更记录已补**且含该节自述要求的要素（理由/MSRV/许可证/消费方/证据，`:135` 下条目）；证据路径 `reports/du1-pv1.log`、`reports/wp2-workspace-tests.log` **在变更目录真实存在**（`du1-pv1.log` 头部确为 `npm run verify` 全量输出，含 `check:rust` 的 fmt/clippy/test）。
3. **合同 §2 的类型清单已对齐实现**：清单内每个名字都可核到实际定义，`HandshakeCompletion` 已从 §2 消失，`PairingTarget` 归入「已有并直接复用（纯 token 枚举，不带载荷）」，`Completion` 取代旧名（`crates/identity-auth/src/types.rs:775-781`）。两 crate 的 `Cargo.toml` 依赖与 §3.1 的依赖口径逐项一致（`identity-auth`：core/两个协议 crate/acpr-transcript/async-trait/thiserror/p256+ecdsa/sha2/hmac；`identity-keystore`：identity-auth/async-trait/thiserror/p256/sha2/getrandom + `cfg(windows)` 的 windows-dpapi）。
4. **§4.1 已实现形状块、§5.1、§7 与实现逐字段相符**：`begin_pairing/verify_claim/settle/due_pairings/unrecoverable_after_restart/pairing_status/pairing_sas` 与 `ClaimedPairing::to_claim` 的签名与 `pairing.rs:29/89/146/267/316/334/348/421` 完全一致；`ChallengeRequest`/`ChallengeIssue`/`ProofSubmission`/`Completion`/`ConnectionBinding`/`Authenticated`/`CredentialStatus` 与 `handshake.rs:22-64`、`types.rs:622-630/646-661/680-707/775-781` 一致；§5.1 的 `hello/verify_proof/complete_auth` 与 `handshake.rs:72/163/305` 一致；`PeerTrust` 已补 `host_binding`/`node_kind`；§7 的 trait 7 个方法与 `port.rs:156-190` 一致，`KeyPurpose` **只保留 `NodeIdentity`**（`port.rs:17-19`）且 `DeviceIdentity` 全仓无残留（`:386` 有删除说明）。
5. **§4.3 的正面口径与 spec/实现一致**：`§4.3:213`「最迟在 `expires_at` 清除；`approved` 在首次 WSS 认证成功时提前清除；`rejected` 保留到原过期时间但不得超过」↔ `specs/identity-pairing/spec.md:100`、场景 `:107-110`（「完成首次连接认证」）↔ `pairing.rs:260-267`（settle 不清除）、`pairing.rs:400-412`（complete 清除并给出 `consume_pairing`）。
6. **三个身份 crate 的落地状态口径四处一致**：`README.md:28`「两个身份 crate 已落地」+「尚未开始：前端工程，以及 `server`、`node-link-client`、`app`」；`docs/DEVELOPMENT_PLAN.md:14`（切片 3 已落地、未落地清单同）；`AGENTS.md:98-101`（两 crate 已落地、`server`/`app`/`node-link-client` 待落地）；`MODULE_ARCHITECTURE.md:3` 状态行把十个 crate 列为已实现。**没有把未落地写成已落地**，且 macOS/Linux keystore 后端在 `AGENTS.md:256`（「尚未落地」）与 `Cargo.toml`/`platform/` 实际（只有 `mod.rs`/`windows.rs`/`unsupported.rs`）上一致。
7. **`verification.md` 的 RV1 记录是如实且非 PASS-only 的**：`## Review Findings` 保留 4 行（RV1-WP1 PASS/P2×6、RV1-WP2 PASS/P2×5、RV1-WP3 **FAIL**、RV1-WP4 **FAIL（MAJOR）**）并写了修复与 recheck 证据；`## Failures and Retests` 逐条记录两次 FAIL 的复现依据、修复内容、重测证据与**残余限制**（Linux 运行时未在本机执行）。
8. **证据内容与记录数字吻合**（读根 `reports/rv2-pv1.log`）：`doc links OK: 369 relative links, 3454 section refs across 173 markdown files`、`crate boundaries OK: 10 个 crate`、`identity-auth` = 15+19+23+9+5 条用例 + 2 个 `compile_fail` doctest = **73**、`identity-keystore` = 1+5+6+8+12 = **32**、文件末尾 `EXIT=0` —— 与 `verification.md` 的「identity-auth 73 / identity-keystore 32」「369/3454」一致（但**路径引用有误**，见 F4）。

### Findings

| ID | 严重度 | 位置 | 事实与证据 | 影响 | 建议 |
|---|---|---|---|---|---|
| **RV2-DOCS-F1** | **P1** | `docs/IDENTITY_AND_AUTH_CONTRACT.md:78-117`（§4.1 第一块）+ `:4`（v0.3 版本注记） | §4.1 仍以 `[决定]` 形式保留 0.2 的「**目标形状的入口（实现时新增到 `identity-auth`）**」代码块：`pub enum PairingTarget { Device { canonical_origin: CanonicalOrigin }, Node { endpoint: NodeEndpoint, kind: NodeKind } }`（`:81-84`）、`pub struct PairingDraft { target, display_name, requested, expires_at, secret }`（`:92-98`）、`pub struct ClaimVerification`（`:103-107`）、`pub struct SettlementRequest`（`:110-113`）。对照：`crates/core/src/model/identity.rs:395-401` 的 `PairingTarget` 是 `token_enum!`（`Device => "device"`，无载荷），§2 `:46` 也写「纯 token 枚举，不带载荷」；实现里**不存在** `ClaimVerification`/`SettlementRequest`（全 crate 无定义）；实现的 `PairingDraft` 是 `types.rs:437-445` 的 `{ record, secret, server_nonce, pairing_request_id }`；而 `:4` 的 0.3 注记声称「把 §2 清单里不存在的类型名（`HandshakeCompletion`/`ClaimVerification`/`SettlementRequest`）换成实现名」 | 合同是切片 4–7 的唯一接口权威（`AGENTS.md` §1）。§4.1 同一小节内出现两个互相冲突的规范块，且第一块声明了 `core::model` **无法表达**的带载荷 `PairingTarget` 与两个不存在的类型；照它建模会编译不过或重复建模（正是该合同要避免的）。本合同没有漂移门禁（`:3` 自述） | 删除第一块，或按 §4.1 的「实现期的口径收窄」体例标明「0.2 历史形状、已废弃」，只保留真实存在的 `PairingDecision`/`RequestedCapabilities`；同时把 `:78` 的「实现时新增到 …」去掉。严重度说明：同类问题 RV1 记 MINOR，但此处落在**接口签名块**且与版本注记自相矛盾，我按 P1 报出，可由主 Agent 校准 |
| **RV2-DOCS-F2** | **P1** | `docs/IDENTITY_AND_AUTH_CONTRACT.md:149`（§4.1 `settle` 注释）与 `openspec/changes/identity-auth-and-keystore/design.md:72` | 两处都写「（拒绝可从 created/pending_confirmation 进入）**两条路径都清除内存 secret**」。对照 §4.3 `:213`「最迟在 `expires_at` 清除；`approved` 配对在首次认证成功时提前清除」、`specs/identity-pairing/spec.md:100`、以及 `crates/identity-auth/src/pairing.rs:260-267`（“**不在落定时清除 secret**（合同 §4.3）”） | 这正是 RV1-WP2 的 P2-1 触发并确认为**真实缺陷**的旧行为（批准即清除会让批准后的 `pairing-status` HMAC 证明与「首次认证成功给出 `consume_pairing`」两条要求都不可能满足）。合同/design 作为权威保留该句，会让后续实现者重新引入该缺陷 | 把 `:149` 与 `design.md:72` 改为「两条路径都**不**提前清除；首次认证成功或到达 `expires_at` 时清除」，并把 §4.3 明写为「批准本身不清除、由过期扫描兜底」，使三处口径唯一 |
| **RV2-DOCS-F3** | **P1（实现侧，需 WP2 复核）** | `crates/identity-auth/src/pairing.rs:316-328`；对照 `docs/IDENTITY_AND_AUTH_CONTRACT.md:213`、`specs/identity-pairing/spec.md:100` | `due_pairings` 的过滤条件是 `!record.state().is_terminal() && at_or_after(now, expires_at)`，而 `crates/core/src/model/identity.rs:418-420` 把 `Rejected` 计入终态。因此**已拒绝**的配对不在过期扫描集合里，`settle` 也已不清除 → 该配对的 secret 在进程内没有任何到期清除路径，只剩 `Authority::reset_memory`（`authority.rs:144-149`，仅启动时调用）。该 crate 自己的用例印证：`tests/pairing.rs:640-645` 断言 rejected 记录传给 `due_pairings` 返回空；`tests/pairing.rs:468-475` 的「到达 expires_at 必须清除」之所以成立，是因为它传的是**非终态**的 `created.draft.record`，而非拒绝后的记录 | 合同 §4.3 与 spec 需求正文都要求 secret「不得超过原过期时间」，实现里该上界对 `rejected` 不成立：配对 secret（`pairing-status` HMAC 的密钥材料）生存期超过合同上界，直到重启。属实现范围判定，我按任务第 3 项的「合同↔实现比对」报出 | 二选一（由主 Agent/WP2 决定）：(a) 实现侧对所有到达 `expires_at` 的记录（含终态）清除 secret，并把用例改为传拒绝后的记录；(b) 若认定「终态记录在组合根被移除后即无引用」，则在合同 §4.3 明写该前提，不能保留「硬上界」的无落点措辞 |
| **RV2-DOCS-F4** | P2 | `openspec/changes/identity-auth-and-keystore/verification.md`（RV1 修复轮行、Review Findings 的 Recheck Evidence、Failures and Retests）；`plan.md` 的 [PV1] 证据列 | `verification.md` 引用的 `reports/rv2-pv1.log`、`rv2-pv2.log`、`rv2-pv3.log`、`rv2-pv4.log`、`rv2-pv5.log`、`rv2-linux-clippy.log`、`rv2-selfcheck-rg.log` **在变更目录 `reports/` 下不存在**（两次全量 `ls` 无该批文件；直接 read `rv2-pv1.log`/`rv2-pv3.log` 均返回 ENOENT），实际文件在**仓库根** `D:\Project\acp-remote\reports\`。同一文档其余每一行都指向变更目录（`w0-*`、`wp2-*`、`wp3-*`、`bv-pv*` 只在变更目录存在），因此按文档自身惯例这 7 条引用不可解析。另：`.gitignore:15/20/22-25` 明确「仓库根 `reports/` 是早期变更的历史遗留、**不再接受**证据/日志目录」且 `*.log` 不入库；根目录里已有**另一个变更**的同名日志（根 `du1-pv1.log` 头部 = `DU1 / task 6.3 候选 aed9fb5 … 2026-09-23`，根 `du1-main-verify.log` 头部 = `6.7 … HEAD 86f282b`），而本变更也会引用 `reports/du1-pv1.log`、`plan.md` 的 [PV1] 证据引用 `reports/du1-main-verify.log` → 同名不同内容、歧义已实际发生（我读过内容，**不是**编造证据） | RV1-WP3 的 P0 修复重测证据不可从变更记录解析；归档/清理根目录后无法复核；`plan.md` 的 [PV1] 主分支证据当前只能解析到**另一变更**的日志 | 把 rv2 日志复制进 `openspec/changes/identity-auth-and-keystore/reports/`；跨变更同名文件（`du1-pv1.log`/`du1-main-verify.log`）一律写完整路径或改名；6.7 写 `du1-main-verify.log` 时避免与根目录遗留同名。严重度按本变更既有校准（RV1-WP1 F5、RV1-WP4 F3 均记 MINOR）记 P2，但请在归档前收口 |
| **RV2-DOCS-F5** | P2 | `README.md:114-115`、`README.md:152-154`、`docs/adr/0008-ci-supply-chain-tooling.md:110-111` | 三处仍按「自研格式**规则**尚不存在」叙述（「仍待办：…规则等密钥格式定稿再加」「**规则本身等密钥格式定稿再加**：现在只有默认规则集…触发条件是「`identity-auth`/`identity-keystore` 开始产生真实密钥」」）。对照：`.gitleaks.toml:40-50` 已有 `id = "acpr-keystore-plaintext-entry"` 规则，且两个身份 crate 已落地（触发条件已发生）。**这就是 RV1-WP4 的 MINOR F2**（其修复建议明确点了 ADR-0008），但 `verification.md` 的 RV1-WP4 行只记录了 F1（hmac）的修复，F2 至今无处置 | 安全类文档陈述与仓库现实相反：防线已存在却被描述为待办；后续 agent 可能重复添加规则或误判防线缺失。`check-doc-links` 不覆盖非链接文本，无门禁 | 三处改为「规则已随 `identity-auth-and-keystore` 落地（`.gitleaks.toml` 的 `acpr-keystore-plaintext-entry`），首次真实执行仍在 CI 的 `secrets` job」；并在 Review Findings 里为 RV1-WP4 的 F2–F6 逐条记处置 |
| **RV2-DOCS-F6** | P2 | `verification.md` 的 `## Review Findings` RV1-WP1 行（Resolution / Recheck Evidence） | 该行写「6 条全部修复」。逐条复核：F1（§4.1 签名）✓、F3（`PeerTrust` 字段+三入口签名）✓、F4（hmac）✓（见 Correct 1）、F5（删除 `bv-pv1.log` 引用）✓、F6（合同状态行）✓；**F2 只修了 §2**，它要求删掉的 `ClaimVerification`/`SettlementRequest` 仍在 §4.1 `:103/:110`（F1 条目）。该行的 Recheck Evidence 又引用 F4 中不可解析的 `reports/rv2-pv1.log` | 修复结论被高估；`verification.md` 是最终验收的输入，会掩盖仍未闭环的漂移 | 改为「F1/F3/F4/F5/F6 已修；F2 仅 §2 部分修复，§4.1 残留见 RV2-DOCS-F1」，并把 Recheck Evidence 改成可解析路径 |
| **RV2-DOCS-F7** | P2 | `design.md:68`、`design.md:76`、`design.md:109`；`design.md:72` 见 F2 | 三处与实现形状不一致：`:68` 状态机类型名 `IdentityAuthority` vs 实现 `Authority`（`authority.rs:21`）；`:76` `pairing_sas(transcript_inputs, secret)` vs 实现 `pairing_sas(&self, pairing: &PairingRecord, peer: &ClaimedPairing)`（`pairing.rs:89`）；`:109` 写条目「头部（magic **`ACPRKS`**、格式版本、`purpose`、`label`、序号/版本、**32 字节私钥标量**）」，实际 magic 是 **`ACPK`**（`crates/identity-keystore/src/entry.rs:28`），头部是 `magic(4)+版本(2)+用途(1)+标签长度(2)+标签+32 字节盐`、私钥标量在被平台包裹的部分（`entry.rs:5-23`、`identity-keystore/src/lib.rs:15-16`；`.gitleaks.toml` 与 `README.md` 也写 `ACPK`）。D3/D4 的写集口径已修好（`design.md` 的「实现期的口径收窄」段与 D4 三入口签名 ✓） | `tasks.md` 2.1 的完成条件是「文档与 `design.md` D1–D7 一致」，而这些残留会误导切片 4–7 的接线（尤其 magic 与头部字段） | 按实现订正（幂等、无行为影响的文档改动） |
| **RV2-DOCS-F8** | P2 | `verification.md` 的 `## Check Plan Changes`、自检行（RV1 后补） | `Check Plan Changes` 写「无（尚未发生需求/接口澄清或检查清单调整）」，但修复轮实际新增了计划未列的检查与判据：`cargo clippy --locked … --target x86_64-unknown-linux-gnu`（`plan.md` 的 Local Checks/Project Verify 均无此命令）、`tests/permissions.rs`（`cfg(unix)`）与 `FileKeystore::with_availability` 注入缝；同时自检行声称「原计划措辞『零命中』过强，本轮修正为『可失败路径零 `unwrap`/`expect`』」，而 `plan.md` 的「审查重点自检」现在仍写「（正常路径零命中）」，`design.md`/`tasks.md` 均无该措辞。另：`plan.md` 的 [PV4] 要求「跳过数必须如实记录」，而日志显示 `identity-keystore/tests/permissions.rs` 在 Windows 上是 `running 0 tests`（根 `rv2-pv1.log:571-575`），记录里没有该计数（残余限制另有说明） | 检查清单与记录不一致：新增判定未登记、声称的措辞修改在计划里看不到，读者无法判断本地判定边界 | 在 Check Plan Changes 记一条（新增 Linux 目标 clippy + 自检判据收窄 + `permissions.rs` 在 Windows 上 0 用例）；或把自检行改成与 `plan.md` 实际文本一致 |
| **RV2-DOCS-F9** | P2 | `docs/MODULE_ARCHITECTURE.md:410-412` | §4.12 以完成式列出「Windows：…DPAPI…」「macOS：Keychain；可用时使用不可导出或硬件保护能力」「Linux：Secret Service（D-Bus）…」，未加「计划/未实现」标记。对照：`crates/identity-keystore/src/platform/` 只有 `mod.rs`/`windows.rs`/`unsupported.rs`（无 macOS/Linux 后端），§3.1 也只声明 `cfg(windows)` 的 DPAPI wrapper，`AGENTS.md:256` 明说「macOS/Linux 的 keystore 后端**尚未落地**」 | 与 `AGENTS.md` §10「代码尚未实现的设计必须继续使用『计划』『建议』或『待验证』措辞」冲突；同一节还写着 crate「已落地」，容易被读成三平台后端都已实现 | 给这三行加「（未实现，规划中）」或改成计划式表述（`README.md` 的 crate 行已正确写「非 Windows 一律失败关闭」） |
| **RV2-DOCS-F10** | P2 | `tasks.md` 2.6、2.7 的完成条件 | 2.6 写「[PV3] 覆盖 **[R30]–[R52]** 的全部握手场景」，2.7 写「[PV3] 覆盖 **[R53]–[R68]** 的全部展开场景」。按 `plan.md` 的 Coverage Index：R53–R55 属 `identity-handshake`（「收尾副作用是认证的唯一写入点」及其两个场景），R56–R71 才属 `scope-expansion`，且 R69–R71 在索引里映射到任务 `2.7`。另有 2.6 写入口名 `complete`，合同/实现是 `complete_auth`（`handshake.rs:305`） | 任务级覆盖声明与索引错位（R53–R55 被算进展开、R69–R71 无人主张），索引本身仍完整；不改变断言强度，但让「任务↔场景」核对失真 | 2.6 改为 R30–R55、2.7 改为 R56–R71，并把 `complete` 更正为 `complete_auth` |
| **RV2-DOCS-F11** | P2 | `docs/MODULE_ARCHITECTURE.md:428`、`:5`、`:424`；`docs/DEVELOPMENT_PLAN.md:4` | RV1-WP4 的 F3–F6 未闭环也未登记处置：F3 → `:428` 已把第一条证据写成完整路径 `openspec/changes/identity-auth-and-keystore/reports/wp3-dpapi-verification.log`，但第二条仍是裸 `reports/pv5-windows-dpapi.log`（从 `docs/` 不可解析），且「两者随变更归档」与 `.gitignore:20`（这类日志不入库）不符；F4 → MSRV 句仍是「传递依赖声明的**最低** MSRV 是 `log 0.4.34` 的 1.71」（按字面最低是 anyhow 的 1.68）；F5 → 修订记录 `:5` 仍写「§3.1 的依赖口径记录**两个**身份 crate 已落地」，而 §3.1 内只有 `identity-keystore（已落地）` 带标记；F6 → `DEVELOPMENT_PLAN.md:4` 基线仍 `2026-09-23` | 归档后的证据指针不可解析 + 若干措辞级不准；不改变任何结论，但累计使「文档已收口」的结论站不住 | 逐条按 RV1-WP4 的原建议修（完整路径 + 注明日志不入库、MSRV 措辞改「声明值中的最高者」、修订记录归因改为「§3 状态行与 §3.1」、基线日期或 §2 段落点明 09-24 补充） |

## 4) plan.md 覆盖索引与本 WP 的 Check ID 核对

**Coverage Index 行数/归属（机械核对，全部通过）**

| 段 | R 行 | 索引计数 | spec 实际计数 | 结论 |
|---|---|---|---|---|
| `identity-pairing` | R1–R29 | 29 | 7 Requirement + 22 Scenario = 29 | 一致；标题逐个对应（含 R22「批准后提前清除」= `spec.md:107`、R23 = `:112`） |
| `identity-handshake` | R30–R55 | 26 | 7 + 19 = 26 | 一致（R53–R55 = `spec.md:116/120/125`） |
| `scope-expansion` | R56–R71 | 16 | 5 + 11 = 16 | 一致（R71 = `spec.md:77`） |
| `platform-keystore` | R72–R90 | 19 | 5 + 14 = 19 | 一致 |
| 合计 | R1–R90 | 90 | 90 | **无缺行**；`checks` 列只引用 PV3/PV4/PV5（均存在于 Project Verify 表），`tasks` 列引用的 `2.1`/`2.4–2.8`/`2.10`/`2.12–2.14` **全部存在于 `tasks.md`**，`evidence` 列引用的 `wp2-identity-auth-*.log`/`wp3-identity-keystore.log`/`pv5-windows-dpapi.log` 均存在于变更 `reports/` |

**本 WP（WP1+WP4）的 Check ID**

| Check | 本轮核对结论 |
|---|---|
| **PV1**（`npm run verify`，WP4 的任务 2.17/3.7） | 内容已核对：根 `reports/rv2-pv1.log` 显示 `doc links OK: 369 relative links, 3454 section refs`、`crate boundaries OK: 10 个 crate`、全部 `test result: ok`（无 failed/error）、`EXIT=0`；`identity-auth` 73、`identity-keystore` 32 与记录一致。**但引用路径不可解析**（F4），且「68 个测试二进制」我只做了用例计数复算、未逐二进制计数 |
| **PV2**（`check-crate-boundaries`） | 根 `reports/rv2-pv2.log` 全文 = `crate boundaries OK: 10 个 crate …`；与 `Cargo.toml` members（10 项）和 §5 矩阵两行一致；同见 F4 的路径问题 |
| **PV3 / PV4 / PV5** | 属 WP2/WP3；本轮仅作为 WP1/WP4 文档陈述的**事实来源**使用（不在本轮背书其用例强度） |
| **RV1**（3.2/3.4/3.6/3.8） | `plan.md` 的 RV1 证据列 `reports/rv1-wp1.md`…`rv1-wp4.md` **均在变更 `reports/` 存在**（不再是 RV1 时「无报告」的状态）；`reports/rv1-du1.md` 属 6.4，尚未产生（正常） |
| **WP4 的 R 行** | 索引中**没有**任何 R 行映射到 2.16/2.17（与 WP4 的 Verification 列 = [PV1]/[PV2] 一致）；因此 WP4 的验收只能靠「文档 ↔ 实际元数据」的静态一致性——本轮 F1/F2/F5/F7/F9/F11 正是这条一致性上的失败点 |
| 自检项（`plan.md` Local Checks） | `cfg` 零命中、`.await` 持锁对照：`identity-auth` 无平台 `cfg`（`lib.rs` 仅在文档注释提及 `#[cfg(test)]`）、`verification.md` 自检行**如实记录了不符预期项**（`types.rs:256`、`entropy.rs:37-38`、`ephemeral.rs:50`，根 `rv2-selfcheck-rg.log` 内容与之一致）——记录诚实，但判据措辞与计划不一致（F8） |

## 5) 未覆盖 / 无法确认项与所需证据

1. **`6861046..fe52694` 的提交区间 diff 未读**（工具限制，见 §1）。因此 F1/F2/F7/F11 的「是本轮修复遗留还是历史遗留」「是否属于本变更改动」我无法归因，只能报告当前内容事实。建议主 Agent 补：`git diff 6861046..fe52694 -- docs/IDENTITY_AND_AUTH_CONTRACT.md docs/MODULE_ARCHITECTURE.md README.md openspec/changes/identity-auth-and-keystore/{design,plan,tasks,verification}.md`。
2. **我未执行任何命令**（无 shell/无 git/cargo 工具）：所有「PASS/exit 0」结论都来自我**读取的日志文本**，退出码未由我复算。需要项目 verify 执行者按同一命令复跑并留证的项目：`npm run verify`、`node scripts/check-crate-boundaries.mjs`、`node scripts/check-doc-links.mjs`（尤其确认 369 链接/3454 引用在当前固定版本上仍绿）；以及 CI-only 的 `deps`/`advisories`/`secrets` 三项（本地无等价物，不得声称通过）。
3. **`hmac 0.13.0` 的 `rust-version = 1.85` 与 `MIT OR Apache-2.0` 无法从仓库内核实**（本机无 registry 源码/`cargo metadata` 输出）：§3.1 新增记录里的这两条属「未经本轮证实的自述」，需要 `cargo tree -i hmac --locked` 或 `cargo metadata` 证据。同类：§4.12 的 `windows-dpapi` 语义（`encrypt_data`/`decrypt_data` + `Scope::{User,Machine}`）与「不暴露 `CRYPTPROTECT_UI_FORBIDDEN`」我同样无法独立确认（RV1-WP4 已在 §5 第 5 条记录过该限制）。
4. **Linux 运行时行为未验证**：`identity-keystore` 的 `cfg(unix)` 权限路径与 `Unavailable` 失败关闭的**运行时**结论只有编译/lint（根 `rv2-linux-clippy.log`，`EXIT=0`）+ 注入缝证据（`with_availability`）；`tests/permissions.rs` 在 Windows 上收集 0 个用例。需 CI Linux runner 的实际用例结果。
5. **未覆盖范围**：`server`/`app`/`node-link-client` 尚无实现，无法核对（本轮只核对「是否被误写成已落地」）；WP2/WP3 用例的断言强度（是否恒真、是否有 flaky）不在本轮范围；`proposal.md` 与 `openspec/specs/` 归档目标未读。**未执行 E2E**（`plan.md` mode = `not-applicable`，本轮不对替代验证 7.1 做任何通过声明）。
6. **低置信、未计为发现的观察**：`README.md:44` 的「（实现前合同）」与 `MODULE_ARCHITECTURE.md:412` 的「目标签名见 … §7」在合同头部已写「已定型并已落地」后，容易被读成状态标签；但 `MODULE_ARCHITECTURE.md:338-340` 用「实现前合同」作文体词（「本 crate 的实现前合同…已随 … 定型在 …」），所以我按文体标签理解，**不作为发现**，仅在措辞收口时可一并考虑。
7. **`vitiation` 无、资源无**：本轮只读，未创建/修改/删除任何文件，未切分支、未提交、未释放任何资源（无并发执行者）。

---
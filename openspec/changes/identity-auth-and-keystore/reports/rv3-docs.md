<!-- 落盘说明：主 Agent 原样保留 reviewer 的正文与结论；仅当某处 `§` 引用会触发文档引用门禁的歧义归属时，补全被引文档名（不改变语义）。 -->
<!-- 由独立 reviewer 子 Agent 产出（RV3 轮，target revision 5ed869f），主 Agent 原样落盘。 -->
<!-- review run: rv3-docs（见 RV3 工作流 b80022df-27b1-49e1-8695-face55f9a776）；落盘时间：2026-09-24 -->
## 结论：**FAIL**（1 条阻断项：RV3-DOCS-1，P1，纯文档一致性）

- **target revision**：`5ed869f`（父 `fe52694` = RV2 target，`6861046` = RV1 target）。`watchdog_diff` 建议本轮读到的文件内容即 `5ed869f` 的内容（相对 launch HEAD `5ed869f453be` 无任何工作区改动）。
- **阻断项**：**是，1 条**——RV2-DOCS F2（P1）只闭环了一半：合同 §4.1 已改对，`design.md:72` 仍写着 RV2 明确要求改掉的那句话（见 §3.2、§4 RV3-DOCS-1）。这是单行文档修复，不涉及代码、协议或证据链。
- **无 P0**；本轮未发现新的实现类阻断（我的范围不含 WP2/WP3 实现正确性）。

**与上一轮 P1 的逐条对照**

| 上一轮 P1 | 位置 | 本轮复核 | 依据（本轮实读） |
| --- | --- | --- | --- |
| RV2-DOCS F1 — 合同 §4.1 保留 0.2「目标形状」草案块 | `docs/IDENTITY_AND_AUTH_CONTRACT.md` §4.1 | **已闭环** | §4.1 现有 `[已废弃]` 说明块并把草案**删除**；§4.1 每个类型名均可核到实现（§3.1） |
| RV2-DOCS F2 — 「两条路径都清除内存 secret」 | 合同 §4.1 + `design.md:72` | **未闭环（残留 1 处）** | 合同 `:125` 已改为「**两条路径都不在落定时清除 secret**」；**`design.md:72` 原文未动** |
| RV2-DOCS F3 — `due_pairings` 对 `rejected` 无落点 | 合同 §4.3/§4.1 ↔ 实现 | **文档侧已闭环** | 合同 §4.1 `:132-137` 写明「任何到达 `expires_at` 的记录都清除（含终态）」「已批准但已过期也在返回集，其落库终态是切片 4/5 的开放项」；实现 `pairing.rs:327-341` 先清除后判终态，口径一致（实现结论归 rv3-auth） |
| RV2-WP2 F1 — 被拒绝配对 secret 无清除路径 | `crates/identity-auth/src/pairing.rs` | **记录侧与文档侧已闭环**（实现结论归 rv3-auth） | 合同 §4.3 `:193` 与 §4.1 `:125-126` 只剩一套口径；`rv3-mutation.log` mutation 3 显示退化后 3 条用例失败（来自既有日志） |
| RV2-WP2 F2 — `challenge_cache_is_bounded` 恒真 | `crates/identity-auth/tests/handshake.rs` | **记录侧已闭环**（实现结论归 rv3-auth） | 合同 §5.2 `:322-329` 已登记 `MAX_CHALLENGES = 1024` 与清扫/淘汰语义；`rv3-mutation.log` mutation 1/2 两分支退化后用例失败（来自既有日志） |
| RV2-WP3 G1 — R86 恒假死分支且记录声称已修 | `crates/identity-keystore/tests/keystore.rs` | **记录侧已闭环**（实现结论归 rv3-keystore） | `rv3-mutation.log` mutation 10「R86 替身不再模拟提交失败 → 用例必须失败」被捕获（来自既有日志）；`verification.md` 的映射（10 = R86 两阶段替身）与日志一致 |

---

## 1) 隔离与版本声明

```
task_id          : RV3-DOCS-2026-09-24
role             : reviewer（独立只读，未继承实现对话）
phase            : branch / recheck（RV3：RV2 修复轮后的 WP1+WP4 文档与规划资产一致性 + 记录真实性）
agent_context    : 新建子 Agent；输入仅为本轮 prompt 与项目上下文；未参与实现或实现讨论
target_revision  : 5ed869f（父 fe52694 = RV2 target；6861046 = RV1 target）
base_revision    : 任务给定 fe52694（**未读**该区间逐提交 diff，见下）
scope            : docs/IDENTITY_AND_AUTH_CONTRACT.md、docs/MODULE_ARCHITECTURE.md、docs/DEVELOPMENT_PLAN.md、
                   README.md、docs/adr/0008-*.md、.gitleaks.toml（陈述一致性）、
                   openspec/changes/identity-auth-and-keystore/{design,plan,tasks,verification}.md、
                   变更目录 reports/**（存在性、头部 revision、被引用文件）
checks           : 见 §5（PV1–PV5 的**证据文本**核对；本 Agent 未执行任何命令）
issues           : RV3-DOCS-1（P1）；RV3-DOCS-2..6（P2）
result           : FAIL（1 条 P1 未闭环；无 P0）
evidence_paths   : 本报告；openspec/changes/identity-auth-and-keystore/reports/rv3-*.log（既有日志，非我执行）
resource_cleanup : 无（未创建、未修改、未删除任何文件；未切分支、未提交、未并发）
```

- **隔离**：我是本轮新建的 reviewer 子 Agent。我只读了 `openspec/schemas/agentic/roles/reviewer.md`（全文）、项目 `AGENTS.md`（上下文注入）与本轮台账/代码/文档。我**不能**证明宿主未注入其它上下文，只能声明我所见范围。
- **版本**：`watchdog_diff`（stat）= `No working-tree changes against reviewer-launch HEAD 5ed869f453be` → 我读到的**文件内容对应 `5ed869f` 的内容**；检视期间未见变化。
- **限制（必须声明）**：(a) `watchdog_diff` **不含已提交区间**，我**没有读到 `fe52694..5ed869f` 的逐提交 diff**，因此凡「这条是本轮改的还是历史遗留」均不下结论，只报告当前文件内容的事实；(b) 本 Agent 的工具集只有 `read/grep/find/ls/watchdog_diff` 与 `contact_supervisor`，**没有 shell**，因此**没有执行** `node scripts/check-doc-links.mjs`、`git`、`cargo`、`npm run verify` 中的任何一条；凡引用「PASS / exit 0 / 用例数」处**一律来自既有日志文本**，退出码未由我复算。

## 2) 已读取范围

| 类别 | 实际读取 |
| --- | --- |
| 规则/合同 | `openspec/schemas/agentic/roles/reviewer.md`、`openspec/schemas/agentic/templates/verification.md`（节与 Check Plan Changes 字段）、`templates/plan.md`、`procedures/acceptance.md`（定向） |
| 权威文档 | `docs/IDENTITY_AND_AUTH_CONTRACT.md`（§1–§9 全读）、`docs/MODULE_ARCHITECTURE.md`（§3 状态行/§4.8/§4.12/§4.13/§5 与修订记录）、`docs/DEVELOPMENT_PLAN.md`（头部+§2）、`README.md`（crate 表、合同检查、分支保护/gitleaks 段）、`docs/adr/0008-ci-supply-chain-tooling.md`（定向）、`.gitleaks.toml`（全文）、`.husky/pre-commit`、`scripts/pre-commit.mjs`（定向） |
| 变更资产 | `design.md`（D3/D6 定向+关键段逐行）、`plan.md`（Coverage Index 全量标题行、Project Verify、Local Checks、Failure and Recovery、Completion Criteria）、`tasks.md`（2.x/3.x/6.x）、`verification.md`（**全文**） |
| 实现（仅为核对文档↔代码口径） | `crates/identity-auth/src/{lib,types,pairing,handshake,authority}.rs`、`crates/identity-keystore/src/{entry,store,ephemeral}.rs`、`crates/identity-keystore/tests/*`（清单） |
| 证据 | 变更目录 `reports/` **全量 `ls`**；逐条读取 `rv3-pv1.log`（头部/门禁行/EXIT）、`rv3-pv2.log`（全文）、`rv3-pv3.log`、`rv3-pv4.log`（用例计数）、`rv3-linux-clippy.log`（全文）、`rv3-selfcheck-rg.log`（全文）、`rv3-mutation.log`（全文）、`rv2-docs.md`（全文）、`rv2-auth.md`/`rv2-keystore.md`（发现段定向） |
| 未读 | `proposal.md`、`specs/*/spec.md` 正文（仅按 plan 行标题与 secret 生命周期段核对）、`crates/identity-auth/tests/*` 正文、`reports/rv1-wp*.md` 全文 |

## 3) 上一轮发现的逐条复核（编号对齐）

### 3.1 RV2-DOCS F1（P1）——**已闭环**
- 合同 §4.1（`docs/IDENTITY_AND_AUTH_CONTRACT.md:78-81`）新增 `[已废弃]` 块：说明 0.2 的「目标形状」草案（带载荷的 `PairingTarget`、`ClaimVerification`、`SettlementRequest`）**已被删除**，`PairingTarget` 是 `core::model` 的纯 token 枚举。全仓 `grep` 显示 `ClaimVerification`/`SettlementRequest` 只剩三处：该 `[已废弃]` 说明本身、以及历史 reviewer 报告（`reports/rv1-wp1.md`/`rv2-docs.md`/`rv2-auth.md`）——**没有活的规范块**。
- §4.1 里每个类型名逐条核到实现：`Authority`（`crates/identity-auth/src/authority.rs:21`）、`begin_pairing`（`pairing.rs:29`）、`verify_claim`（`:148`）、`settle`（`:272`）、`due_pairings`（`:327`）、`unrecoverable_after_restart`（`:346`）、`pairing_status`（`:360`）、`pairing_sas`（`:91`）、`ClaimedPairing::to_claim`（`:435`）、`ClaimOutcome::Repeat`（`types.rs:588/592`）、`PairingDecision`（`types.rs:450`）；`PairingDraft`/`PairingSpec`/`PairingStatusView`/`ClaimFields`/`ClaimedPairing`/`RequestedCapabilities`/`Sas` 由 `lib.rs:59-66` 转出，`PairingRecord`/`PairingClaim`/`PairingSettlement`/`PairingId`/`PairingState`/`Timestamp`/`ScopeSet`/`GrantSet`/`NodeKind` 由 `lib.rs:31-36` 如实转出。**无未落地名**。

### 3.2 RV2-DOCS F2（P1）——**未闭环，残留 1 处 → RV3-DOCS-1**
- 合同 §4.1（`docs/IDENTITY_AND_AUTH_CONTRACT.md:125`）已改为「**两条路径都不在落定时清除 secret**：批准后的配对还要支持状态查询的 HMAC 证明，直到「首次认证成功」才提前清除；拒绝的配对可为可靠轮询保留到原过期时间」，与 `:193`（§4.3）、`pairing.rs:265-296`（`settle` 不清除）、`pairing.rs:410-429`（`complete` 只清「已批准」）一致。
- **`openspec/changes/identity-auth-and-keystore/design.md:72` 仍是原文**：「落定：`settle(...)` → `PairingSettlement`（…）；拒绝/过期不产生信任，**两条路径都清除内存 secret**。」RV2 两处点名的就是它（`reports/rv2-docs.md` F2 的位置列、`reports/rv2-auth.md` F1 的「最小修复建议」都写了 `design.md:72`）。
- 口径唯一性核查（任务第 2 项）：`docs/SYNC_PROTOCOL.md:357`、`docs/NODE_LINK_PROTOCOL.md:820`、`specs/identity-pairing/spec.md:100`、合同 §4.3 `:193`、§4.1 `:125-137`、实现三处全部一致；**`design.md:72` 是唯一残留**。
- 与之相关的记录失真：`verification.md:80`（RV2-WP2 行）写「合同/**design**/tasks/error 文案同步」、`:82`（RV2-DOCS 行）写「settle/§4.3 措辞统一为…」——design 部分并未发生。

### 3.3 RV2-DOCS F3（P1）——**文档侧已闭环**
- 合同 §4.1 `:132-137` 明确：`due_pairings` 对**任何**到达 `expires_at` 的记录清除 secret（与是否终结无关，`rejected`/`expired` 也清除）；返回值只含未终结记录；`Approved` 不是终态，故「已批准但已过期」**在返回集里**，其落库终态（`approved_at` 是否保留）是**留给切片 4/5 的开放项**。
- 与实现一致：`pairing.rs:327-341` 先 `clear_secret` 再 `if !record.state().is_terminal() { due.push(...) }`。实现是否满足 spec 由 rv3-auth 判定；我只记录「合同↔实现口径已单一」。

### 3.4 F4（`reports/*` 存在性 + rv3 日志 revision 标注）——**存在性通过；标注有缺口 → RV3-DOCS-2**
- 机械核对 `verification.md`/`plan.md`/`tasks.md`/`docs/MODULE_ARCHITECTURE.md` 引用的每一个 `reports/*.log|*.md`：`w0-baseline-*`、`du1-pv1.log`、`wp1-boundaries.log`、`wp2-identity-auth-{transcripts,pairing,handshake,expansion,ports,full}.log`、`wp2-{boundaries,npm-check,workspace-tests}.log`、`wp3-{boundaries,identity-keystore,dpapi-verification,npm-check}.log`、`pv5-windows-dpapi.log`、`wp4-boundaries.log`、`bv-pv2..5.log`、`rv2-pv1..5.log`、`rv2-linux-clippy.log`、`rv2-selfcheck-rg.log`、`rv3-pv1..5.log`、`rv3-linux-clippy.log`、`rv3-mutation.log`、`rv3-selfcheck-rg.log`、`rv1-wp1..4.md`、`rv2-{auth,keystore,docs}.md` **全部存在**（RV2 指出的「写在仓库根」问题已修）。
- 例外两处，均为**尚未开始任务**的将来产物，不算缺陷：`plan.md:807` 的 [PV1] 证据列 `reports/du1-main-verify.log`（任务 6.7）、`plan.md:812` 的 RV1 证据列 `reports/rv1-du1.md`（任务 6.4）。`verification.md:31` 的 `bv-pv1.log` 是**显式的「不存在」说明**（正确）。
- `rv3-*.log` 头部 revision 标注（逐条实读）：`rv3-pv1.log:2`、`rv3-pv2.log:1`、`rv3-pv3.log:1`、`rv3-pv4.log:1`、`rv3-pv5.log:1`、`rv3-selfcheck-rg.log:2` 标注 `fe526941614dbd5fbe365802593c93a3cc49438a`（**父提交**，并注明「工作区有未提交改动」）；`rv3-linux-clippy.log` **没有任何 revision 行**（只有「附加：Linux 目标编译 + lint」）；`rv3-mutation.log:1` 只写「target revision = 修复后工作树」。**8 个 rv3 日志中没有一个写出本轮 target `5ed869f`**，且变更目录全境（含 `verification.md`）`grep "5ed869f"` **零命中**。

### 3.5 F5（README / ADR-0008 的 gitleaks 陈述）——**已闭环**
- `README.md:114-118`：「自研 keystore 条目**明文**形态的规则（`acpr-keystore-plaintext-entry`）已随 `identity-auth-and-keystore` 落地；它的**首次真实执行仍在 CI 的 `secrets` job**（本机无 gitleaks），且只覆盖「被 base64/base64url 编码后的明文条目」这一形态——原始二进制与 DPAPI 密文无法模式识别」；`:154-157` 补「触发条件已发生」「`keywords` 用编码前缀 `QUNQS`（不能写字面 `ACPK`…规则会永不评估）」。
- `docs/adr/0008-ci-supply-chain-tooling.md:107-114`（残余风险 3）同口径：规则已落地、`keywords` 前缀、边界、首次真实执行在 CI。
- 与 `.gitleaks.toml` 实际内容一致：`:42` `id = "acpr-keystore-plaintext-entry"`、`:47` `keywords = ["QUNQS"]`、regex `(?i)QUNQS[A-Za-z0-9+_-]{16,}={0,2}`、文件内「诚实的限制」注明只覆盖编码形态。另核 `README.md:114` 的「`.husky/pre-commit` 会调 gitleaks」：`.husky/pre-commit` → `scripts/pre-commit.mjs:77` `["gitleaks", ["git","--pre-commit","--redact","--staged"]]`、`:130` 缺二进制只提示 → **陈述属实**。

### 3.6 F6（verification.md 的 RV1 行收窄 + RV2 三行）——**已闭环（附 1 处残余见 §3.2）**
- RV1-WP1 行（`verification.md:76`）已改为「**部分修复（后续 RV2 复核纠正）**：F1/F3/F4/F5/F6 已修；**F2 只修了 §2**——§4.1 的 0.2 草案块…直到 RV2 才被删除」，不再声称「6 条全部修复」；与 `reports/rv2-docs.md` 的 Correct/F6 段**原文一致**。
- RV2 三行分别对应 `reports/rv2-auth.md`（F1/F2/F3 → P1；F4–F8 → P2）、`reports/rv2-keystore.md`（G1 → P1；G2–G9 → P2）、`reports/rv2-docs.md`（F1/F2/F3 → P1；F4–F11 → P2）。抽查：rv2-auth 的 `### F1/F2/F3` 标题与 RV2-WP2 行一致；rv2-keystore 的「1 项阻断项：G1」与 RV2-WP3 行一致；rv2-docs 的 P2 × 8 与 RV2-DOCS 行一致。**P2 列表是摘要形态**（我抽查未发现与原文矛盾，但未逐条逐号对账，见 §6）。
- 唯一残余：RV2-WP2 / RV2-DOCS 两行的 Resolution 声称 design 措辞已同步，而 `design.md:72` 未改（§3.2 / RV3-DOCS-1）。

### 3.7 F7（design.md 三处失效细节）——**两处已闭环，一处新残留 → RV3-DOCS-5**
- `design.md:68`：`IdentityAuthority` → **`Authority`** ✓（全仓 `grep IdentityAuthority` 零命中）。
- `design.md:76`：`pairing_sas(&self, pairing: &PairingRecord, peer: &ClaimedPairing)` ✓ 与 `pairing.rs:91` 一致（内含 `derive_sas` 说明）。
- `design.md:110`：magic **`ACPK`** + 头字段「格式版本 u16be、`purpose` u8、`label` 长度+标签、32 字节盐；私钥标量在被包裹段、头部不含私钥」✓ 与 `crates/identity-keystore/src/entry.rs:1-23`（magic/format/purpose/label len u16be/label/salt 32/wrapped scalar）、`:26-38`（`MAGIC = b"ACPK"`、`FORMAT_VERSION = 1`、`SALT_LEN = 32`）一致；条目路径 `<root>/<purpose>/<label>.<version>` ✓ 与 `store.rs:126-134` 一致。
- **新残留**：`design.md:69` 写「状态机入口（同步，除挑战签发需要经端口签名外均为纯计算）」，而 `pairing_sas` 是 `pub async fn`（`pairing.rs:91`）并在 `:107` 经端口 `node_public_key().await`（`authority.rs:61`）读公钥 → 该句对这条入口不成立。

### 3.8 F8（Check Plan Changes + plan 自检措辞）——**已闭环**
- `verification.md:41-47` 的 `## Check Plan Changes` 记录四项，均含原/新值与理由：① Linux 目标 clippy（`--target x86_64-unknown-linux-gnu --all-targets`，含「`plan.md` 原本没有这条」与「不链接、不执行」边界）；② mutation 检查（理由：RV2 的「恒真断言」风险；证据 `reports/rv3-mutation.log`）；③ 自检判据收窄（`plan.md` 已同步）；④ 测试位置调整（删 `tests/permissions.rs`，改为 `src/store.rs` 的 `mode_tests` + `unix_modes`）。
- `plan.md:801` 已同步为「（**可失败路径**零命中；已确认为不变的构造与测试内部断言不计入，命中处必须在 `verification.md` 逐条登记理由）」，与 verification 的叙述一致。

### 3.9 F9/F11（MODULE_ARCHITECTURE §4.12 三平台/证据/MSRV/归因 + 基线日期）——**已闭环**
- `docs/MODULE_ARCHITECTURE.md:408-410` 新增 `[现状]`（2026-09-24）：只有 Windows/DPAPI 一列已落地，macOS/Linux **尚未实现**（`platform/` 只有 `mod.rs`/`windows.rs`/`unsupported.rs`），非 Windows 走失败关闭桩，「以下三行是**设计意图**」；`:412-414` 分别标「**已落地**」/「（未实现，规划中）」。
- `:424` MSRV 改为「传递依赖声明的 MSRV **最高者是 `log 0.4.34` 的 1.71**」（RV2 的 F4 订正）；`:429-430` 两条证据均写**完整路径**并注明「这类日志受 `.gitignore` 忽略、不进版本库」；`:5` 修订记录归因改为「§3 状态行与 §3.1 的依赖口径记录两个身份 crate 已落地」+「§4.12 标注 macOS/Linux 后端未实现」。
- `docs/DEVELOPMENT_PLAN.md:4` 基线改为「2026-09-23（2026-09-24 随 `identity-auth-and-keystore` 补充切片 3 的落地状态）」。

### 3.10 F10（tasks 2.6/2.7 的 R 编号与入口名）——**已闭环**
- `tasks.md:15`：`[R30]–[R55]` + 入口名 `complete_auth`（与 `handshake.rs:305` 一致）；`tasks.md:16`：`[R56]–[R71]`。
- 与 `plan.md` Coverage Index 一致：`identity-pairing` R1–R29、`identity-handshake` **R30–R55**、`scope-expansion` **R56–R71**、`platform-keystore` R72–R90（逐段核到 `plan.md:443`(R55, handshake)/`:451`(R56, expansion)/`:566`(R71)/`:574`(R72)）。

### 3.11 记录真实性抽查（WP2/WP3 的 RV2 P1，仅核记录与证据文本）
- `verification.md:37` 的 RV2 修复轮 Check 行声明：identity-auth 78 用例（15 authorization + 19 handshake + 26 pairing + 2 parity + 9 ports + 5 transcripts + 2 doc-tests）、identity-keystore 33 用例（2 lib + 5 dpapi + 6 entry_format + 8 fail_closed + 12 keystore） → 与 `rv3-pv3.log`（`:10/:29/:31/:54/:56/:86/:88/:94/:96/:109/:111/:120/:128`）与 `rv3-pv4.log`（`:9/:20/:32/:46/:64`）**逐项吻合**（76+2=78；2+5+6+8+12=33）。
- `rv3-pv1.log:29/33/58/986`：`doc links OK: 369 relative links, 3589 section refs across 176 markdown files`、`crate boundaries OK: 10 个 crate`、`Totals: 9 passed, 0 failed`、`EXIT=0` → 与 Check 行「verify exit 0；10 crate 边界 OK」一致。
- `rv3-linux-clippy.log`：`EXIT=0`、无 warning（内容只有两条 `Checking` 与 `Finished`）→ 与「Linux 目标 clippy 无告警」一致。
- mutation：`rv3-mutation.log` 中 1/2（缓存上限与清扫）、3（`due_pairings` 终态清除）、4（`complete` 的「已批准」判定）、5/6（origin/endpoint 等价比对）、7（模式位）、8/9/9b（标签校验写/读路径）、10（R86 两阶段替身）**逐个出现 FAILED 且报错指向对应用例**，各段后有针对性的复绿 → 记录「10 个 mutation 全部被对应用例捕获」成立（编号细节见 §6）。
- 需区分：以上均为**记录与证据文本的一致性**，不是我对 WP2/WP3 实现正确性的判定。

## 4) 新发现

| ID | 级别 | 位置 | 事实与证据 | 影响 | 建议 |
| --- | --- | --- | --- | --- | --- |
| **RV3-DOCS-1** | **P1** | `openspec/changes/identity-auth-and-keystore/design.md:72` | 该行仍写「落定：`settle(...)` → `PairingSettlement`（…）；拒绝/过期不产生信任，**两条路径都清除内存 secret**」。对照：合同 §4.1 `:125`（已改为「两条路径都**不**在落定时清除」）、§4.3 `:193`、`specs/identity-pairing/spec.md:100`、`crates/identity-auth/src/pairing.rs:265-296`（`settle` 不触碰 secret）。RV2 的两份报告都点名了 `design.md:72`（`reports/rv2-docs.md` F2 位置列；`reports/rv2-auth.md` F1 最小修复建议），本轮只改了合同一侧 | `design.md` 的 D3 是切片 4–7 的接线输入（`tasks.md` 2.1 的完成条件就是「文档与 `design.md` D1–D7 一致」）。保留该句会重复 RV1-WP2 P2-1 触发过的误接线：调用方可能认为「落定后 secret 已清除」，从而自行持有 `PairingDraft.secret` 或错判 `consume_pairing` 语义。同时 `verification.md:80/:82` 已声称 design 文案「同步」，使记录高估了修复完成度 | 单行改为与合同同一口径：「拒绝/过期不产生信任；`settle` **不**清除 secret——批准后保留到首次认证成功，拒绝后保留到 `expires_at`（由 `due_pairings` 统一清除）」，并把 `verification.md:80/:82` 的「design 文案同步」改为「合同/design 措辞统一（design 见 `design.md:72`）」。级别说明：RV2 记为 P1；若主 Agent 按 RV1-WP1 对同类纯文档漂移的校准（P2）下调，本条不影响其余结论 |
| **RV3-DOCS-2** | P2 | `reports/rv3-*.log` 头部；`verification.md:37`；`Merge History`；`plan.md:807`/`tasks.md:56` 的 `reports/du1-pv1.log`/`du1-main-verify.log` | (a) 6/8 个 rv3 日志标注的是**父提交** `fe52694` 且注明「工作区有未提交改动」；`rv3-linux-clippy.log` 无 revision 行；`rv3-mutation.log` 只写「修复后工作树」。变更目录内 `5ed869f` **零命中**，`verification.md:37` 的 revision 列写「RV2 结束后的工作树（提交见 Merge History）」，而 `Merge History` 仍写「尚未开始集成」→ 本轮证据无法从工件内定位到 `5ed869f`。(b) 仓库根 `reports/` 仍在且存放**另一变更**的同名文件（`du1-pv1.log`、`du1-main-verify.log`、`rv1-wp1.md`、`rv1-wp4.md`），而变更记录对这四个名字仍用裸 `reports/…`；`docs/MODULE_ARCHITECTURE.md:429-430` 反而明确采用完整路径以避免同名混淆 | 归档/最终验收时无法仅凭 `verification.md` 复现「哪一轮在哪个提交上跑过」；同名文件让 `reports/rv1-wp1.md` 这类引用在「相对仓库根」读法下指向**另一变更**的报告 | 在 rv3 日志头部补目标提交（或在 `verification.md:37` 的 revision 列直接写 `5ed869f`）；对跨变更同名文件（`du1-pv1.log`/`du1-main-verify.log`/`rv1-wp1.md`/`rv1-wp4.md`）在记录里写完整路径或改名（RV2-DOCS F4 的建议只有「复制进变更目录」这一半被执行） |
| **RV3-DOCS-3** | P2 | `verification.md:38`（自检行）vs `reports/rv3-selfcheck-rg.log` | 该行写「identity-auth 1 处：`types.rs:256`；identity-keystore **3 处**：`entropy.rs:37-38`、`ephemeral.rs:50`」，但它自己引用的日志里是**15 行命中**：`types.rs:256`；`entropy.rs:37/38`；`ephemeral.rs:**53**`；`store.rs:487/503/511/513/517/518/527/546/549/550/552`（11 行，均在 `#[cfg(all(test, unix))] mod unix_modes`，`store.rs:470-487`）。同时该行 Command 列写 `grep -rn "unwrap()\|expect(\|panic!\|unreachable!\|from_der"`，而日志文件名/`plan.md:801` 用的是 `rg -n "from_der\|unwrap\(\|expect\("`（模式集合不同） | `plan.md:801` 要求「命中处必须在 `verification.md` 逐条登记理由」，而 11 处未登记；行号与日志不符，读者无法把记录与证据对上。「可失败路径零命中」的**结论**不受影响（11 处都在测试模块内），但「登记」义务未满足 | 把该行改为与日志一致：列出 15 处（或明确「14 处集中在测试代码：`entropy.rs:37-38`、`ephemeral.rs:53`、`store.rs` 的 `unix_modes` 内 11 处」），并把命令写成实际执行的 `rg` 模式 |
| **RV3-DOCS-4** | P2 | `verification.md:18` | 该 Check 行仍用旧编号：「配对状态机（R1–R29）、握手（**R30–R52**）、授权展开（**R53–R68**）、端口与秘密类型（**R69–R71**）」。按 `plan.md` 的 Coverage Index：握手 = R30–R55（`plan.md:443`）、展开 = R56–R71（`:451`–`:566`）、keystore = R72–R90（`:574`+）；R69–R71 是 `scope-expansion` 的场景，不是「端口与秘密类型」 | 与 RV2-DOCS F10 同类（任务级声明与索引错位），最终验收若按 R 编号逐行关联会错配 | 改为「配对 R1–R29、握手 R30–R55、展开 R56–R71、端口与秘密类型按 `plan.md`（R72–R90 属 keystore）」并把该行的 scope 列与索引对齐 |
| **RV3-DOCS-5** | P2 | `openspec/changes/identity-auth-and-keystore/design.md:69` | 该行写「状态机入口（同步，除挑战签发需要经端口签名外均为纯计算）」，但同一列表里的 `pairing_sas` 是 `pub async fn`（`crates/identity-auth/src/pairing.rs:91`）且 `:107` 经端口读公钥（`authority.rs:61 pub async fn node_public_key`）；合同 §4.1 已把该入口写成 `pub async fn pairing_sas` | 与 RV2-DOCS F7 同一节（D3）的残余：切片 4–7 按 design 读会以为配对入口全部同步、不需 await 上下文 | 改为「入口除挑战签发与 SAS 派生（需经端口读本节点公钥）外均为纯计算」 |
| **RV3-DOCS-6** | P2 | `docs/IDENTITY_AND_AUTH_CONTRACT.md:294-296`（§5.1 登记段）vs `crates/identity-auth/src/{pairing,authority}.rs` | §5.1 自述「诊断/测试入口属于公开 API 的一部分：`challenge_cache_len()`、`pairing_status`/`failure_count`/`has_secret` 与常量 `MAX_CHALLENGES`…新增同类入口时在本节登记，避免公开面静默膨胀」。实际公开面多出：`Authority::complete`（`pairing.rs:413`，与 §5.1 登记的 `complete_auth` 是同一行为的两个公开名，且 `pairing.rs:410-412` 与 `handshake.rs:304` 的 rustdoc 互相指认对方为「唯一副作用入口」）、`Authority::reset_memory`（`authority.rs:144`，返回计数并清除全部内存 secret，只在启动路径与测试使用，未见于任何合同小节）、`Authority::node_public_key`（`authority.rs:61`，async、经端口） | 公开 API 面比合同登记多 3 项，§5.1 的「不静默膨胀」规则未被执行；`complete`/`complete_auth` 双名会让切片 4–7 的接线与审计用词不确定 | 二选一：在 §5.1 补登 `complete`（注明是 `complete_auth` 的实现名）与 `reset_memory`（启动语义、返回计数、不清除持久材料）；或把实现收敛为单一公开名。属文档/接口面收口，可由 WP2 与 WP1 在同一改动里决定 |

## 5) plan.md 覆盖索引与 Check ID 核对

**Coverage Index（机械核对）**

| 段 | R 行 | 索引计数 | spec 计数（`plan.md` 自述） | 结论 |
| --- | --- | --- | --- | --- |
| `identity-pairing` | R1–R29（`plan.md:25`–`:242`） | 29 | 7 Requirement + 22 Scenario = 29 | 连续、无缺行 |
| `identity-handshake` | R30–R55（`:250`–`:443`） | 26 | 7 + 19 = 26 | 连续；段边界与 `tasks.md` 2.6（R30–R55）一致 |
| `scope-expansion` | R56–R71（`:451`–`:566`） | 16 | 5 + 11 = 16 | 连续；与 `tasks.md` 2.7（R56–R71）一致 |
| `platform-keystore` | R72–R90（`:574`–`:713`） | 19 | 5 + 14 = 19 | 连续 |
| 合计 | R1–R90，grep `^  - id: R` = **90 行** | 90 | — | 无缺行、无重复 |

- 抽查行的 `tasks`/`checks`/`evidence` 列：`tasks` 引用的 `2.1`/`2.4–2.8`/`2.10`/`2.12–2.14` 均存在于 `tasks.md`；`checks` 只引用 PV3/PV4/PV5（均存在于 Project Verify 表）；`evidence` 引用的 `reports/wp2-identity-auth-*.log`、`wp3-identity-keystore.log`、`pv5-windows-dpapi.log` 均在变更 `reports/` 存在。
- **WP4 没有 R 行**（与 WP4 的 Verification 列 = [PV1]/[PV2] 一致）→ WP4 只能靠「文档 ↔ 实际元数据」的静态一致性验收，本轮 RV3-DOCS-1/2/3/4/5/6 正是这条一致性上的失败点。

**Check ID（plan.md 的 Project Verify）**

| Check ID | 定义 | 本轮核对（仅证据文本） |
| --- | --- | --- |
| PV1 `npm run verify` | `plan.md:807` | `rv3-pv1.log` 全文可读（十道门禁 + `check:rust`），`EXIT=0`、`doc links OK: 369/3589`、`crate boundaries OK: 10` —— **PASS 来自既有日志，非我执行** |
| PV2 `check-crate-boundaries` | `:808` | `rv3-pv2.log` = `crate boundaries OK: 10 个 crate …` + `EXIT=0` |
| PV3 `cargo test -p identity-auth` | `:809` | `rv3-pv3.log` 7 个测试二进制 76 用例 + 2 doctest，全 `ok`、无 filtered-out 归零疑点 |
| PV4 `cargo test -p identity-keystore` | `:810` | `rv3-pv4.log` 5 个二进制 33 用例，全 `ok`（Windows）；Linux 侧只有编译/lint |
| PV5 Windows DPAPI | `:811` | `rv3-pv5.log` 存在（5 用例）；本轮**未**由我复算 |
| RV1 独立 review | `:812` | `reports/rv1-wp1..4.md` 均存在；`rv1-du1.md` 属 6.4，未开始（正常） |
| 计划外新增判定 | — | Linux 目标 clippy、mutation、`with_availability` 注入缝、自检判据收窄、测试位置调整 —— 均在 `verification.md` 的 `## Check Plan Changes` 登记 ✓（未回填 plan 的 Local Checks/Project Verify，按模板不强制） |
| E2E | mode = `not-applicable` | 无 TP/E2E 用例（第 4 组写「不适用」）；`Main E2E` 记录了降级批准来源（用户原话「1和2都同意」）与替代清单；7.1/7.3 未执行（任务未开始） |
| CI-only | `deps`/`advisories`/`secrets` | 本地无等价物 → 本轮**不**声称通过；我也没有执行任何等价检查 |

**未覆盖的 Check ID**：无遗漏（PV1–PV5 均有 Check 行）；本轮新增判定均有 Check Plan Changes 条目。

## 6) 未覆盖 / 无法确认项与所需证据

1. **我未执行任何命令**（无 shell/git/cargo/node）：`node scripts/check-doc-links.mjs`、`npm run check`、`npm run verify`、`cargo …` 的「PASS / exit 0」全部来自**既有日志文本**（`rv3-pv1.log` 等），退出码未由我复算。**需要**：由项目 verify 执行者在固定版本上复跑并留证（尤其确认 `doc links`（369/3589）在 `5ed869f` 上仍绿）。
2. **`fe52694..5ed869f` 的提交区间 diff 未读**（工具限制）。因此 RV3-DOCS-1 的「design.md:72 是本轮未改还是历史遗留」我**不归因**，只报当前内容事实。**需要**：`git diff fe52694..5ed869f -- docs openspec/changes/identity-auth-and-keystore README.md .gitleaks.toml`。
3. **未评价 WP2/WP3 实现正确性**（我只为核对文档口径读了 `settle`/`due_pairings`/`complete`/`pairing_sas`/`entry` 相关片段）：RV2-WP2 F1/F2 与 RV2-WP3 G1 的**实现侧**结论由 `rv3-auth`/`rv3-keystore` 给出；我只确认「记录与既有日志一致」。
4. **Linux 运行时未验证**：本机无 WSL/Docker，也无法链接 Linux 二进制；Linux 侧只有「编译 + lint + `#[cfg(unix)]` 单测可编译」。**需要**：CI Linux runner 的实际运行结果（`store::unix_modes`、失败关闭路径）。
5. **CI-only 三项未验证**：`deps`/`advisories`/`secrets`（含本轮新增 `getrandom`、`windows-dpapi` 的许可证/来源/advisory 与自研规则的**首次真实执行**）——本地无等价物，任何「通过」声明都不成立。**需要**：CI job 结果。
6. **低置信观察（未计为发现）**：(a) `## Check Plan Changes` 的四条目未按 `openspec/schemas/agentic/templates/verification.md:30` 列「受影响任务」与「独立 review 引用」（内容与理由已具备，仅字段不全）；(b) `crates/identity-auth/src/authority.rs:142` 的 rustdoc 链接指向不存在的 `crate::pairing::AuthorityPairingExt`（跨车道，供 `rv3-auth` 判定；`cargo doc -D warnings` 才会暴露，本仓库不是门禁）；(c) `rv3-mutation.log` 有 11 个标签段（含 `9b`），记录写「10 个 mutation」——我按「9b 是 9 的子情形」理解，未计为差异；(d) mutation `9b` 恢复后只列出 `entry_format` 一个二进制的复绿，全量复绿由 `rv3-pv1/pv3/pv4` 的整轮日志补足。
7. **`verification.md` 的 RV3 行**（`:83`）仍是「待 RV3 结果填入」；我按只读边界**未**写入任何文件——本报告应作为该行与 `reviewFindings` 的输入由主 Agent 落盘。
8. **修复后的复核要求**：RV3-DOCS-1（P1）修复后需由**新的**独立 reviewer 按原问题 ID 复核；RV3-DOCS-2..6 为只读报告项，不影响本轮对实现的判断。
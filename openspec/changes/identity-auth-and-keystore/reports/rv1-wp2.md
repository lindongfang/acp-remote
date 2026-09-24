<!-- 落盘说明：主 Agent 原样保留 reviewer 的正文与结论；仅当某处 `§` 引用会触发文档引用门禁的歧义归属时，补全被引文档名（不改变语义）。 -->
<!-- 由独立 reviewer 子 Agent 产出（未继承实现对话，只读），主 Agent 原样落盘。 -->
<!-- review run: 1a30da02-58a2-40f4-b160-c90a576c9cbf；target revision: 6861046（RV1 轮次）；落盘时间：2026-09-24 -->

# WP2 独立代码检视报告

```
task_id:  RV1-WP2-2026-09-24
role:     reviewer（独立检视子 Agent，本轮只读，未修改任何文件）
phase:    branch（WP2 交付前独立检视；W4 阶段入口）
agent_context: 新建子 Agent，未继承实现对话；仅收到本任务文本 + 仓库；无法自证宿主未注入其它上下文
target_revision: 6861046（工具报告工作区 HEAD = 686104647e49，工作区干净）
scope:    crates/identity-auth/**（src 与 tests 全部）
changes:  无（本轮不写入任何文件）
checks:   以 reports/ 下既有日志为依据逐 ID 核对；本轮不执行任何命令
issues:   无 CRITICAL / MAJOR；5 条非阻断发现 + 3 条 report-only 说明
result:   PASS
evidence_paths: reports/wp2-identity-auth-{full,transcripts,pairing,handshake,expansion,ports}.log、wp2-boundaries.log、wp2-npm-check.log、du1-pv1.log、bv-pv{2,3,4,5}.log、verification.md
resource_cleanup: 本轮未创建任何资源（无测试、无临时目录、无 worktree）
```

## 结论

**PASS**（无 CRITICAL/MAJOR，无阻断项）。已按 `roles/reviewer.md` 的 Procedure 与任务书 9 项重点核对完成 WP2 静态检视；所有阻断标准（依赖方向、秘密不落可观察位置、无 `from_der`、一次性消费先于验签、无跨 `await` 持锁、无平台 `cfg`、展开表与 `commands.json` 逐项一致）**全部满足**。发现 5 条非阻断项（P2），其中 P2-1（R54 正向路径无用例）需要主 Agent 做一次判断：`plan.md` 的阻断标准写着「任一规格场景缺覆盖」即阻断，若按字面执行，它是唯一可能被判阻断的条目；我按缺陷影响判为 MINOR（代码本身经我逐行确认正确，仅缺用例）。

## 1) 版本与隔离声明

- **隔离**：本 Agent 为新建子 Agent，未继承实现对话，未参与 `identity-auth` 的实现或实现讨论；只读过任务书列出的固定输入、仓库文件与 `openspec/changes/identity-auth-and-keystore/` 下的计划/任务/验证/日志。我只能声明**自己收到的输入与限制**，不能证明宿主未注入其它上下文（`fork_turns="none"` 或等效设置由调度者记录）。
- **版本**：`watchdog_diff` 报告"工作区相对 reviewer-launch HEAD `686104647e49` 无任何变更"，即当前工作区文件内容 = 目标修订 6861046 的内容，因此我读到的源码/测试即目标版本内容。
- **重要限制（如实声明）**：`watchdog_diff` **不包含已提交区间的 diff**，我因此**没有**读到 `30f0d78..6861046` 的完整 diff，也无法独立验证 6861046 是否为 `feat/identity-auth-and-keystore` 的当前 HEAD、5c9d2c4/627bb8e 的逐提交视图、或"变更只动了 `crates/identity-auth` + `members` + 文档"这一范围声明。本轮实质是**目标修订内容 + 契约的逐行检视**，不等价于 commit-range diff 检视。
- **只读**：未修改代码/测试/计划/任务状态/`verification.md`，未切分支、提交、合并、修复；未执行任何构建或测试命令（因此下文所有"通过"均来自既有日志，不是我复算的结果）。按 reviewer 角色规则，review-only/no-edit 优先于 progress 记录，我未写 `progress.md`。

## 2) 已读取的 diff 范围与文件

**契约/需求**：`openspec/changes/identity-auth-and-keystore/{proposal 未单读, design.md(全文), plan.md(全文), tasks.md(全文), verification.md(全文)}`、`specs/{identity-pairing,identity-handshake,scope-expansion}/spec.md`（WP2 相关三份全文）、`compatibility/transcripts/v1/transcripts.json`（全文 12 domain）、`compatibility/commands/v1/commands.json`（全文）、`docs/IDENTITY_AND_AUTH_CONTRACT.md`（§2–§9）、`docs/MODULE_ARCHITECTURE.md`（§3/§3.1/§4.8/§4.11/§4.12/§5/§11 相关段）、`docs/SYNC_PROTOCOL.md`（§6.2/§11/§14/§15 相关段）、`Cargo.toml`（members 与 workspace deps）、`crates/identity-auth/Cargo.toml`、`scripts/check-contract-assets.mjs`（nul-joined 与 fixedWidth 口径）。

**源码（全读）**：`crates/identity-auth/src/{lib,types,transcript,pairing,handshake,authority,authorization,port,state,error}.rs`。
**测试（全读）**：`crates/identity-auth/tests/{transcripts,pairing,handshake,authorization,ports}.rs` 与 `tests/support/mod.rs`。
**对照物**：`crates/sync-protocol/src/pairing.rs`、`crates/node-link-protocol/src/pairing.rs`、`crates/core/src/model/scalars.rs`（`Timestamp` 校验口径）、`fixtures/sync/v1/transcripts/host-challenge.json`。
**证据日志**：`reports/wp2-identity-auth-full.log`、`wp2-identity-auth-transcripts.log`、`wp2-npm-check.log`、`wp2-boundaries.log`、`wp2-workspace-tests.log`、`du1-pv1.log`（前 140 行）、`bv-pv3.log`（前 40 行），并列出 `reports/` 全目录。

## 3) 逐条发现

### 无阻断项

以下重点项**已核对且满足**（结论先给，证据附后）：

| 重点 | 结论 | 证据 |
|---|---|---|
| (a) transcript 只从协议表取域/tag | 实质满足（表是唯一权威、装配失败关闭），字面有两处如实说明见 P2-6 | `src/transcript.rs:165-204`（`domain_spec` 查表 + 字段数/tag 顺序/定长宽度逐条断言）；`src/transcript.rs:408/450/489/526/562/597/633/673/710/746/782/817` |
| (b) 验签公钥只来自 `PeerTrust` 快照 | 满足，且入口类型不携带公钥 | `src/handshake.rs:52`（`ProofSubmission` 无公钥字段）、`src/handshake.rs:192`（唯一取公钥处 = `trust.public_key`）；配对侧用内存 secret：`src/pairing.rs:210-241` |
| (c) 无 `from_der`/DER、只收 64 字节 P1363、拒绝零值、接受 high-S | 满足 | `grep from_der` 于 `src` 零命中；`src/types.rs:164-176`（构造即 64 字节断言）、`src/transcript.rs:341-357`（先 `components_nonzero` 再解析点/签名）；`tests/handshake.rs:216`（high-S 接受）、`tests/handshake.rs:252`（零值拒绝）、`tests/ports.rs:95-107`（71/33 字节被拒） |
| (d) 一次性消费先于验签 | 满足（比 design D4 更严：D4 是"③绑定后、④验签前消费"，实现是"消费最先") | `src/handshake.rs:169-176`（`take_challenge` 在 kind/nonce/过期/绑定/验签之前）；`tests/handshake.rs:265`（重放 → `UnknownChallenge`） |
| (e) 展开表与 `commands.json` 逐项一致、`local.*` 拒绝、grant/pack 不可互换 | 满足（我逐项比对 4 张表 + 7 项本地能力） | `src/authorization.rs:16-74` ↔ `compatibility/commands/v1/commands.json`（pack 4/4、preset 2/2、grant 5/5、localCapabilities 7/7 全等）；`src/authorization.rs:186-191`（`local.` 前缀整体保留）；`tests/authorization.rs:300-315` |
| (f) 不跨 `await` 持锁 | 满足（9 处 `.await` 全在锁外，与 verification 自检行一致） | `src/pairing.rs:107`（`material` 临时守卫在 `let` 语句末释放，`node_public_key().await` 在其后）、`src/handshake.rs:109/127`（签名先于 `let mut state = self.state()`，`src/handshake.rs:147` 显式 `drop(state)`）、`src/authority.rs:65/93/103/114/125/136` |
| (g) 平台 `cfg` 零命中 | 满足 | `src` 内 `cfg(windows)|cfg(unix)|cfg(target_os` 零命中（我复核）；`tests/transcripts.rs:480-484` 的协议表引用在测试内 |
| (h) 用例能失败 | 无明显恒真阻断项，两处弱断言见 P2-8 | 12 向量逐字节 + 摘要 + HMAC + 验签 + SAS 全为真值比对（`tests/transcripts.rs:396-470`），并含"换一把 secret/公钥必须失败"的反向断言；实现内**无**任何 fixture 期望值（`grep 1789650000|bdb2ec20|...` 仅命中 `tests/support/mod.rs`） |
| (i) 秘密不泄漏 | 满足 | `src/types.rs:262-275`（`PairingSecret` Debug redacted）、`src/port.rs:73-100`（`SecretBytes` 无 Debug/Clone + 2 个 `compile_fail` 文档测试，日志实测 2 passed）、`src/port.rs:177-190`（`KeystoreError` 无载荷）、`src/authority.rs:154-160`（`Authority` Debug 只打 `local_node`） |

### P2-1（MINOR，建议本轮或下一次 RV 前补）：R54「认证成功后才消费配对」的正向路径没有任何用例

- 位置：`crates/identity-auth/tests/handshake.rs:458-476`（`completion_reports_consumption_target_once`）；实现分支 `crates/identity-auth/src/pairing.rs:399-414`（`complete` 的 `filter` → `clear_secret` → `Some(pairing_id)`）。
- 触发条件：唯一断言是"内存里没有该配对的 secret"（传入的是 challenge id `CONNECTION`，从无 secret），因此 `consume_pairing` 为 `Some` 的分支**从未被执行**。
- 预期/实际：规格 `specs/identity-handshake/spec.md` 的 R54 场景要求"收尾写集包含该配对转已消费与审计"；实际无用例观察 `Some`。
- 影响：该分支若被改坏（例如误把谓词取反、或忘 `cloned()`），不会有任何用例失败；影响面限于"配对停在 `approved` 而不进 `consumed`"这一记账状态（secret 仍会清除，`is_unconfirmed` 不含 approved，故不影响认证与重启终结），因此不是功能/安全缺陷。但 `plan.md` 的 Code Review「阻断标准」写着"任一规格场景缺覆盖"即阻断——若按字面执行，本条就是唯一候选阻断项。
- 修复建议（最小）：在 `tests/pairing.rs` 里先用 `create_device_pairing()` 造出有 secret 的配对，调用 `authority.complete(fact, Some(&pairing(PAIRING)), &ts(CREATED))`，断言 `consume_pairing == Some(pairing(PAIRING))` 且 `!authority.has_secret(...)`，并再调一次断言 `None`（幂等/非首次）。约 10 行。
- 严重度：**MINOR**（P2；非阻断）。请主 Agent 决定"补用例"还是"记录经批准的豁免"。

### P2-2（MINOR）：已定型的契约签名与实现形状不一致（文档 ↔ 代码漂移）

- 位置与对照（左 = `docs/IDENTITY_AND_AUTH_CONTRACT.md` 的 2026-09-24 定型块，右 = 实现）：
  - §4.1 `begin_pairing(...) -> Result<(PairingDraft, PairingWrite), _>` / `PairingDraft{target, display_name, requested, expires_at, secret}` ↔ `src/pairing.rs:29`（返回 `PairingDraft{record, secret, server_nonce, pairing_request_id}`，`src/types.rs:437-448`）；`expires_at` 是入参但 `created_at` 也是入参。
  - §4.1 `verify_claim(&PairingRecord, ClaimFields) -> Result<(PairingClaim, PairingClaimWrite), _>` ↔ `src/pairing.rs:146`（多一个 `existing: Option<&ClaimedPairing>`，返回 `ClaimOutcome`；`PairingClaim` 要经 `to_claim()`，`src/pairing.rs:420`）。
  - §4.1 `settle(pairing, peer: Option<&PairingPeer>, decision, at) -> Result<PairingSettlementWrite, _>` ↔ `src/pairing.rs:264`（无 `peer` 入参，返回 `PairingSettlement`）。
  - §4.1 `expire`/`recover_after_restart -> Vec<*Write>` ↔ `src/pairing.rs:315/333`（`due_pairings`/`unrecoverable_after_restart`，返回 `Vec<PairingId>`）。
  - §4.1 `pairing_sas(transcript: SasTranscript, secret) -> Sas` ↔ `src/pairing.rs:89`（`pairing_sas(&PairingRecord, &ClaimedPairing)`，自己装配 transcript——这一点与 §5.1「transcript 由 `identity-auth` 自己编码」一致，反而更收敛）。
  - §5.1 `ChallengeRequest.supported_features: Vec<String>` ↔ `src/handshake.rs:22-37`（`negotiated_features: FeatureList` + `catalog_revision: Option<u64>`）；`HandshakeCompletion{fact, at}` ↔ `src/types.rs:784-791`（`Completion{fact, at, consume_pairing}`）。
  - §5.1 `PeerTrust{peer, public_key, credential, scopes, grants}` ↔ `src/types.rs:646-662`（多 `host_binding`、`node_kind`，两者被 `src/handshake.rs:271/287/240` 实际使用；没有它们 `IdentityFact::Node.kind` 无法从声明的输入产出）。
  - §2（`docs/IDENTITY_AND_AUTH_CONTRACT.md:47`）仍列举 `HandshakeCompletion`/`ClaimVerification`/`SettlementRequest`，实现导出的是 `Completion`/`ClaimOutcome`（`src/lib.rs:40-66`）。
- 预期/实际：WP1 的任务 2.1 明确要求"把目标形状**定型为已实现形状**"，且 design D9 把该文档列为合同同步面；实际定型文本停在设计草案的签名上，WP2 的实现没有再回写。
- 影响：切片 4–7 的 adapter/组合根若照文档写会编译不过（可发现、非静默），但"合同 = 唯一权威"这一条在本次变更内不自洽；不影响运行期行为与安全。
- 修复建议（最小）：由 WP4/文档所有者把 §4.1/§5.1/§2 的代码块改成实现形状（并在版本注记里写明"以实现为准定型"），或反向把实现改回文档形状——二者取一，不要两边都留。属文档/契约维护范围，不是 WP2 代码缺陷。
- 严重度：**MINOR**（P2）。

### P2-3（MINOR）：design D8 的「值对象等价比对语料」未落地，且注释指向不存在的测试文件

- 位置：`crates/identity-auth/src/types.rs:32`（"…等价比对由测试语料保证，**见 tests/parity.rs**"）、`src/types.rs:40`、`src/types.rs:73`。
- 触发条件：`crates/identity-auth/tests/` 无 `parity.rs`；全 crate 无对 `sync_protocol::pairing::CanonicalOrigin` / `node_link_protocol::pairing::Endpoint` 的等价比对（grep 仅命中 `src/transcript.rs:10-11` 的 DOMAINS 与 `tests/transcripts.rs:480-484` 的表查找）。
- 预期/实际：design D8 与 Risks#9 明确要求"用同一份接受/拒绝语料做等价比对（同一输入两边必须同为接受或同为拒绝）"，语料含路径/查询/fragment、非 `https`/`wss`、大写主机、端口、IPv6、空串、超长串、尾随点。实际无此测试。
- 影响：当前两份实现**逐条相同**（我逐行比对 `src/types.rs:41-68` ↔ `crates/sync-protocol/src/pairing.rs:34-48`；`src/types.rs:74-93` ↔ `crates/node-link-protocol/src/pairing.rs:40-62`，仅错误类型不同），所以今天没有行为差异；缺的是 D8 承诺的**漂移防线**，未来单边改动不会被发现。
- 修复建议（最小）：新增 `tests/parity.rs`，把一组正/负语料对两边 `parse` 的接受-拒绝结果做等值断言（约 30 行）；或删除这三处注释并按变更流程登记 design D8 的调整。
- 严重度：**MINOR**（P2）。

### P2-4（MINOR）：计划自检项「`from_der|unwrap\(|expect\(` 正常路径零命中」未留证且与事实不符

- 位置：`openspec/changes/identity-auth-and-keystore/plan.md`（「Verification Strategy → Local Checks」的"审查重点自检"行）；`verification.md` 的「自检 / WP2 / 任务 2.9」行只记录了 `cfg` 与 `.await` 两条；代码侧 `crates/identity-auth/src/types.rs:256`（`Digest::new(...).expect("SHA-256 的 base64url 文本必然是规范 Digest 形状")`）。
- 触发条件：按计划原样执行该 grep，命中 1 处（`src` 内 `.expect(` 唯一一处；`from_der` 与 `.unwrap(` 零命中）。
- 预期/实际：计划的判据是"正常路径零命中"；实际有 1 处（`PairingSecret::digest`），且该自检未写入 `verification.md`。
- 影响：`AGENTS.md` §7 禁止的是"无说明的 panic"，此处带说明且可证明不可达（32 字节 SHA-256 的 base64url 恒为 43 字符合法 `Digest`），不构成缺陷；问题是**证据与计划不完全对应**（一条计划自检被静默略去，且略去的恰是不满足的那条）。
- 修复建议（最小）：在 `verification.md` 如实登记该命令输出（"1 处命中 + 不可达理由"），或把该 `expect` 改成返回 `Result`；不要保留"计划写了、证据里没有"的状态。
- 严重度：**MINOR**（P2）。属证据/流程问题，交主 Agent 与 WP2 实现者处理。

### P2-5（MINOR，加固建议）：`PeerTrust.peer` 在 crate 内从未被读取，`is_usable()` 是无调用的公开 API 且语义与规格相反

- 位置：`src/types.rs:646-662`（`peer` 字段）、`src/types.rs:678-687`（`is_usable`）；`src/handshake.rs:70/192/236-246/271/287` 只用 `public_key`/`credential`/`scopes`/`grants`/`node_kind`/`host_binding`。
- 触发条件：`hello` 不比对 `trust.peer` 与 `request.peer`；`verify_proof_inner` 不比对 `trust.peer` 与 `record.peer`/`submission.peer`，而事实主体取自 `submission.peer`、公钥与 scope/grant 取自 `trust`。
- 预期/实际：规格没有要求该一致性断言（我逐条读过 `identity-handshake` 的 7 条需求，均未涉及快照身份），所以这不是规格违规；但同一次调用里"身份来自 A、权限与公钥来自 B"可以被成功认证，只在调用方传错快照（拼变量、缓存串号）时可达。
- 影响：需要 adapter 侧 bug 才可利用；无本地错误、无日志，属于静默的主体/权限错配窗口。`is_usable()`（"撤销态不可用于验签"）与 `identity-handshake` 的要求相反（撤销设备仍须返回 `Authenticated + Revoked`），无调用方，容易被后续切片误用。
- 修复建议（最小）：在 `hello`/`verify_proof_inner` 开头加 3 行 `if trust.peer != request.peer { return Err(HandshakeError::TrustKind) }`（对现有 66 个用例零影响——所有用例的快照 peer 都与请求 peer 一致，我已逐个确认），并删除或改名 `is_usable()`；或明确在契约里写明"由 adapter 保证快照与请求同一对端"。
- 严重度：**MINOR**（P2，报告项；当前无规格要求，不建议据此判 FAIL）。

### P2-6 ~ P2-8（report-only，均为如实说明，无需修复）

- **P2-6 transcript 字面常量**：12 个 `domain_input!` 调用写死了 domain 字符串与 field tag 字面量（`src/transcript.rs:408`、`450`、`489`、`526`、`562`、`597`、`633`、`673`、`710`、`746`、`782`、`817`），`Nonce::field_bytes` 另写死 32（`src/transcript.rs:67-77`）。与 design D2「domain 字符串与 field tag 不写死在 identity-auth」在**字面**上不完全一致；但装配会逐条对表（`src/transcript.rs:165-204`：domain 查表、字段数量、tag 位置、定长宽度，任一不符即 `UnknownDomain`/`FieldCount`/`FieldOrder`/`FieldWidth`），且 tag 位错位必然被 `FieldOrder` 拦住（同域内 tag 唯一），12 向量 + 20 负例覆盖。**实质要求满足**（无手拼字节、无第二份表）。建议在注释里点明"字面量只作查表键，权威在协议表"，或在 design 层收口措辞。
- **P2-7 挑战缓存无上限、无过期清理**：`src/state.rs:60-96`（`put_challenge` 只插入；`take_challenge` 才移除）；`clear_challenges` 只在启动语义 `src/authority.rs:139` 调用。触发：完成 `hello` 却不发 `proof` 的连接反复重连 → `challenges` 单调增长（每条含 2 个 nonce + features + binding）。参考 `AGENTS.md` §5「所有输入都有长度、数量、频率和资源限制」；`SYNC_PROTOCOL.md` §14 只规定认证超时 15 秒、未规定缓存上限，连接级限流归后续 `server::transport`。影响：未经认证的连接可缓慢耗尽内存（单条百字节量级，需要大量连接）。最小修复：在签发/消费时用注入时钟顺带清扫 `expires_at` 已过的条目，或给 map 设上限并在满时拒绝签发。
- **P2-8 两处措辞/断言强度**：(a) `src/transcript.rs:145-148` 注释称 negotiatedFeatures 按"UTF-16 code unit 序，即 Rust 的 `String` 序"排序——Rust `String` 的 `Ord` 是 UTF-8 字节序，二者只在 BMP 之外不同；featrue ID 被 `[a-z0-9.-]` 约束（`SYNC_PROTOCOL.md:1448`），当前无实际差异；参考实现 `scripts/check-contract-assets.mjs:324-334` 用 JS 默认排序并**要求输入已排序**。(b) `tests/ports.rs:52-66` 的 `assert!(!text.contains("secret-value"))` 对无载荷的 `KeystoreError` 是恒真断言（结构性不可能失败）；真正的防线是 `SecretBytes` 无 `Debug`（`src/port.rs:89-100` 的 `compile_fail`）与 `PairingSecret` 的 redacted `Debug`。另：`tests/transcripts.rs:420-430` 的 `verify_hmac`/`verify_signature` 返回 `Option` 并用 `if let Some(...)` 消费，若域分派被改错会**静默跳过**而仍通过（12 向量的字节断言不受影响）；建议加一句"expected 里有 `hmacSha256` 时必须返回 `Some`"的断言。

## 4) plan.md 覆盖索引（WP2 行）与 Check ID 核对结论

### R 编号（R1–R71，任务 2.4–2.9）

- **R1–R29（配对，`tasks 2.4/2.5`）**：逐行可指到 `tests/pairing.rs` 的 20 个用例；R22（批准后提前清除）由 `approve_requires_pending_and_within_requested` 断言 `has_secret == false`；R21/R20 由 `restart_terminates_unconfirmed_pairings` + `approved_pairing_is_not_terminated_after_restart` 覆盖；R25/R26 由 `fifth_failure_invalidates_pairing` + `all_rejections_share_one_public_class` 覆盖。**两处为"部分覆盖"并如实登记**：R28（撤销后普通写入不能复活）在本 crate 只能表现为"已固定对端不被改写"（`claim_never_rebinds_existing_peer`），真正的撤销写入路径属 `storage-sqlite`/core 用例；R11 的"重启后可读"部分由"公钥逐字节一致 + 可构造 core `PairingClaim`"表达（状态机不读存储，符合 D4）。
- **R30–R55（握手，`tasks 2.4/2.6`）**：17 个用例覆盖绝大多数场景；**缺** R54 的正向断言（P2-1）；R36（长度先于密码学）由类型构造级用例覆盖（`tests/ports.rs:95-107` 的 71/33 字节、`tests/transcripts.rs:509+` 的填充/非字母表 base64url），非握手级；R45（授权结果不随时间缓存）无专门"随快照缩减再认证"用例，仅由"每次调用取当次快照、无缓存"的代码结构与 `revoked_and_scope_reduced_are_not_authorization_denials` 间接体现。
- **R56–R71（展开，`tasks 2.7`）**：15 个用例覆盖；R58（重复输入去重）靠 `ScopeSet` 集合语义 + preset 递归用例表达，没有"同一 pack 与其成员同时请求"的专门用例（低风险，集合类型天然去重）。
- **编号不一致（报告项）**：`tasks.md` 把展开场景编号为 R53–R68、把端口/秘密类型编号为 R69–R71，`tests/*.rs` 的文件头注释沿用了同一套编号，而 `plan.md` 的 Coverage Index 把展开场景编号为 R56–R71、把端口/秘密类型归到平台 keystore 的 R81–R84（checks `[PV3, PV4]`）。同一含义的 `R67/R68` 在两份文件里指不同场景。**影响**：不影响按 Check ID + 日志路径建立的证据链，但最终验收若按 R 编号逐行关联会产生错配，建议主 Agent 在 5.2 统一（以 `plan.md` 的 Coverage Index 为准，或在 verification 里注明两套编号的对应关系）。
- **R72–R90（platform-keystore）不在本轮范围**（属 WP3，我未检视 `crates/identity-keystore`）。

### Check ID

| Check ID | 计划判据 | 我的核对 | 是否影响本轮判断 |
|---|---|---|---|
| PV3 | `cargo test --locked -p identity-auth --all-features`，12 向量重算/SAS/畸形拒绝/失败路径全过、无 0 用例、无全跳过 | 日志齐备：`wp2-identity-auth-full.log`（lib 单测 0 + authorization 15 + handshake 17 + pairing 20 + ports 9 + transcripts 5 = 66 通过、0 failed、0 ignored + 2 `compile_fail` doc-tests）、5 个分项日志、`bv-pv3.log`（候选轮次）；`npm run check` 的 `check:assets` 另行独立复算 "12 transcript vectors re-encoded from input, 20 negative vectors rejected, 2 SAS values recomputed"（`wp2-npm-check.log`） | 否（不影响）；我未复算，日志未标注 revision（见 §5） |
| PV1 | `npm run verify` 全绿、无零用例/全跳过 | `du1-pv1.log`（十道门禁 + `cargo fmt/clippy --workspace/test --workspace` 三条在一个 `check:rust` 里，含 identity-auth/identity-keystore 编译与 67 个测试二进制） | 否 |
| PV2 | `check-crate-boundaries.mjs`：`identity-auth` 行只依赖 core/两协议/acpr-transcript、`acpr-wire` 格为空 | `wp2-boundaries.log`（9 crate）、`wp4-boundaries.log`/`bv-pv2.log`（10 crate）均 OK；我另核对 `crates/identity-auth/Cargo.toml` 与 `MODULE_ARCHITECTURE.md:456` 行逐格一致 | 否 |
| PV4 / PV5 | keystore 全量 / Windows DPAPI | 只确认日志存在（`wp3-identity-keystore.log`、`pv5-windows-dpapi.log`、`bv-pv4/bv-pv5`）；**未检视** WP3 代码，结论以 RV1-WP3 为准 | 否（越界，不评价） |
| RV1 | 独立 reviewer 报告完整、无未解决阻断项 | 本报告即 WP2 的 RV1 输入；`reports/rv1-wp2.md` 待主 Agent 落盘（我不写文件） | 是（本轮结论以此报告为准） |

**证据一致性问题（P2，报告项）**：`verification.md` 的 3.1 行把"最终以 `reports/bv-pv1.log`/`du1-pv1.log` 为准"写成已存在证据，但 `reports/` 目录里**没有** `bv-pv1.log`（只有 `bv-pv2/3/4/5`）；PV1 行同时引用了 `du1-main-verify.log`（6.7 才产生，属计划项，可接受）。另：`verification.md` 的「PV3（全量）」行声称"clippy/fmt 无告警"，但被引用的 `wp2-identity-auth-full.log` 只含 `cargo test` 输出（92+ 行，末尾是 doc-tests），不含 clippy/fmt 段——该结论实际由 `du1-pv1.log` 的 workspace 级 `check:rust` 支撑。建议改引 `du1-pv1.log` 或补跑留证。

## 5) 未覆盖 / 无法确认项与所需证据

1. **无法读取 base→target 的 committed diff**（`watchdog_diff` 只覆盖工作区增量，且报告工作区相对 HEAD 干净）。因此：变更范围（"只动 `crates/identity-auth` + members + 文档"）我**未**逐文件核对；`5c9d2c4`/`627bb8e` 的逐提交视图未读；也无法证明 6861046 就是分支 HEAD。**所需**：主 Agent 在 5.2/6.3 提供 `git log --oneline 30f0d78..6861046` 与非 `crates/identity-auth/**` 的 `--stat`（尤其 `Cargo.toml`、`docs/**`、`.gitleaks.toml`），或为 reviewer 提供固定 worktree。
2. **日志无 revision 标注**：`reports/*.log` 均为命令原始输出，不含 commit hash / `git status` 头（`wp2-*.log`、`du1-pv1.log`、`bv-*.log` 我都抽查过）。因此"证据来自 6861046"只能由 `verification.md` 的文字声明支撑，日志本身不可自证。**所需**：候选轮次（6.3）重跑时在日志头部记录 `git rev-parse HEAD` 与 `git status --porcelain`，或补一份 revision 旁证。
3. **我没有执行任何命令**：PV1/PV2/PV3 的通过以日志为准，未独立复算；`cargo clippy/fmt` 我未运行。若主 Agent 需要"reviewer 之外的独立一轮"，应在 6.3 的候选轮次执行（属 Project Verify，不属本轮）。
4. **WP3（`identity-keystore`）与 `platform-keystore` 的 R72–R90 未检视**：`plan.md` 把 PV4/PV5 与 R81–R84（秘密不进 Debug/日志/错误）绑定到 WP3；我只在 `crates/identity-auth` 侧确认了 `SecretBytes`/`PairingSecret`/`KeyHandle` 的防护（P2-8 提及 `KeystoreError` 断言的强弱）。WP3 侧结论请以 RV1-WP3 为准。
5. **未确认**：`proposal.md` 的 `agentic-intent` 与用户两条批准原文（任务书未列为必读，我只读了 design/plan/tasks/verification 与三份 specs）；`docs/SECURITY_DESIGN.md` 的 §9.2/§13.1/§13.2/§14.1/§14.2/§20 我**未逐节通读**（只在 plan/design/契约的引用段落侧读），因此"WP2 侧与这些节的一致性"只覆盖了被引用的部分（秘密不落可观察位置、失败关闭、无平台 cfg、DPAPI 与 WP2 无关）。
6. **不影响本轮判断但需在 6.3/7.1 补齐**：`bv-pv1.log` 缺失（P2 证据引用）；`rv1-wp1.md`/`rv1-wp3.md`/`rv1-wp4.md`/`rv1-du1.md` 尚未生成（属后续任务，正常）；`du1-main-verify.log` 待 6.7。

## 汇总

- 结论：**PASS**（target revision 6861046；6 项重点核对全部满足；无 CRITICAL/MAJOR，无阻断项）。
- 待主 Agent 决策/处理：P2-1（R54 正向用例缺失，是否按 plan 的"缺覆盖即阻断"口径要求补用例或记录豁免）、P2-2（契约 §2/§4.1/§5.1 定型块与实现不一致的收口方向）、P2-4（`verification.md` 的自检行与证据引用订正）、以及 §4 末尾的 `bv-pv1.log`/clippy 证据引用问题。
- 任何修复落地后，按 `roles/reviewer.md` 需由**新的**隔离子 Agent 用新 Review ID 复核受影响结论；本轮报告与结论供 5.2/6.4/8.1 核对。
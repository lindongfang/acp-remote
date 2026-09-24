<!-- 落盘说明：主 Agent 原样保留 reviewer 的正文与结论；仅当某处 `§` 引用会触发文档引用门禁的歧义归属时，补全被引文档名（不改变语义）。 -->
<!-- 由独立 reviewer 子 Agent 产出（RV3 轮，target revision 5ed869f），主 Agent 原样落盘。 -->
<!-- review run: rv3-auth（见 RV3 工作流 b80022df-27b1-49e1-8695-face55f9a776）；落盘时间：2026-09-24 -->
All source and evidence inspected. Writing the final review.

## 结论

**FAIL（阻断项 1 条：P1，纯文档闭环；`crates/identity-auth` 的代码与用例侧我未发现 P0/P1）**

- RV2-WP2 的三条 P1 **实质上全部已修**（F1 终态 secret 清除、F2 恒真缓存用例、F3 证据日志落点），且修复**可被证伪**（`reports/rv3-mutation.log` 的 mutation 1–4 分别击穿旧行为）；RV2-WP2 的 P2 与 RV1 遗留 P2-3（`tests/parity.rs`）也基本闭环。
- 唯一未闭环项是 **RV2-DOCS F2 / RV2-WP2 F4(a) 的 `design.md` 半**：`design.md:72` 仍写「两条路径都清除内存 secret」（与合同 §4.1:123、§4.3、实现相反），而 `verification.md` 的 RV2 行声称「合同/design/tasks/error 文案同步」。一行文档 + 一条记录订正即可，但它是上一轮已判 P1 的同一句，且正是 RV1 轮把 `settle` 写错的那条口径，留在变更设计文档里会继续误导切片 4/5 → 本轮按 P1 记（详见 F1）。
- 若主 Agent 把该行归入 `rv3-docs` 工作流并由其修复，则本轮 **WP2 的代码/用例判定为 OK with notes**。

与上一轮 P1 的逐条对照：

| 上一轮 ID | 结论 | 依据 |
|---|---|---|
| RV2-WP2 F1（rejected 等终态 secret 无清除路径） | **已修** | `pairing.rs:327-344` 先 `clear_secret` 再判终态；`tests/pairing.rs:644-649`（Rejected 记录）、`799-803`（Approved 记录）、`844-848`、`898-902` 断言；mutation 3（恢复旧行为）使 3 条用例失败 |
| RV2-WP2 F2（`challenge_cache_is_bounded` 恒真） | **已修** | 熵源改为按调用序号播种（`tests/support/mod.rs` 的 `SequenceEntropy::fill`），三条断言改 `assert_eq!`；mutation 1（去淘汰）得 1025≠1024、mutation 2（去清扫）得 1024≠1 |
| RV2-WP2 F3（本轮日志缺失） | **已修** | `reports/rv3-{pv1..pv5,linux-clippy,selfcheck-rg,mutation}.log` 均在变更目录（残留：日志头只标 base revision） |
| RV2-WP2 F8（`complete_auth` 用「有 secret」代理「已批准」） | **已修** | `PairingMaterial.approved` + `take_approved_secret`（`state.rs:52/81-101`）；`tests/pairing.rs:733-777`；mutation 4 使其失败 |
| RV1-WP2 P2-3（`tests/parity.rs` 缺） | **已修** | `tests/parity.rs` 两条等价比对用例；mutation 5/6 可失败 |
| RV2-DOCS F2 的 `design.md` 半 | **未修** | `design.md:72` 原文未动 → 本轮 F1 |

---

## 1) 隔离与版本声明

- **隔离**：本轮为新建只读 reviewer 子 Agent，未继承实现对话，未参与 `identity-auth` 的实现或讨论；**未修改任何文件**（代码、测试、`tasks.md`/`verification.md`/`progress.md` 均未动），未提交、未合并、未修复、未写盘。
- **版本**：`watchdog_diff` 报告「No working-tree changes against reviewer-launch HEAD `5ed869f453be`」→ **我读到的文件内容 = target revision 5ed869f 的内容**。我不能证明 5ed869f 就是 `feat/identity-auth-and-keystore` 的 HEAD（该结论来自调度者声明）。
- **限制（如实声明）**：`watchdog_diff` **不含已提交区间**，我**没有**读 `fe52694..5ed869f` 的逐提交 diff，也没有复核「本轮只改了 X 项」这类范围声明。本轮实质是**目标修订内容 + 契约/规格逐行检视**，不等于 commit-range diff 检视。
- **未执行任何命令**（无 cargo/npm 可用）：下文所有「通过/全绿」均**来自既有日志**（`reports/rv3-*.log`），凡我未亲手复核者均单独注明；我自做的静态核对（grep 级）已在下文逐条标明「我自行 grep 复核」。
- **证据来源限制**：`rv3-*.log` 头部一致标注 `revision fe526941614dbd5fbe365802593c93a3cc49438a`（= RV2 轮 target）并注明（`rv3-pv1.log:2`）「工作区有未提交改动，见 git status」。即**日志绑定的是 base 提交 + 脏工作区，不能按提交哈希绑定到 5ed869f**。旁证：`rv3-pv3.log` 的用例数（authorization 15 / handshake 19 / pairing 26 / parity 2 / ports 9 / transcripts 5 + 2 doc-tests = 78）与我逐文件清点的 `#[test]` 数（26/19/2/…）逐一吻合，且包含只可能存在于修复后树中的 `unapproved_pairings_are_never_reported_as_consumption_targets` 与 `parity.rs`。

## 2) 已读取范围

- **代码（全读）**：`crates/identity-auth/src/{pairing,handshake,state,authority,types,error,lib}.rs`；`crates/identity-auth/Cargo.toml`。
- **测试（全读相关段）**：`tests/{pairing,handshake,parity,transcripts}.rs`；`tests/support/mod.rs`。
- **契约/资产**：`docs/IDENTITY_AND_AUTH_CONTRACT.md` §1–§9（重点 §2、§4.1、§4.3、§5.1、§5.2、§7）；`openspec/changes/identity-auth-and-keystore/{design,plan,verification,tasks}.md`；`specs/identity-pairing/spec.md`（R20 段）、`specs/identity-handshake/spec.md`（挑战/重放段）；`docs/CORE_PORTS_AND_STORAGE.md` §11.3/§11.6/§7.3（配对保留与 `expire_pairings`）；`crates/core/src/model/identity.rs`（`PairingState::is_terminal`）。
- **对照实现**：`crates/sync-protocol/src/pairing.rs`、`crates/node-link-protocol`（`Endpoint`，经 parity 用例与 mutation 6 交叉验证）。
- **证据日志（只读、引用时标注「来自既有日志」）**：`rv3-pv1.log`（`npm run verify`，EXIT=0；doc links 369/3589；boundaries 10 crate；check 9 items；identity-auth 78 用例 0 failed）、`rv3-pv2.log`、`rv3-pv3.log`（PV3，EXIT=0）、`rv3-pv4.log`、`rv3-pv5.log`、`rv3-linux-clippy.log`、`rv3-selfcheck-rg.log`、`rv3-mutation.log`；历史：`rv1-wp2.md`、`rv2-auth.md`、`rv2-docs.md`。
- **未读**：`crates/identity-keystore/**`（WP3）、`docs/MODULE_ARCHITECTURE.md`/`README.md`/`AGENTS.md`/`DEVELOPMENT_PLAN.md`（WP4）、`.gitleaks.toml`。

## 3) 上一轮发现的逐条复核（编号对齐）

**RV2-WP2 F1（P1）：终态记录的 secret 清除路径**
- **实现**：`pairing.rs:327-344` 已改为「先 `self.state().clear_secret(record.id())`，再 `if !record.state().is_terminal() { due.push(...) }`」；返回值仍只含未终结记录。`Rejected` 是终态（`core/src/model/identity.rs:418-420`）因此只清 secret、不进返回集——与合同 §4.1:126-129 的措辞逐字一致。
- **逐路径核对**：①`settle(Reject)` 后 → secret 保留到 `expires_at`，由 `due_pairings` 清除（用例 `rejected_secret_is_cleared_at_expiry_too`，`tests/pairing.rs:809-851`，用真正的 `PairingState::Rejected` 记录断言）；②`consume` 之后 → `take_approved_secret` 直接 `remove`（`state.rs:92-101`），且第二次 `complete_auth` 返回 `None`（`tests/pairing.rs:686-691`）；③`unrecoverable_after_restart` 之后 → 只处理 `created/pending_confirmation`，但该函数是启动路径、`reset_memory` 已清空全部（`authority.rs:144-150`），无残留；④第 5 次失败 `invalidate` 后 → secret 仍在，但记录未过期，到 `expires_at` 由 `due_pairings` 清除。**未发现仍然永不清除的状态**。
- **「以已落定记录断言」**：新用例使用了落定态记录（`tests/pairing.rs:795` 的 `approved_record`、`:844` 的 Rejected 记录、`:640-649`），不再是 `Created` 记录冒充；但两处**旧**调用点仍传 `created.draft.record`（`:414-417`、`:468-471`）→ 见本轮 F4。
- **证伪证据**：mutation 3 恢复旧过滤行为后 `rejection_survives_until_original_expiry` / `rejected_secret_is_cleared_at_expiry_too` / `expiry_scan_touches_every_expired_record_regardless_of_terminal_state` 三条失败（来自既有 `rv3-mutation.log`）。**F1 判定：已修。**

**RV2-WP2 F2（P1，测试）：`challenge_cache_is_bounded` 恒真**
- **前提是否成立（我独立验算）**：`SequenceEntropy::fill` 现在以**调用序号**为种子（`tests/support/mod.rs:206-243`：`seed = *calls; *calls = seed.wrapping_add(1); state = seed.wrapping_mul(0x9E3779B97F4A7C15)` 后逐字节旋转/乘加，取 `state >> 33`）。每次 `hello` 恰好 2 次 `fill`（`handshake.rs:80` 的 32 字节 nonce、`:83` 的 16 字节 `challenge_id`），`ChallengeId::generate` 消费 16 字节（`types.rs:131-136`）；两处乘数均为奇数 → 种子到状态是单射，故 1088 次 hello 的 1088 个 `connectionId` 互异（仅 16 字节投影偶然相撞的概率可忽略）。`request()`/`client_nonce()` 为常量、不消耗熵（`tests/handshake.rs:73-87`）。→ **前提成立**。
- **三条断言均可失败**：①填满后 `assert_eq!(len, MAX_CHALLENGES)`（前提正是「id 互异」，不互异则恒 ≤ 某小数）；②再签发一条仍等于 1024（断言去淘汰即为 1025）；③推进时钟到 `AFTER_TTL` 后等于 1（断言去清扫即为 1024 → 实际会先淘汰再插入成 1025）。`rv3-mutation.log` 的 mutation 1/2 输出（`left: 1025 / right: 1024`、`left: 1024 / right: 1`）**同时反证了断言①在 1024 处成立**，即互异前提在实际执行中成立。**F2 判定：已修**；残余不足见本轮 F3（未覆盖「淘汰最早过期」的选择语义）。

**RV2-WP2 F8（P2→本轮必需核对）：`PairingMaterial.approved` / `take_approved_secret`**
- 形状：`state.rs:43-53` 新增 `approved: bool`；`:81-88` `mark_approved`；`:92-101` `take_approved_secret`（仅 `approved==true` 时 `remove` 并返回 `true`）；`pairing.rs:413-431` `complete` 用 `pairing.filter(|id| self.state().take_approved_secret(id))`。
- **绕过面核对**：①`settle(Approve)` 只在 `state == PendingConfirmation && !expired && granted.within(requested) && matches_target` 之后调 `mark_approved`（`pairing.rs:273-305`），同一 id 二次 `settle(Approve)` 因状态检查返回 `WrongState`；②`mark_approved` 唯一调用点就是这里（我自行 grep 复核，全 crate 仅 `state.rs:81` 定义、`pairing.rs:303` 调用）；③`created/pending/rejected` 记录即使仍持有 secret 也得 `None`（用例 ①②③ 覆盖，`tests/pairing.rs:733-777`）；④`reset_memory` 后全部 material 被清 → 一切配对均为 `None`（重启语义，与「secret 只在内存」一致）；⑤未知 id → `None` 且不清除任何东西。
- **新增用例可失败**：mutation 4 把判定换回「有 secret」→ `unapproved_pairings_are_never_reported_as_consumption_targets` 失败（`left: Some(...) / right: None`）。**F8 判定：已修**；置位时机引入的新窗口见本轮 F2（P2）。

**RV1-WP2 P2-3：`tests/parity.rs` 等价比对**
- **实现**：两条用例分别把 `identity_auth::CanonicalOrigin::parse` / `NodeEndpoint::parse` 与 `sync_protocol::pairing::CanonicalOrigin::parse` / `node_link_protocol::pairing::Endpoint::parse` 在**同一语料**上比对 `is_ok()`，并在接受时比对保留文本逐字相等。
- **语料覆盖（对照 design D8 列表）**：路径/查询/fragment ✓（origin 侧 3 条）、非 `https`/`wss` 方案 ✓（`http://…`、`wss://…` 放在 origin 位、`https://…` 放在 endpoint 位）、大写主机 ✓（origin 侧）、端口 ✓（origin + endpoint）、IPv6 字面量 ✓（两侧）、空串 ✓（两侧）、超长串 ✓（两侧各一条 `>2048` 的构造）、尾随点 ✓（origin 侧）。**结论：覆盖 D8 列表**（唯一不对称：endpoint 语料没有大写主机项，属可接受的轻微不对称，不构成缺陷）。
- **可失败**：mutation 5（`identity-auth` 的 origin 放宽为接受路径）/ mutation 6（endpoint 放宽为接受 `https`）都使 parity 用例失败并打印两侧分歧（来自既有 `rv3-mutation.log`）。**P2-3 判定：已修。**
- 附带：`src/types.rs:32/40/73` 的「见 tests/parity.rs」注释现在有真实落点 ✓（不再是悬空引用）。

**RV2-WP2 F3（P1，证据落点）**：`reports/` 下 `rv3-*` 7 个日志齐全、内容与源码用例清单吻合（78 用例 / pairing 26 / parity 2），且 `rv3-pv1.log` 的 `EXIT=0`、`Totals: 9 passed, 0 failed (9 items)`、doc links 369/3589 可读。**判定：已修**；残余为「日志锚定 base revision + 脏工作区」的 provenance 限制（见 §1）。

**RV2-WP2 F4（P2，四条）**：
- (a) 合同 §4.1:123 已改为「**两条路径都不在落定时清除 secret**」✓；`design.md:72` 未改 ✗ → 本轮 **F1（P1）**。
- (b) §5.2:341-346 已登记 `MAX_CHALLENGES = 1024`、清扫、淘汰最早过期、被淘汰者统一失败 ✓，与 `state.rs:22/149-165` 逐条一致。
- (c) §5.1:291-293 已登记「快照主体 == 提交主体」检查 ✓；`error.rs:102` 文案已改为「对端没有可用的持久化信任材料，或信任快照不属于本次提交的主体」✓。
- (d) `challenge_cache_len()` 与 `MAX_CHALLENGES` 已在 §5.1:294-297 登记 ✓；但 `Authority::complete` 与 `complete_auth` 双入口、`sign_*`×4、`reset_memory`、`now`、`local_node`、`node_public_key` 仍未登记 → 本轮 **F6（P2）**。

**RV2-WP2 F5（P2）**：合同 §4.1:78-82 已用 `[已废弃]` 注记取代 0.2 草案块，`ClaimVerification`/`SettlementRequest` 只出现在版本注记与历史说明里（我 grep 复核）✓。

**RV2-WP2 F6（P2，用例精度）**：`tests/pairing.rs:695-732` 已改名 `approved_pairing_keeps_its_material_until_first_auth` 并在注释里只主张两条可断言性质（材料仍在、首次认证会识别为消费目标）；到期清除用例改用 `approved_record(&pending)` ✓。残余（未计入新发现）：`pairing.rs:265` 的 `settle` 注释仍以「`pairing-status` 之类的 HMAC 证明」作为保留 secret 的理由，而 `Authority` 并不提供「用内存 secret 校验状态证明」的入口（该证明由调用方持 `PairingDraft.secret` 自行校验）——纯措辞不精确，无行为影响。

**RV2-WP2 F7（P2，跨 crate）**：已在合同 §4.1:126-129 登记为开放项（「已批准但已过期」的落库终态由存储侧决定，`approved_at` 是否保留留给切片 4/5）✓。

**RV2-WP2 未列但 RV2-DOCS 列出的 P1**：DOCS-F1（合同 §4.1 草案块）✓ 已删；DOCS-F2 的合同半 ✓、`design.md` 半 ✗（→ F1）；DOCS-F3（`due_pairings` 对 rejected 无落点）✓ 已由 F1 的修复闭环。

**回归面（任务书第 5 项，我自行 grep 复核）**：`cfg(windows)|cfg(unix)|cfg(target_os` 零命中；`from_der` 零命中；`unwrap()/panic!/unreachable!` 零命中；`expect(` 仅 `types.rs:256`（`Digest` 构造不变式，不可失败路径，与 `rv3-selfcheck-rg.log` 一致）；`src` 内 9 处 `.await` 全在锁外（`hello` 先签名后 `let mut state = …; … drop(state);`，`pairing_sas` 的 `state()` 临时 guard 在同一条 `let` 语句结束时释放，`sign_*`/`node_public_key` 不取锁）；`tests/transcripts.rs:339-408` 的固定向量断言仍是精确断言（HMAC 域要求 `Some(true)` 且换 secret 必须 `Some(false)`；SAS 域逐字比对 + 与 `Sas::from_hmac_output` 一致；签名域换公钥必须 `Some(false)`），`registry_and_fixture_domains_agree` 仍断言 12 个 domain 全覆盖，无静默跳过。

## 4) 新发现

### RV3-WP2-F1（P1）`design.md` 仍写「落定时两条路径都清除内存 secret」，且记录声称已同步

- **位置**：`openspec/changes/identity-auth-and-keystore/design.md:72`（D3 的 `settle` 行）；引用方/对照：`docs/IDENTITY_AND_AUTH_CONTRACT.md:123`、`:213`（§4.3）、`crates/identity-auth/src/pairing.rs:263-270`、`specs/identity-pairing/spec.md:100`、`verification.md:80`（RV2-WP2 的 Resolution 写「合同/design/tasks/error 文案同步」）。
- **事实**：`design.md:72` 原文仍为「拒绝/过期不产生信任，**两条路径都清除内存 secret**」。同一变更的合同 §4.1:123 已改为「两条路径都**不**在落定时清除 secret」，实现 `settle` 确实不碰 secret（`pairing.rs:272-313` 无 `clear_secret`），规格 R20 也只要求「批准的在首次认证时提前清除、拒绝的可保留到原过期时间」。即 **design 与（合同 + 规格 + 实现）三方相反**。
- **影响**：design D3 是 WP2/切片 4–5 实现与评审的直接依据。RV1 轮正是因为照「落定即清除」写 `settle` 才产生必须回滚的真实行为缺陷（`verification.md:113` 自述）；同一句留在设计文档里，会把同一错误再引一次。对端不可见、无运行时影响，但属上一轮已判 P1 的同一句未闭环，且 `verification.md` 的「已同步」记录与事实不符。
- **最小修复建议**：把 `design.md:72` 改为与合同 §4.1:123 同口径（「落定不清除；首次认证成功提前清除，上界为 `expires_at`，由 `due_pairings` 统一清除——含已终结的 `rejected`」），并在 `verification.md` 的 RV2 行把「design 文案同步」改为准确表述（如「合同/design 的 settle 行随本轮修正」）。（该行也落在 `rv3-docs` 范围，若由其修复可复用本证据。）

### RV3-WP2-F2（P2）`mark_approved` 在 `settle` 内、持久化提交之前置位，可短暂制造 design D3 明文禁止的状态

- **位置**：`crates/identity-auth/src/pairing.rs:303`（`self.state().mark_approved(pairing.id());`，返回值被丢弃）、`state.rs:81-88`；对照 `design.md` D3 的「内存态只允许比已提交状态更严格……**绝不出现「内存里批准、库里没有信任」**」与合同 §4.1:123。
- **事实/触发**：`settle(Approve)` 先置位内存 `approved`，再把 `PairingSettlement::Approved` 交给调用方提交。若调用方事务失败（存储不可用/冲突），内存为「已批准」而库仍为 `pending_confirmation`——即 design D3 用词直接禁止的组合。此后该对端若已持有旧的持久信任并通过握手，`complete_auth` 会返回 `Some(pairing_id)`，要求把仍是 `pending_confirmation` 的行推进为 `consumed`（core 的 `PairingRecord::try_new` 要求 `Consumed ⇒ approved_at`，会被存储拒绝）；按合同 §5.1:255-258，consume 与 `last_seen`/审计同一事务，故该次认证的收尾写入整体失败（记账/连接生命周期受影响，**不产生信任、不放宽授权**）。另 `mark_approved` 返回 `false`（material 缺失，例如 `due_pairings`/`reset_memory` 已清除）被忽略，使「已批准」判定与材料存在脱钩且无任何信号。
- **影响**：仅在「settle 提交失败」这一窄窗口可达，无安全影响；但它使一个新引入的内存标志与既有不变式的文字表述冲突，且失败方向是「多要求一次落库」，属应由合同明确取舍的接线契约。
- **最小修复建议**：二选一并在同一改动里回写合同 §4.1 ——（a）把置位挪到提交成功之后（新增 `Authority::mark_pairing_approved(&PairingId)` 供组合根在事务提交成功后调用，`settle` 只返回待提交的领域值）；或（b）保留现状但在 §4.1/design D3 明确「该标志先于提交置位；提交失败时调用方必须按 §11.6 重试或调用 `reset`/重新装配，且不得据 `consume_pairing` 推进非 `approved` 行」，同时让 `settle` 在 `mark_approved` 返回 `false` 时给出显式信号（而非静默）。

### RV3-WP2-F3（P2，用例强度）缓存用例未覆盖「淘汰最早过期条目」的选择语义

- **位置**：`crates/identity-auth/tests/handshake.rs:625-656`；实现 `state.rs:149-165`（`len() >= MAX_CHALLENGES` 时 `min_by(expires_at)` 淘汰再插入）；合同 §5.2:341-346 声称该语义。
- **事实**：用例的 1088 次 `hello` 全在同一个 `FakeClock` 时刻（`setup()` 后不推进），因此**所有条目 `expires_at` 相同**；三条断言只断言「长度 == 1024 / 长度不增长 / 越过 TTL 后为 1」。若实现改成淘汰**最新**（或任意）条目，断言全部仍然通过——「淘汰最早过期」这一被合同明文登记的语义目前无任何用例保护。
- **影响**：非阻断。淘汰策略退化（例如淘汰刚签发的清单里最可能被使用的条目）不会被现有 78 条用例发现。
- **最小修复建议**：新增一条用例（约 15 行）：填满后每 N 次 `hello` 推进 `FakeClock` 1 秒，使 `expires_at` 互异，再签发一条并断言「最早过期者被淘汰、最新一条仍在缓存」（可用 `take_challenge` 对新/旧 id 的可用性做可观察断言）。

### RV3-WP2-F4（P2，用例精度）两处旧断言仍以 `Created` 记录叙述「已落定记录」的清除性质

- **位置**：`crates/identity-auth/tests/pairing.rs:414-421`（`approve_requires_pending_and_within_requested`）、`:468-475`（`reject_produces_no_trust`）。
- **事实**：两处把 `created.draft.record`（状态 `Created`）传给 `due_pairings`，而内存材料此刻分别是「已批准」/「已拒绝」。因为 `due_pairings` 按 id 清除、与记录状态无关，当前断言成立；但若将来把清除条件改成「按记录自身状态判定」，这两处仍会通过——即它们不保护自己叙述的「落定态 clear」性质。
- **影响**：非阻断（真正的性质已由 `:640-649`、`:799-803`、`:844-848` 的新用例以 Rejected/Approved 记录覆盖）。保留旧写法会让人误以为覆盖重复。
- **最小修复建议**：两处改传对应落定态记录（`approved_record(&pending)` / `rejected_record(...)`），或删掉这两处 redundant 断言并保留新用例。

### RV3-WP2-F5（P2，装配契约）`due_pairings` 的「硬上界」依赖调用方能拿到仍在库的记录，而终态配对行的清理归属未写明

- **位置**：`crates/identity-auth/src/pairing.rs:327-344`（入口签名要求调用方传入记录列表）；`docs/CORE_PORTS_AND_STORAGE.md:1354`（「pairing 终态记录至少保留到原 `expires_at`，随后可清理」）、`:1409`（`expire_pairings` 只终结「未确认且 `expires_at <= at`」的行）。
- **事实**：终态行（如 `rejected`）不会进入 `due_pairings` 的返回值、也不被 `expire_pairings` 重写，因此其删除只能来自保留/容量清理。若某条终态行在下一次 60s 周期的 `expire_pairings`/`due_pairings` 扫描前被清理，该 `PairingId` 的内存 secret **没有清除路径**（直到进程退出），而合同/规格把 `expires_at` 写成硬上界。WP2 内无法判定（属切片 4/5 组合根的扫描与清理顺序）。
- **影响**：最坏情况是少量被拒绝配对的内存条目驻留到进程退出（约每配对数十字节），无安全可利用性（`verify_claim` 要求 `Created`、`pairing_sas` 要求 `PendingConfirmation`、`complete` 要求 `approved`，我逐条核对）。但它使「硬上界」在装配层留了一个未写明的隐含前提。
- **最小修复建议**：在合同 §4.1（或 §11.3 的配对保留段）补一句装配要求：「组合根必须对 `expires_at <= now` 的全部配对记录（含终态）调用 `due_pairings`，且该扫描先于任何配对行的保留/容量清理」；或给 `Authority` 一个「按存储当前存活 id 集合清理」的入口（`prune_secrets(&[PairingId])`），使上界不依赖记录是否还在库里。

### RV3-WP2-F6（P2，公开面/契约登记）`complete` 与 `complete_auth` 是同一操作的公开双入口，且有若干公开入口未登记

- **位置**：`crates/identity-auth/src/pairing.rs:413`（`pub fn complete`）、`handshake.rs:305-312`（`pub fn complete_auth` 仅 `self.complete(...)` 转发）；`docs/IDENTITY_AND_AUTH_CONTRACT.md:261-263`（三入口清单只登记 `complete_auth`）、`:294-297`（登记 `challenge_cache_len`/`pairing_status`/`failure_count`/`has_secret`/`MAX_CHALLENGES`，并自述「新增同类入口时在本节登记，避免公开面静默膨胀」）。
- **事实**：`Authority` 的公开方法还包含 `now`、`local_node`、`node_public_key`、`sign_sync_pairing_host_proof`/`sign_sync_host_challenge`/`sign_node_link_pairing_owner_proof`/`sign_node_link_challenge`、`reset_memory`，以及未在合同 §4.1/§5.1 出现的 `complete`（与 `complete_auth` 行为完全相同）。另外 §5.1:259-260 的「`hello` 是唯一带 `await` 的入口」在存在 4 个公开 async `sign_*` 时字面上不成立（应限定为「三个握手入口中唯一」）。
- **影响**：无运行时影响；但切片 4–7 的 adapter 会面对两个等价入口（`AGENTS.md` §7 要求公开 API 最小），且合同自称的「公开面登记」不完整，后续复核只能靠逐方法 grep 才能发现新增。
- **最小修复建议**：把 `complete` 收为私有（或删除转发，只保留合同登记的 `complete_auth`）；在 §5.1 的登记段落补一行「`now`/`local_node`/`node_public_key`/`reset_memory`/四个 `sign_*` 亦为公开入口及其用途」，并把「唯一带 `await`」限定为三个握手入口。

## 5) plan.md 覆盖索引与 Check ID 核对

**WP2 的 Check 范围**：`plan.md:723` 的 Work Packages 表把 WP2 写范围定为 `crates/identity-auth/**`（含 `Cargo.toml`）+ `members` 条目 + `Cargo.lock`，Verification = `[PV3]`（+ `[PV1]`）；覆盖索引 R1–R29（pairing）/R30–R55（handshake）/R56–R71（展开）的 `checks` 均为 `[PV3]`。

| Check ID | 计划判据（`plan.md:796-812`） | 本轮核对 | 是否影响本轮判断 |
|---|---|---|---|
| PV3 | `cargo test --locked -p identity-auth --all-features`：12 向量重算/SAS/畸形拒绝/失败路径全过、无 0 用例、无全跳过 | **已有日志（非我执行）**：`reports/rv3-pv3.log` `EXIT=0`，authorization 15 / handshake 19 / pairing 26 / parity 2 / ports 9 / transcripts 5 = 76 + 2 doc-tests = 78，0 failed；`rv3-pv1.log:487` 的 `pairing … 26 passed` 一致。**注**：`unittests src\lib.rs` 目标为 0 用例（库无内联单测），计划措辞「无 0 用例」按聚合 78 理解 | 否（F1–F6 均由源码/契约判据得出） |
| PV1 | `npm run verify` 全绿 | **已有日志**：`rv3-pv1.log` `EXIT=0`，check 9 items passed、doc links 369/3589、contract drift OK、agentic Installation PASS；fmt/clippy/workspace test 无 failed | 否 |
| PV2 | `check-crate-boundaries.mjs`：`identity-auth` 行只依赖 core/两协议/`acpr-transcript`，`acpr-wire` 格为空 | **已有日志**：`rv3-pv2.log`「10 个 crate … 与 §5 矩阵一致」；我另核 `crates/identity-auth/Cargo.toml` 依赖集合与该判据一致（无 `acpr-wire`、无 runtime/serde/DB） | 否 |
| 计划自检行 | 非测试代码**可失败路径**零 `unwrap/expect`；`cfg` 零命中；持锁不跨 `await` | **已有日志**：`rv3-selfcheck-rg.log`；我**自行 grep 复核**同结论（`types.rs:256` 一处 `expect`，不可失败路径；`cfg`/`from_der` 零命中；9 处 `.await` 全在锁外） | 否 |
| RV1 | 独立 reviewer 报告完整、无未解决阻断项 | 本轮即 RV3（`rv3-auth`）；上一轮 `rv2-auth.md` 的 P1 已闭环、P2 大部分闭环（残留 F1/F4/F6） | 是 |
| Check Plan Changes | RV2 修复轮的 4 条调整（Linux 目标 lint、mutation 检查、自检判据收窄、测试文件位置调整） | 与 `rv3-linux-clippy.log`、`rv3-mutation.log`、`rv3-selfcheck-rg.log` 一致；mutation 日志含 10 个 mutation + 恢复复跑 | 否 |

**观察（不单列为发现）**：
1. **覆盖索引的 evidence 路径仍是旧轮次**：`plan.md:31` 起所有 R 行的 `evidence` 仍写 `reports/wp2-identity-auth-{pairing,handshake,expansion,ports}.log`（RV1 轮、revision 6861046 的产物），而 `verification.md` 的 Check/RV3 行已改用 `rv3-*`。Check ID 本身对齐；但 7.2「覆盖索引逐行关联」时需明确「复用旧证据的适用性」或改用 RV3 轮日志，否则会出现「计划指向的日志版本 ≠ 本轮 target」。
2. **R30 的映射偏松**：覆盖索引 R30 = `specs/identity-handshake/spec.md` 的「### Requirement: 每条新连接完整执行挑战签发」，而缓冲上限（`MAX_CHALLENGES`）并非该 spec 的场景，其来源是 `AGENTS.md` §5 与 `IDENTITY_AND_AUTH_CONTRACT.md` §5.2。`challenge_cache_is_bounded` 引 R30 属合理但宽泛的挂靠（新用例不因此无效，只是「R30 全绿」不等于「上限被规格要求」）。
3. **plan.md 的复核安排与角色文档不一致**：`plan.md:818` 写「修复后由**同一** reviewer 对新固定版本复核」，而 `openspec/schemas/agentic/roles/reviewer.md` 与 `tasks.md:3.4` 要求「由**新的**隔离子 Agent 复核」。建议在 8.1 前统一为后者的措辞。

## 6) 未覆盖 / 无法确认项与所需证据

1. **未能读 `fe52694..5ed869f` 的已提交 diff**（工具限制）：因此「本轮只改了列出的项」这一范围声明我未核对，本轮结论只对 5ed869f 的**文件内容**负责。
2. **`rv3-*.log` 不能按提交绑定**：头部一致标注 base revision `fe52694` + 脏工作区。建议后续日志同时写入 `git rev-parse HEAD` 与实际被测树的 `git stash create`/内容哈希，否则最终验收只能依赖「用例数与源码吻合」这类旁证。**所需**：主 Agent 在候选轮（6.3/7.1）重跑时标注确切被测提交。
3. **Linux 运行时未在本机执行**：`rv3-linux-clippy.log` 只有编译 + lint（本机无法链接 Linux 二进制）。对 WP2 而言无 Linux-only 分支（`cfg` 零命中），因此不影响我的判断；但 `[PV3]` 的 Linux 运行时判定仍待 CI。
4. **F5 需要切片 4/5 的证据才能真正收口**：`TrustStore::expire_pairings`（§11.6 第 6 条）与配对行保留/容量清理的执行顺序、以及 60s 周期任务的调用点，都由组合根装配；WP2 只能登记为隐含前提。
5. **F2 的失败方向需要切片 4/5 确认**：`settle_pairing` 提交失败时 adapter 是否可能带旧快照重复调用 `complete_auth`，决定该窗口是「记账噪声」还是「收尾写入失败」。
6. **本轮未检视**：`crates/identity-keystore/**`（WP3，含 R86 两阶段修复）、`docs/MODULE_ARCHITECTURE.md`/`README.md`/`DEVELOPMENT_PLAN.md`/`AGENTS.md`（WP4）、`.gitleaks.toml`；这些请以 `rv3-keystore`/`rv3-docs` 为准。`design.md` 的其他段落我只按与本 WP 相关处读取。
7. **`npm run check` 的十道门禁、`cargo fmt/clippy`、mutation 复跑**：我**未执行**，上文引用均为既有日志；`gitleaks`/`cargo-deny` 本地无等价物（只在 CI），本轮不构成任何通过声明。
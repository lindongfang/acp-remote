<!-- 由独立 reviewer 子 Agent 产出（RV4 定向确认轮，target revision 892e5f5），主 Agent 原样落盘（仅歧义 `§` 引用补全被引文档名）。 -->
## 结论

**FAIL（定向复核：1 条原有 P1 已实质修复；1 条原有 P2 升级确认为未修的 P1 阻断项——证据文件缺失；其余 P2 3 条未修/部分修、3 条已修；未发现新的产品/代码缺陷）**

| 原问题 ID | 原级别 | 本轮结论 | 关键证据 |
| --- | --- | --- | --- |
| RV3-DOCS-1 / RV3-WP2 F1（design.md:72 secret 生命周期） | P1 | **已修（已闭环）** | `design.md:72`；合同 §4.1 `:125-126`、§4.3 `:193`；`specs/identity-pairing/spec.md:100`；`crates/identity-auth/src/pairing.rs:265-268,272-321,327-343,345-357,416-421` |
| RV3-DOCS-2（日志 target/revision、跨变更同名文件完整路径） | P2 | **部分修**：(a) 头部已含 revision + 完整命令（含 target triple）✓，但 revision= `de61aa5`≠本轮 target `892e5f5`，且变更目录内 `892e5f5` 零命中；(b) 仅 `verification.md` 改完整路径，`plan.md`/`tasks.md` 仍是裸 `reports/...` | `reports/rv4-*.log:2-3`；`verification.md:16,30,31,34,35,37,86`；`plan.md:807,808,812`；`tasks.md:12,26,36,53,56` |
| RV3-DOCS-3（自检行 vs 自检日志） | P2 | **未修**（计数、行号、所属模块、命令模式四处都不符） | `verification.md:38` vs `reports/rv4-selfcheck-rg.log:5-21`（我独立 grep 复现同一 17 行） |
| RV3-DOCS-4（旧 Check 行 R 编号） | P2 | **已修** | `verification.md:18` ↔ `plan.md:242,250,443,451,566,574,713` |
| RV3-DOCS-5（SAS 入口需 `await`） | P2 | **已修** | `design.md:69`；`pairing.rs:91`（`pub async fn pairing_sas`）、`pairing.rs:107-110` |
| RV3-DOCS-6（§5.1 登记 + 单一对外名） | P2 | **已修** | 合同 §5.1 `:294-301`；`handshake.rs:305-312`；`pairing.rs:410-416` |
| RV3-WP2 P2（证据绑定/悬空引用） | P2 | **未修 → 本轮判为 P1 阻断**：`reports/rv4-mutation.log` 被引用 5 处但**文件不存在**；`rv3-*` 引用已清除 ✓ | `verification.md:37,44,124,128`；`plan.md:689`；`read reports/rv4-mutation.log` → ENOENT；`reports/` 全量 `ls` 无该文件；`grep -i mutation reports/rv4-*` 只命中一个用例名 |
| D6/D7 实现名订正（RV3-WP3 F6 跨车道项） | P2 | **部分修**：`FileKeystore::new`/`with_availability`、`EntryMissing`/`EntryCorrupt`/`Unavailable` 均已订正 ✓；但 `design.md:114` 仍写实现里不存在的 `PlatformKeystore` | `design.md:109,112,114,120`；`crates/identity-keystore/src/store.rs:95,102`；`crates/identity-auth/src/port.rs:136-144` |

---

## 1) 隔离与版本声明

- **task_id**：RV4（定向确认轮，第 4 轮）；**role**：独立 reviewer 子 Agent（只读）；**phase**：修复后 recheck（原问题 ID 逐条复核）。
- **agent_context**：本轮为新会话子 Agent，未继承实现对话、未参与实现；工具集只有 `read/grep/find/ls/watchdog_diff` 与 `contact_supervisor`，**无 shell/git**。
- **Base Revision**：任务给定 `de61aa5`（RV3 修复轮）；**Target Revision**：`892e5f5`。
- **版本确认（我自己做的一条机械核实）**：`watchdog_diff` → `No working-tree changes against reviewer-launch HEAD 892e5f52b75b` ⇒ **我读到的所有受版本控制文件内容 = `892e5f5` 的内容**，检视期间无变化。
- **限制（如实声明）**：(a) `watchdog_diff` **不含已提交区间**，我**没有**读 `de61aa5..892e5f5` 的 diff，因此「哪一条是本轮改的」我不归因，只对 `892e5f5` 的文件现状负责；任务允许的 `git show --stat`/`git log` 我**无法执行**（无 shell），所以**无法核实 commit 是否真实存在**（`de61aa5`/`892e5f5` 的存在性留给调度者核对）。(b) 我**未执行** `npm run verify`、`cargo test`、`grep`、`diff` 中的任何一条；凡引用「PASS/exit 0/用例数/revision」处**一律来自既有日志文本**（在正文逐处标注）。
- **实际检查范围**：`openspec/changes/identity-auth-and-keystore/{design.md,plan.md,tasks.md,verification.md,specs/**}`、`docs/IDENTITY_AND_AUTH_CONTRACT.md`、`docs/MODULE_ARCHITECTURE.md`（定向段）、`openspec/changes/…/reports/**`（全量 `ls` + 逐条头部/正文）、`crates/identity-auth/src/{pairing.rs,handshake.rs,authority.rs,port.rs}`、`crates/identity-keystore/src/{store.rs,ephemeral.rs}`。
- **未扩展范围**：不做 RV1–RV3 已完成的 WP2/WP3 全量检视（本轮刻意收窄）；未读 `crates/identity-keystore/tests/**` 全文与 `schemas/`、`fixtures/`。
- **资源清理**：无（只读，未产生临时文件、未改分支、未写 `progress.md`——review-only 优先）。

---

## 2) 逐条复核

### RV3-DOCS-1 / RV3-WP2 F1（P1）：**已修**

- `design.md:72` 现写：「**`settle` 不清除内存 secret**：批准后的配对还要支持状态查询的 HMAC 证明，清除发生在「首次认证成功」（`complete_auth` 返回 `consume_pairing`）或到达 `expires_at`（`due_pairings` 统一清除，含 `rejected` 等终态记录）」。原「两条路径都清除内存 secret」的说法在本文件中已**零命中**（对 `design.md` 全量检索 `清除|secret` 只命中 `:68-79/:109-112/:49-51`）。
- 合同一侧同口径且只有一套：§4.1 `:125-126`「**两条路径都不在落定时清除 secret**」；§4.3 `:193`「最迟在 `expires_at` 清除；`approved` 配对在首次 WSS 认证成功时提前清除；`rejected` 可为可靠轮询保留到原过期时间，但**不得超过**该时间」；§4.1 `:134-137` 登记硬上界与开放项（「`rejected`/`expired` 也在此清除」+「Approved 不是终态，因此『已批准但已过期』也在返回集里，其落库终态……是留给切片 4/5 的开放项」）。
- 第三份权威（spec）一致：`specs/identity-pairing/spec.md:100`。
- 实现一致（三处都核对过）：
  - `pairing.rs:265-268`（`settle` 文档：不在落定时清除）+ `:272-321`（`Approve` 只 `mark_approved`，`Reject` 只返回 `PairingSettlement::rejected`，两条路径**都没有** `clear_secret`）；
  - `pairing.rs:327-343`（`due_pairings`：**先 `clear_secret` 再判终态**，注释 `:322-326` 与代码 `:336-341` 一致；`approved` 已过期也进返回集）；
  - `pairing.rs:416-421`（`complete` 只清「已批准」）。
- **一处非阻断观察**：design 只写到「`due_pairings`/`unrecoverable_after_restart` 返回需要由调用方终结的配对标识」（`design.md:73`），**未复述**「已批准但已过期仍在返回集、落库终态属切片 4/5 开放项」这两句——该细节只在合同 §4.1 `:136-137` 与 `pairing.rs:324-326` 登记。**不构成矛盾**，故不计为发现。

### RV3-DOCS-2（P2）：**部分修**

- (a) **头部已达标**：`rv4-pv1.log:2-3`、`rv4-pv2.log:2-3`、`rv4-pv3.log:1-2`、`rv4-pv4.log:1-2`、`rv4-pv5.log:1-3`、`rv4-selfcheck-rg.log:2-3` 均写明命令与 `revision：de61aa518c7f40e777a676274a7882fa7fe3db79（de61aa5，干净）`；`rv4-linux-clippy.log:2` 回显完整命令且**含 target triple**：`cargo clippy --locked -p identity-auth -p identity-keystore --target x86_64-unknown-linux-gnu --all-targets --all-features -- -D warnings`（这条正是 RV3-WP3 F5(c) 的诉求）。
- (a) **未达标的部分**：**7 个 rv4 日志全部标注 `de61aa5`，而本轮 target 是 `892e5f5`**；变更目录内 `892e5f5` **零命中**（我在 `openspec/` 下全量检索）；`verification.md:86` 的 RV4 行 revision 列也写 `de61aa5`。⇒ RV4 的证据**未绑定到本轮 target**。因无 git，我无法判断 `de61aa5` 与本轮被测树在受测代码上是否等价（`watchdog_diff` 只能证明**当前**文件内容 = `892e5f5`）。
- (b) **仅 `verification.md` 改完整路径**：`verification.md:16,30,31,34,35` 对 `du1-pv1.log` 用 `openspec/changes/identity-auth-and-keystore/reports/du1-pv1.log` ✓。但 `plan.md:807/808/812`（`reports/du1-pv1.log`、`reports/du1-main-verify.log`、`reports/rv1-wp1.md`…`reports/rv1-du1.md`）与 `tasks.md:12,26,36,53,56` **仍是裸 `reports/...`**，而仓库根 `reports/` 确实并存**另一变更**的同名文件（根目录实读：`du1-pv1.log`、`du1-main-verify.log`、`rv1-wp1.md`、`rv1-wp4.md`、`rv1-du1.md`、`rv1-wp23.md`…）。`plan.md` 全文既无「`reports/` 指本变更目录」的定义句，也无任何完整路径引用。
  ⇒ 因此 `verification.md:127` 记录的修复结论「`reports/` 引用全部改完整路径」**与事实不符**。

### RV3-DOCS-3（P2）：**未修**

- `verification.md:38` 写「**共 15 处命中**」并归类为 ① `types.rs:256`（1）② `entropy.rs:37-38`（2）③ `ephemeral.rs:53`（1）④「`store.rs` 的 `#[cfg(test)]` 模块内 11 处（`mode_tests`/`unix_modes`/`atomic_tests`，行号见日志）」（1+2+1+11=15）。
- 但它自己引用的 `reports/rv4-selfcheck-rg.log:5-21` 实际列出 **17 行命中**：`types.rs:256`；`entropy.rs:37`、`:38`；**`ephemeral.rs:54`**；`store.rs:499,515,523,525,529,530,539,567,570,571,573,581,587`（**13** 行）。
- **我独立复核（只读 grep，非执行日志命令）**：对 `crates/identity-auth/src` + `crates/identity-keystore/src` 用同一模式检索得到**完全相同**的 17 行（顺序、行号逐行一致）。⇒ 日志本身与源码相符，**错的是 `verification.md:38` 的转录**：计数 15≠17、`ephemeral.rs:53`≠`:54`、`store.rs` 11≠13，且点名的 `mode_tests`（`store.rs:458-481`）**一处 `expect` 都没有**——13 处实际分布在 `unix_modes`（`store.rs:482-549`，7 处）与 `atomic_tests`（`store.rs:550-599`，6 处）。
- **命令模式不一致**：`verification.md:38` 记 `grep -rn "unwrap()\|expect(\|panic!\|unreachable!\|from_der"`；日志头 `rv4-selfcheck-rg.log:2` 记 `grep -rn "unwrap()|expect(|panic!|unreachable!|from_der"`（**BRE 下未转义的 `|` 是字面量 → 该字面命令匹配不到任何东西**）；文件名/标题写「自检 **rg**」，`plan.md:801` 用的是 `rg -n "from_der|unwrap\(|expect\("`。三处模式互不相同，**记录的命令不可复现其声称的输出**。

### RV3-DOCS-4（P2）：**已修**

- `verification.md:18` 现写「配对状态机（R1–R29）、握手（R30–R55）、授权展开（R56–R71）；端口与秘密类型在 `plan.md` 的 Coverage Index 里按 `platform-keystore` 段（R72–R90）……」。
- `plan.md` Coverage Index 实读边界吻合：`R29`→`specs/identity-pairing/spec.md`（`:242-249`）；`R30`→`specs/identity-handshake/spec.md`（`:250`）、`R55`结束（`:443-450`）；`R56`→`specs/scope-expansion/spec.md`（`:451`）、`R71`结束（`:566-573`）；`R72`→`specs/platform-keystore/spec.md`（`:574`）、`R90`结束（`:713`）。端口类条目（`R81-R84` 证据含 `reports/wp2-identity-auth-ports.log`，`plan.md:650-674`）确实落在 R72–R90 段。✓

### RV3-DOCS-5（P2）：**已修**

- `design.md:69`：「状态机入口（除**挑战签发**与 **SAS 派生**需要 `await` 经端口读本节点公钥/签名外均为纯同步计算；`pairing_sas` 因此是 `pub async fn`）」。实现吻合：`pairing.rs:91` `pub async fn pairing_sas`，内部 `pairing.rs:107-110` `self.node_public_key().await`（`authority.rs:61` 为 async）。

### RV3-DOCS-6（P2）：**已修**

- 合同 §5.1 `:294-301` 已登记诊断入口与另两个公开入口：`Authority::reset_memory()`（`:297-298`，含「丢弃全部内存 secret 与挑战缓存……**不**触碰持久材料」）与 `Authority::node_public_key()`（`:299`），并明确：「认证收尾对外只有**一个**名字 `complete_auth`（其实现体是 crate 私有的 `complete`），避免同一行为出现两个公开名」（`:300`）。
- 实现吻合且只有一个对外名：`handshake.rs:305-312` `pub fn complete_auth(...) { self.complete(fact, pairing, at) }`；`pairing.rs:416` `pub(crate) fn complete`（文档 `:410-411` 自证「保持 `pub(crate)` 以免出现两个对外名字」）。全 crate 检索 `fn complete` 只有这两处定义。

### RV3-WP2 P2（证据绑定 / 悬空引用）：**未修（本轮升级为 P1）**

- **`reports/rv4-mutation.log` 不存在**：`read` 直接返回 `ENOENT`；变更目录 `reports/` 全量 `ls`（48 个文件）无此文件；仓库根 `reports/` 全量 `ls` 也无；`grep -i mutation` 在 `reports/rv4-*` 下只命中一条用例名（`rv4-pv1.log:741`），**没有任何 rv4 日志包含 mutation 输出**。而 `reports/rv3-mutation.log`（RV3 的解释）已随 `rv3-*` 一起被替换掉。
- 被引用位置（5 处）：`verification.md:37`（PASS Check 行的 Command 列与 Evidence 列）、`:44`（Check Plan Changes 第 2 条，含「**10 个 mutation 全部被对应用例捕获**」）、`:124`（Failures and Retests 的复现/回归证据，逐条列出 mutation 1–10）、`:128`（RV3 修复轮重测里的 mutation 1–3）；`plan.md:689`（Coverage Index `R86` 的 evidence 第二项，即 RV3-WP3 F5(d) 要求的「证据指针改指修复轮日志」）。
- ⇒ 结论：`rv3-pv*.log` 的引用**已全部清除**（`verification.md`/`plan.md`/`tasks.md`/`design.md`/`specs/**` 内 `rv3-` 零命中，仅存于历史评审报告 `reports/rv3-*.md` 自身），但取而代之的 `rv4-mutation.log` **从未落盘**，于是「本轮修复可被证伪」这条关键主张在其自己的验收记录里**没有可解析的证据**。这与 RV2-WP2 F3（日志落在仓库根、Check 行引用不可解析 = P1）同族且更彻底（文件不存在）。
- 另有两处裸路径/外来引用（同 RV3-DOCS-2(b)，P2）：`plan.md:807` 的 `reports/du1-main-verify.log`、`plan.md:812` 的 `reports/rv1-du1.md`——两者**只存在于仓库根**（属另一变更/尚未开始的任务 6.7、6.4），而 `reports/rv1-wp1.md`…`rv1-wp4.md` **两处都有同名文件**。

### D6/D7 实现名订正：**部分修**

- 已订正 ✓：`design.md:109` `FileKeystore::new(root, entropy)`/`with_availability(..)`、`EphemeralKeystore::new(entropy)`、`OsEntropy` —— 与 `store.rs:95`（`pub fn new(root: impl Into<PathBuf>, entropy: Arc<dyn EntropySource>)`）、`store.rs:102`（`pub fn with_availability`）逐一吻合；`design.md:112`「**引用缺失 → `EntryMissing`**、**解包失败/条目损坏 → `EntryCorrupt`**、**平台不可用 → `Unavailable`**」与 `port.rs:136-144` 的 `enum KeystoreError { Unavailable, EntryMissing, EntryCorrupt, EntryInvalid, … }` 吻合；`design.md:120`（D7）也写 `KeystoreError::Unavailable`。`KeystoreUnavailable` 在 `design.md` 内**零命中**，`PlatformKeystore::open` 在 `design.md` 内**零命中**。
- 未订正 ✗：`design.md:114` 仍写「`EphemeralKeystore`/`PlatformKeystore` 都只依赖该端口」——`PlatformKeystore` 在 `crates/**` 中**不存在**（只有 `FileKeystore`、`EphemeralKeystore`）。RV3-WP3 F6 要求订正的正是这个失效名，只改掉了 `::open` 那一处，第二处漏改。（`docs/MODULE_ARCHITECTURE.md:484` 也出现 `PlatformKeystore::open(&config.identity)`，但那一处位于 §6「组合根」的未来 `app` 装配示意代码块（同块内 `SqliteStore::open`/`ProcessAgentBackend`/`NodeLinkBackend` 亦均未落地），属规划形状而非本变更「已实现名」，我**不**计为发现。）

---

## 3) 新发现（只报 P0/P1）

**无新的 P0/P1。** 本轮唯一的 P1（`RV4-DESIGN-F1`）是原有问题 ID（RV3-WP2 的 P2 证据绑定）在同一位置上被确认**未修**后的实测结论，已在上表与 §2 登记，不再重复计为新发现。

## 4) Findings（评审报告表）

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| RV4-DESIGN-F1（= 原 RV3-WP2 P2，未修） | **P1（阻断）** | `verification.md:37,44,124,128`；`plan.md:689`；应存在路径 `openspec/changes/identity-auth-and-keystore/reports/rv4-mutation.log` | `read` → `ENOENT`；`ls` 变更目录与仓库根 `reports/` 均无；`grep -i mutation reports/rv4-*` 无 mutation 输出 | `verification.md:37` 是判 PASS 的 Check 行，`:124` 声称「10 个 mutation 全部被对应用例捕获」，`plan.md:689` 的 R86 把该文件列为证据 —— 全部无可解析证据；最终验收无法核对「修复可被证伪」这一 RV2/RV3 核心判据 | 二选一：(a) 按 `verification.md:44` 的记录在生产树复原/重跑 mutation 检查并把输出落到 `reports/rv4-mutation.log`；(b) 若不再提供该证据，则删除这 5 处引用，并把 `:124`/`:37` 的 mutation 结论改为「无本轮证据」或改引确有其物的日志 | 待修复后由新 reviewer 按原 ID 复核 |
| RV4-DESIGN-F2（= 原 RV3-DOCS-3，未修） | P2 | `verification.md:38` vs `reports/rv4-selfcheck-rg.log:5-21` | 记录 15 处 / `ephemeral.rs:53` / `store.rs` 11 处 / 点名 `mode_tests`；日志与源码实为 17 处、`ephemeral.rs:54`、`store.rs` 13 处（`unix_modes` 7 + `atomic_tests` 6，`mode_tests` 0 处） | 记录与自身证据不符，读者核对会得到相反印象；`plan.md:801`「命中处必须逐条登记理由」实际未逐条落点 | 把该行改为可直接核对的枚举：17 处、逐处给出行号与所属模块（`mode_tests` 应删除） | — |
| RV4-DESIGN-F3（= 原 RV3-DOCS-2，部分未修） | P2 | 7 个 `reports/rv4-*.log` 头部；`verification.md:37,86`；`plan.md:807,808,812`；`tasks.md:12,26,36,53,56` | 日志统一标 `de61aa5`（`rv4-pv1.log:3` 等），本轮 target 为 `892e5f5`，且 `892e5f5` 在变更目录零命中；`plan.md`/`tasks.md` 仍用裸 `reports/...`，仓库根有另一变更同名文件（根 `reports/` 实读） | 证据无法从工件内绑定到本轮 target；`verification.md:127`「`reports/` 引用全部改完整路径」与事实不符 | 在本轮修复后的确切提交上重跑并回显该 commit（或注明 `de61aa5` 与本轮树在受测文件上等价的依据）；`plan.md`/`tasks.md` 的 `du1-*`/`rv1-*` 引用改完整路径 | — |
| RV4-DESIGN-F4（= 原 RV3-WP3 F6，部分未修） | P2 | `design.md:114` | 该行仍写 `EphemeralKeystore`/`PlatformKeystore`；`PlatformKeystore` 在 `crates/**` 零命中，实现类型是 `FileKeystore`（`store.rs:95`） | D6「identity-keystore 的结构」用了一个不存在的类型名，与同节 `:109` 自相矛盾 | 把 `PlatformKeystore` 改为 `FileKeystore`（或直接写「两个实现」） | — |

—

## Assessment

**本轮检视结论（对应 Target Revision `892e5f5`，由 `watchdog_diff` 证明我读到的文件内容即该版本内容）**

- **原有唯一 P1（RV3-DOCS-1 / RV3-WP2 F1，`design.md:72`）已实质修复且三处口径统一**：spec `:100` ↔ 合同 §4.1 `:125-126`/§4.3 `:193` ↔ design `:72` ↔ 实现 `pairing.rs:265-268/272-321/327-343/416-421`；`settle` 两条路径都不清除、`due_pairings` 先清除再判终态（含 `rejected`）、`complete_auth` 只消费「已批准」。**未发现任何产品/代码缺陷，也未发现该修复引入的回归。**
- **阻断项只有一条，且属证据完整性而非产品行为**：`reports/rv4-mutation.log` 被 5 处引用（含一条判 PASS 的 Check 行的 Command 与 Evidence 列）却不存在，导致「10 个 mutation 全部被对应用例捕获」这一关键主张在验收记录内无法核对。按本项目既有先例（RV2-WP2 F3：Check 行日志引用不可解析 = P1），判 **P1**，故本轮 **FAIL / BLOCK**。
- **另有 3 条 P2**（自检计数与命令模式、日志未绑定本轮 target + 裸路径、`design.md:114` 失效类型名）**不阻断**代码判断，建议随上述 P1 一并修正后再提交最终验收。
- **未执行/待补**：我未执行任何命令。`npm run verify`、`cargo test -p identity-auth/-p identity-keystore`、Linux 目标 clippy 的绿由**既有日志**（`reports/rv4-pv1..pv5.log`、`rv4-linux-clippy.log`）支撑：例如 `rv4-pv3.log:29/54/86/94/109/120/128` 合计 15+19+26+2+9+5+2=78 用例、`rv4-pv4.log:12/23/35/51/69` 合计 3+5+6+10+12=36 用例，与 `verification.md:37` 的计数**逐项吻合**（我读日志，未复算）。这些不影响本轮静态判断，但 **mutation 一项因文件缺失按上表记为待补，且会影响最终验收的证据完整性**。
- **复用/计划变更**：`rv4-linux-clippy.log` 的「命令 + target triple」为 RV1 修复轮新增项的合规落地；自检判据收窄与测试位置调整仍在 `verification.md` 的 Check Plan Changes 登记（内容与理由具备）。
- 我不更新任何任务状态，也不代替调度者宣称本变更可归档；修复后需由**新的独立 reviewer** 按原问题 ID 复核。

### 未覆盖 / 无法确认

1. **提交存在性与区间 diff 无法核对**：无 git/shell，`de61aa5`、`892e5f5` 是否真实存在、`de61aa5..892e5f5` 究竟改了什么，我**无法确认**；`verification.md:37/86` 的 revision 列是否指向真实提交需调度者用 `git rev-parse`/`git show --stat de61aa5` 核对。
2. **Linux 运行时行为仍未验证**（沿用既有日志的残余限制）：只有「编译 + lint」（`rv4-linux-clippy.log`，本机无法链接 Linux 二进制），`unix_modes` 的 0700/0600 断言与 `platform/unsupported.rs` 的失败关闭只能在 CI Linux runner 上真跑。
3. **RV3 轮的原始证据链已不可重建**：`reports/rv3-*.log`（含 `rv3-mutation.log`）已被 `rv4-*` 取代，而历史评审报告仍引向它们（如 `reports/rv3-docs.md:55,77`、`reports/rv3-auth.md:41`、`reports/rv3-keystore.md:50` 引 `rv3-pv*.log`/`rv3-mutation.log`）——这些引用现在会落到不存在的文件。它们属冻结的历史轮次报告（其引用在其回合内成立），我**不**计为发现，但登记为可核对性缺口。
4. **本轮未复算的既有日志**：`rv4-pv1/pv2/pv5` 的 exit 0、`npm run verify` 十道门禁、`10 crate 边界 OK` 等均由日志文本读出，我未执行；`.gitleaks.toml` 规则命中能力与 `cargo-deny` 判定本地无等价物，只在 CI。
5. **`docs/MODULE_ARCHITECTURE.md:484` 的 `PlatformKeystore::open`**：属 §6 组合根的未来 `app` 示意块（同块其他名字也未实现），未计为发现，供调度者判断是否要一并改名。
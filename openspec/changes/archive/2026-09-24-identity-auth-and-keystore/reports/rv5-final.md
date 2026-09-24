<!-- 由独立 reviewer 子 Agent 产出（RV5 定向确认轮，target revision eee409b），主 Agent 原样落盘（仅歧义 `§` 引用补全被引文档名；报告正文未改动）。 -->
# RV5 最终定向确认报告（reviewer 子 Agent，只读）

```
task_id: RV5 / rv5-final
role: reviewer（独立、只读；未继承实现对话）
phase: 最终定向确认（recheck，范围刻意最小）
agent_context: 新 reviewer 子 Agent；工具集 read/grep/ls/watchdog_diff；无 shell、无 git、未执行任何命令、未修改任何文件
target_revision: eee409b（工作区无改动；见 §1 的版本限制）
scope: 仅确认 RV4 列出的 P1 与 P2 是否收口（不做新的全量检视）
changes: 无（只读，未修改 tasks/verification/progress/代码）
checks: 复现自检 grep（17 处）、逐条核对 reports/ 引用存在性、逐条比对记录与工件/评审报告原文
result: FAIL（记录/证据完整性 1 条 P1；产品代码无 P0/P1）
evidence_paths: openspec/changes/identity-auth-and-keystore/reports/rv4-mutation.log、reports/rv4-{pv1..pv5,linux-clippy,selfcheck-rg}.log、verification.md、plan.md、tasks.md、design.md、reports/rv3-auth.md、reports/rv4-{design,keystore}.md、crates/identity-keystore/{src/store.rs,src/entry.rs,src/platform/windows.rs,src/ephemeral.rs,tests/fail_closed.rs}
resource_cleanup: 无需清理（未写入、未生成临时文件、未执行构建）
```

## 结论

**FAIL**（唯一阻断项为 RV4-P1 的未完全收口：`reports/rv4-mutation.log` 已被重建且本身可核对，但记录中仍有 5 处声称「10 个 mutation 全部被对应用例捕获 / mutation 1–6 / 7–10」，与该工件实际内容（4 条，头部自述「RV3/RV4 修复」）**互相矛盾**。产品代码与用例**零改动**，本轮未发现任何代码层 P0/P1。修复面仅限记录文本/证据，属可低成本收口的一项。）

| 原问题 ID（RV4） | 本轮结论 | 关键证据 |
| --- | --- | --- |
| **RV4-P1**（mutation 日志不存在却被引用 5 处） | **部分收口 → P1 残留 `RV5-F1`** | 文件已存在（`reports/rv4-mutation.log`，头部 `### revision：0813ad5…（0813ad5，干净）`；4 条 mutation：退化点→被击穿断言；恢复后 3+5+6+10+12+0 全绿）；5 处引用的**路径**全部可解析；但 `verification.md:37`、`:44`、`:126` 与行 `80`/`81` 的「10 条 / 1–6 / 7–10」无工件支持 |
| RV4-DOCS-2(a)（日志 revision/命令绑定） | **部分收口（P2 残留）** | 7 个 rv4 日志头部均含 `### 命令：…` + revision；`rv4-linux-clippy.log:2` 含 `--target x86_64-unknown-linux-gnu`；但 `verification.md:37` 证据列括注仍写「头部含 revision `de61aa5`」（工件实为 `0813ad5`）；行 `88` 的 RV5 行 revision 写 `0813ad5`（本轮 target 为 `eee409b`） |
| RV4-DOCS-2(b)（跨变更同名文件完整路径） | **部分收口（P2 残留）** | `tasks.md:12,26,36,56`、`plan.md:807,808` 的 `du1-pv1.log`/`du1-main-verify.log`/`rv1-wp1..4.md` 已改完整路径 ✓；**`plan.md:812`、`tasks.md:53` 的 `reports/rv1-du1.md` 仍是裸路径**，而仓库根 `reports/rv1-du1.md` 确实存在（另一变更同名文件） |
| RV4-DOCS-3（自检行 vs `rv4-selfcheck-rg.log`） | **已修（计数/行号）**；命令写法与模块归属仍 P2 | 我按同一模式独立复现：17 处、逐条 `file:line` 与日志**完全一致**；但日志把交替符写成 `[|]`（照抄执行将零命中）、`plan.md:801` 仍用 `rg -n`、`verification.md:38` 用 `\|` 三种写法并存；模块归属仍点名 `mode_tests`（该模块实际 **0 处**，13 = `unix_modes` 7 + `atomic_tests` 6） |
| RV4-DOCS-4（RV2-WP3 行用例数） | **已修** | `verification.md:81` = 「36 用例：3 lib + 5 dpapi + 6 entry_format + 10 fail_closed + 12 keystore」，与 `rv4-pv4.log` 实测 3/5/6/10/12=36 逐项一致 |
| RV4-DOCS-4（Check Plan Changes 的 `ephemeral.rs` 旧行号） | **已修** | `verification.md:47` 与 `:38` 均为 `ephemeral.rs:54`，与日志第 6 行一致（旧值 `:50` 已消失） |
| RV4-DOCS-4（`design.md` 的 `PlatformKeystore`） | **已修** | `design.md` 全文零命中（D6 现写 `FileKeystore`/`EphemeralKeystore`）；仅 `docs/MODULE_ARCHITECTURE.md:484` 的 §6 规划块保留（RV4 已判不计为发现） |
| RV4-WP3 三条注 a/b/c | **全部已修** | a: `tests/fail_closed.rs:274-277` 明写「只为锚定…本身没有判别力，不要把它当作行为断言」；b: `store.rs:232` 改指 `platform/windows.rs` 的 `unwrap_secret` 文档，且 `platform/windows.rs:28-30` 确有该残余登记；c: `entry.rs:42-47` 摘要已含保留字符/首尾空白与点/Windows 保留设备名 |
| 一致性抽查（RV3/RV4 行 vs 评审报告原文） | **5 处一致、1 处不一致** | RV3-WP2 行 ↔ `reports/rv3-auth.md:8`、RV3-WP3 行 ↔ `rv3-keystore.md:12`、RV3-DOCS 行 ↔ `rv3-docs.md:4`、RV4-WP2/DOCS 行 ↔ `rv4-design.md:3,95-98`、RV4-WP3 行 ↔ `rv4-keystore.md:12,53-59` 均一致；**不一致**：RV3-WP2 行的 P2 单元格列的是 `rv3-docs` 的项，`reports/rv3-auth.md` 的 5 条新发现（`RV3-WP2-F2/F3/F4/F5/F6`）在 `verification.md` 内**零登记**（见 §2.7） |

---

## 1) 隔离与版本声明

- **隔离**：本轮为新建只读 reviewer 子 Agent，未参与实现或实现讨论；**未修改任何文件**（代码、测试、`tasks.md`/`verification.md`/`plan.md`/`design.md`/`progress.md` 均未动），未提交、未合并、未修复。
- **工具限制（必须声明）**：本 Agent 只有 `read`/`grep`/`ls`/`watchdog_diff`，**没有 shell 与 git**。因此我**没有执行**任何命令（`npm run verify`、`cargo test/clippy`、`node scripts/check-*`、mutation 复跑均未由我执行）；凡「PASS / exit 0 / 用例数」一律**读自既有日志文本**，退出码未由我复算。`gitleaks`、`cargo-deny` 本地本来也无等价物。
- **已提交区间未覆盖**：`watchdog_diff` 返回「No working-tree changes against reviewer-launch HEAD `eee409b143b6`」，即我读到的文件内容 = `eee409b` 的内容；但该工具**不含已提交区间 diff**，所以我**无法自行验证**「`eee409b` 只改文档、代码与 `0813ad5` 相同」这一前提，只能按调度者给定的 target revision 作业并下结论。
- **读取的规则与需求**：`openspec/schemas/agentic/roles/reviewer.md`（全文）、`verification.md`（全文）、`plan.md`（Local Checks/Project Verify/Completion/覆盖索引相关段）、`tasks.md`（引用段）、`design.md`（D3/D6/D7）、`docs/IDENTITY_AND_AUTH_CONTRACT.md`（检索）、`reports/rv3-auth.md`/`rv3-docs.md`/`rv3-keystore.md`/`rv4-design.md`/`rv4-keystore.md`、`reports/rv4-*.log`（头部及计数全文）、`crates/identity-keystore/{tests/fail_closed.rs, src/store.rs, src/entry.rs, src/platform/windows.rs, src/ephemeral.rs}`。

## 2) 逐条复核

### 2.1 RV4-P1（P1，阻断）—— **部分收口，残留 `RV5-F1`**

已成立的部分：

| 核对点 | 结果 | 证据 |
| --- | --- | --- |
| 文件是否存在 | **存在** | `openspec/changes/identity-auth-and-keystore/reports/rv4-mutation.log`（`reports/` 全量 `ls` 可见；本次 `read` 成功，非 ENOENT） |
| 头部是否写明 revision | **是** | 第 3 行 `### revision：0813ad5b6421aecd2fbfb87d91740c19836dd988（0813ad5，干净）` |
| 是否含「退化点 → 哪条用例失败」 | **是（4 条）** | mutation 1 `parse_handle` 退回旧校验 → `illegal_references…`（断言文本「标签 "CON" 必须在触碰文件系统之前被判为非法引用」，`9 passed; 1 failed`）；mutation 2 `list()` 去闸门 → `list_is_gated_by_availability_too`；mutation 3 `TempFileGuard` Drop 不删临时文件 → `atomic_tests`（残留 `..blocked.1.tmp-4384-2`）；mutation 4 `is_valid_label` 不拒保留设备名 → 负例 |
| 恢复后是否全绿 | **是** | 末段 `3 / 5 / 6 / 10 / 12 / 0` 全 ok = 36 用例，与 `rv4-pv4.log` 逐项一致 |
| 5 处引用路径是否可解析 | **全部可解析** | `verification.md:37`（Command 列与 Evidence 列）、`:44`、`:84`（mutation 1–3）、`:87`（mutation 1–4）、`plan.md:689`（Coverage Index R86 evidence）——文件均存在于本变更 `reports/` |
| 被击穿用例是否真实存在 | **是** | `tests/fail_closed.rs` 有 `list_is_gated_by_availability_too`、`illegal_references_are_rejected_before_touching_the_filesystem`；`rv4-pv4.log` 有 `atomic_tests::write_atomic_replaces_content_without_leaving_temporaries_on_failure` |
| 机械核对被引用的全部 `reports/*.log|*.md` 是否存在 | **仅 3 个不存在，且全属未开始阶段的将来产物** | 不存在：`reports/du1-main-verify.log`（`plan.md:807`、`tasks.md:56`，任务 6.7）、`reports/rv1-du1.md`（`plan.md:812`、`tasks.md:53`，任务 6.4）、`reports/du1-integration.md`（`plan.md:790`）。`verification.md` 引用的全部日志（`w0-*`、`du1-pv1`、`wp1..wp4-*`、`pv5-windows-dpapi`、`bv-pv2..5`、`rv2-*`、`rv4-*`）**全部存在** |

未收口的部分（→ `RV5-F1`，P1）：工件只含 4 条 mutation，而记录中 5 处仍声称 10 条（详见 §3）。

次要项（P2）：**`rv4-mutation.log` 头部没有 `### 命令：` 行**（其余 7 个 rv4 日志都有），因此无法从工件确认执行的确切测试命令/二进制与 target；mutation 1 与 mutation 4 打印的失败消息**完全相同**（同一条 `assert_eq!`），两者只能靠退化描述区分。

### 2.2 RV4-DOCS-2(a)（日志 revision/命令 + 与 Check 行一致性）—— **部分收口**

- **已修**：`rv4-pv1..pv5.log`、`rv4-linux-clippy.log`、`rv4-selfcheck-rg.log` 头部均有 `### 命令：…` 与 `### revision：0813ad5…（0813ad5，干净）`；`rv4-linux-clippy.log:2` 明含 `cargo clippy --locked -p identity-auth -p identity-keystore --target x86_64-unknown-linux-gnu --all-targets --all-features -- -D warnings` ✓（RV4 的该项要求成立）。
- **revision 与 Check 行是否一致**：`verification.md:37` 的 revision 列写 `0813ad5` ✓ 一致；但**同一行的证据列括注仍写「（全部落在本变更目录，头部含 revision `de61aa5` 与完整命令）」**，与工件头部（`0813ad5`）**不符** → P2（属 RV4-DOCS-2(a) 未完全收口，读者会去找 `de61aa5`）。
- **绑定是否足够 / 是否悬空**：本轮 target 为 `eee409b`，而 `eee409b` 在**整个变更目录零命中**（`grep 0813ad5|eee409b` 仅命中 `verification.md:37`、`:88` 的 `0813ad5`）。因此记录的「本批证据绑定到 `0813ad5`」与「本轮检视对象 `eee409b`」之间**没有留下等价性依据**（例如「`eee409b` 仅改文档、受测文件与 `0813ad5` 逐字节相同」的说明），而我又无 git 访问权去自行证明。结论：**绑定不悬空到「错版本」，但悬空到「无记录的等价性论证」**；同时 `verification.md:88`（RV5 行）的 revision 列写 `0813ad5`，与「RV5 检视 `eee409b`」不符 → P2。
- 建议最小修复：把 `:37` 的括注改为 `0813ad5`（或写「本批日志在 `0813ad5` 上产出；`eee409b` 仅改文档，受测文件与其相同」并给出该判断的依据），并把 `:88` 的 revision 改为本轮实际 target。

### 2.3 RV4-DOCS-2(b)（跨变更同名文件完整路径）—— **部分收口**

- 已改完整路径 ✓：`tasks.md:12`（`openspec/changes/identity-auth-and-keystore/reports/du1-pv1.log`）、`tasks.md:26`、`tasks.md:36`、`tasks.md:56`（`…/reports/du1-main-verify.log`）、`plan.md:807`（`…/du1-pv1.log`、`…/du1-main-verify.log`）、`plan.md:812` 的 `rv1-wp1..4.md`、`tasks.md:31,33,35,37` 的 `rv1-wp1..4.md`。
- **未改 ✗**：`plan.md:812` 末项 `reports/rv1-du1.md`、`tasks.md:53` 的 `reports/rv1-du1.md` 仍是裸路径；仓库根**确实存在**另一变更的同名文件 `reports/rv1-du1.md`（根 `reports/` 实读），歧义与 RV4 判定的同类问题相同 → P2。
- 另注：`plan.md:808`/`:809`/`:810`/`:811`/`:812` 中 `reports/wp*-*.log`、`reports/pv5-windows-dpapi.log` 仍为裸路径，但根 `reports/` 无同名文件，不构成歧义（不计为发现）。

### 2.4 RV4-DOCS-3（自检行逐处一致）—— **计数与行号已修；命令写法/模块归属仍 P2**

我按同一模式在 `eee409b` 的树上独立复现（只读 grep，交替符按 `|`）：

```
crates/identity-auth/src/types.rs:256
crates/identity-keystore/src/entropy.rs:37
crates/identity-keystore/src/entropy.rs:38
crates/identity-keystore/src/store.rs:499,515,523,525,529,530,539,567,570,571,573,581,587   (13 处)
crates/identity-keystore/src/ephemeral.rs:54
合计 17 处（无 panic!/unreachable!/from_der 命中）
```

- **与 `reports/rv4-selfcheck-rg.log` 逐处一致 ✓**（同一文件、同一行号、同一总数）。
- **与 `verification.md:38` 一致 ✓**：17 处、`types.rs:256`、`entropy.rs:37-38`、`ephemeral.rs:54`、`store.rs` 13 处且行号枚举逐条相同、末行总数对应。
- **仍有 P2 差异**：(a) 命令写法三处并存——日志把交替符写成 `[|]`（`grep -rn "unwrap()[|]expect(…`，照此执行将**零命中**，属「工件不自证命令」，RV4 注 5 已登记但未收口）、`verification.md:38` 写 `\|`、`plan.md:801` 写 `rg -n "from_der|unwrap\(|expect\("`；(b) `verification.md:38` 的模块点名仍含 `mode_tests`，而该模式在 `mode_tests`（`store.rs:458-478`）**零命中**，13 处实为 `unix_modes` 7（499/515/523/525/529/530/539）+ `atomic_tests` 6（567/570/571/573/581/587）——RV4-DESIGN-F2 的「`mode_tests` 应删除」未被执行。
- 另：`ephemeral.rs:54` 位于 `with_seed`（非 `#[cfg(test)]`，注释标「仅供测试与本地开发」），记录归入「新建实例的锁不可能中毒」并计入「不在可失败的外部输入路径上」，与源码相符 ✓。

### 2.5 RV4-DOCS 的其余 P2 —— **全部已修**（用例数 / `ephemeral.rs` 行号 / `PlatformKeystore`）

- `verification.md:81`（RV2-WP3 行）已改为「`reports/rv4-pv4.log`（36 用例：3 lib + 5 dpapi + 6 entry_format + 10 fail_closed + 12 keystore）」✓，与 `rv4-pv4.log` 实测（`running 3/5/6/10/12`、`EXIT=0`）逐项吻合 ✓。
- `verification.md:47`（Check Plan Changes 第 3 条）与 `:38` 均已写 `ephemeral.rs:54`（旧值 `:50` 已无）✓。
- `design.md` 全文无 `PlatformKeystore` ✓；仅 `docs/MODULE_ARCHITECTURE.md:484` 的 §6 组合根规划块保留（同块 `SqliteStore::open`/`ProcessAgentBackend` 亦未落地，RV4 已明确不计为发现，本轮沿用该判断）。

### 2.6 RV4-WP3 的三条注 —— **全部已修**

| 注 | 结论 | 证据 |
| --- | --- | --- |
| (a) 无判别力的锚定断言 | **已修** | `crates/identity-keystore/tests/fail_closed.rs:274-277`：「下面这行只为**锚定**上面的穷尽匹配…本身没有判别力…**不要把它当作行为断言**」；同用例另有具判别力断言（`store.sign(...)` 的 `EntryMissing`/`Unavailable` 分类断言、`KeystoreError` 穷尽匹配、注释明写不写 `contains(secret)` 式恒真断言） |
| (b) 残余说明交叉引用 | **已修** | `crates/identity-keystore/src/store.rs:232`：「平台 wrapper 内部…登记在 `platform/windows.rs` 的 `unwrap_secret` 文档里」；`src/platform/windows.rs:28-30` 确有该残余登记（「`windows_dpapi::decrypt_data` 内部还有一份自己的明文缓冲，第三方实现不提供清零钩子…」） |
| (c) `is_valid_label` 函数摘要 | **已修** | `src/entry.rs:42-47`：摘要已含「不含 Windows 保留字符、不以空白或点开头/结尾、不是 Windows 保留设备名（`CON`/`NUL`/`COM1`…）」，与实现（`:48-55` 常量表、`:56+` 判定）一致 |

### 2.7 一致性抽查（记录 vs 评审报告原文）—— **5 处一致，1 处不一致（P2，未登记发现）**

- 一致 ✓：`verification.md:83` ↔ `reports/rv3-auth.md:8`（FAIL，1 条 P1 = `design.md:72`）；`:84` ↔ `reports/rv3-keystore.md:12`（PASS，无 P0/P1，G1 已真修）；`:85` ↔ `reports/rv3-docs.md:4`（FAIL，1 条 P1 + P2 × 5）；`:86` ↔ `reports/rv4-design.md:3,95-98`（FAIL：原 P1 闭环 + 新 P1「mutation 工件不存在」+ P2）；`:87` ↔ `reports/rv4-keystore.md:12,53-59`（PASS + 3 条注）。`reports/rv4-design.md`、`reports/rv4-keystore.md` **均已落盘** ✓。
- 不一致 ✗（P2）：`verification.md:83` 的 **P2 单元格**列的是 `rv3-docs` 的项（父提交 revision、自检计数、`authority.rs` rustdoc 死链），而 `reports/rv3-auth.md:94-127` 的 5 条**新发现**（`RV3-WP2-F2` `mark_approved` 在提交前置位、`-F3` 缓存淘汰语义未覆盖、`-F4` 两处旧断言、`-F5` `due_pairings` 装配契约未写明、`-F6` `complete`/`complete_auth` 双公开入口）在 `verification.md` 内**零登记**（`grep 淘汰最早|先于提交|装配要求|旧断言|两个公开名` 于 `verification.md` 无命中）。其中 `-F6` 的处置能从该行 Resolution 文本（「`complete` 改 `pub(crate)`」）读出并经我核实（`pairing.rs:416` 现为 `pub(crate) fn complete`）✓；但 `-F2` 与 `-F5` 的实质**仍然存在且未见任何口径回写**：`pairing.rs:303` 仍在 `settle` 内、持久化提交之前调用 `mark_approved`，而 `design.md` D3（「内存态只允许比已提交状态更严格…**绝不出现「内存里批准、库里没有信任」**」）与合同 §4.1 都未按 `-F2` 的 (b) 方案补「该标志先于提交置位」的口径，合同内亦检索不到「先于提交/提交失败」相关落点。按 `reports/rv3-auth.md:98` 的定级，这属 **P2**（窄窗口、不产生信任、失败方向是多要求一次落库），因此**不构成本轮阻断项**，但它确实是一组「未登记、未处置」的评审发现。

## 3) 新发现（只报 P0/P1）

**RV5-F1（P1，记录/证据完整性，非代码缺陷）** —— `reports/rv4-mutation.log` 的实际内容与其被引用处的结论互相矛盾。

- **位置**：`openspec/changes/identity-auth-and-keystore/verification.md:37`（判 PASS 的 Check 行 Result 列「**10 个 mutation 全部被对应用例捕获**」，Command 列写明「mutation 检查（见 `reports/rv4-mutation.log`）」）、`:44`（Check Plan Changes 第 2 条：「证据：`reports/rv4-mutation.log`（**10 个 mutation** 全部被对应用例捕获）」）、`:126`（Failures and Retests 的 RV2 段：「`reports/rv4-mutation.log` —— …**10 个 mutation 全部被对应用例捕获**（mutation 1/2：挑战缓存上限与清扫；3/4：`due_pairings`…；5/6：origin/endpoint…；7：模式位常量；8/9/9b：标签校验…；10：R86 两阶段替身）」）；另 `review.md` 形式的行 `:80`（RV2-WP2 Recheck Evidence「`reports/rv4-mutation.log`（mutation 1–6）」）与 `:81`（RV2-WP3 Recheck Evidence「`reports/rv4-mutation.log`（mutation 7–10）」）。
- **证据**：工件 `reports/rv4-mutation.log` 只有 **4** 条 mutation，且头部自述范围是「**RV3/RV4 修复**的可失败性验证」，内容为 `parse_handle`／`list()` 闸门／`TempFileGuard`／`is_valid_label` 设备名——与 `verification.md:130`（RV3 段「mutation 1–3」）一致，与上面 5 处的 10 条编号（挑战缓存、`due_pairings`、parity、模式位、标签写/读路径、R86 两阶段）**不重合**。RV2 轮的 10 条 mutation 原始证据（原 `reports/rv3-mutation.log`）已随 `rv3-*.log` 一并被替换，变更目录与仓库根 `reports/` 现均无任何含 10 条 mutation 输出的日志（`ls` 全量 + `grep -i mutation` 复核）。
- **触发/影响**：任何核对者在 `eee409b`/`0813ad5` 上按 `verification.md:37`/`:44`/`:126` 打开被指工件，都会得到「4 条」而非「10 条」，即**当前记录中「修复可被证伪」这一 RV2/RV3 核心判据的一部分无可解析证据**；这与 RV4 判为 P1 的形态同族（RV4 是文件缺失，如今是文件与主张不符），属 RV4-P1 未完全收口。
- **最小修复（二选一）**：(a) 在 `0813ad5`/`eee409b` 上按 `:126` 的原编号重跑那 10 条 mutation，输出落到一条独立日志（例如 `reports/rv2-mutation.log`）并把 5 处引用改指它（同时保留现有 4 条的 RV3/RV4 日志）；或 (b) 把 `:37`、`:44`、`:126` 与行 `:80`/`:81` 的 mutation 数量与编号改为与 `rv4-mutation.log` 一致的 **4 条**（并把 RV2 轮的 10 条证据已不可重建一事如实登记，如现有 RV4 报告 `reports/rv4-design.md:117` 的口径）。
- **顺带（同一修复面，P2）**：把 `:37` 证据列括注的 `de61aa5` 更正为 `0813ad5`（并补一句 `eee409b` 与受测文件等价的依据）、把 `:88` 的 RV5 行 revision 改为本轮实际 target、把 `plan.md:812`/`tasks.md:53` 的 `reports/rv1-du1.md` 改完整路径、把 `:38` 的模块点名去掉实际零命中的 `mode_tests`。

除 `RV5-F1` 外，无其它 P0/P1；产品代码、用例与 `identity-auth`/`identity-keystore` 的行为面本轮未发现任何缺陷。

## 4) 未覆盖 / 无法确认项

1. **无法读已提交区间 diff**（工具限制）：因此「`eee409b` 只改文档、代码与 `0813ad5` 相同」这一前提我**未能自行验证**；`eee409b` 在变更目录零命中，绑定依据只存在于主 Agent 的口头声明。此项**影响**「证据是否对本轮 target 有效」的判断（我按「文件内容 = `eee409b`、证据在 `0813ad5` 产出」如实报告，不代替证明）。建议在最终验收前把该等价性写进记录（或重跑一轮）。
2. **未执行任何命令**：`npm run verify`、`cargo test/clippy`、`node scripts/check-*`、mutation 复跑均**未由我执行**；`rv4-pv1.log`（`doc links OK: 369/3727`、`crate boundaries OK: 10 个 crate`、`Totals: 9 passed, 0 failed`、`EXIT=0`）、`rv4-pv2.log`（`EXIT=0`）、`rv4-pv5.log`（DPAPI 5 用例）等结论均**来自既有日志文本**；我为核对计数只逐条读了 `rv4-pv3.log`（15/19/26/2/9/5/2=78 ✓）与 `rv4-pv4.log`（3/5/6/10/12=36，`EXIT=0` ✓）。
3. **Linux 运行时行为仍未验证**（沿用既有限制）：`rv4-linux-clippy.log` 只有 `Checking/Finished` + `EXIT=0`，不链接、不执行；`unix_modes` 的 0700/0600 真实断言、`platform/unsupported.rs` 的失败关闭运行时行为只能在 CI 的 Linux runner 上真跑。
4. **CI-only 判定**：`.gitleaks.toml` 的规则命中能力（`keywords = ["QUNQS"]`）与 `cargo-deny`/advisory 判定本地无等价物，未在本地验证。
5. **`docs/MODULE_ARCHITECTURE.md:484` 的 `PlatformKeystore::open`** 仍在（§6 组合根未来 `app` 示意块，同块其它名字亦未落地）。RV4 已明确不计为发现，本轮沿用（登记供调度者决定是否一并改名）。
6. **未做新的全量检视**：按本轮刻意最小的范围，我只复核 RV4 列出的 ID 与其证据链；契约/实现的全量正确性、E2E（`not-applicable`）、DU1 集成与主分支验证（任务 6.x 未开始、Merge History 仍为「尚未开始集成」）均不在本轮范围。
7. **本轮结论只对应 target revision `eee409b`**；不更新任务状态、不代表整个变更可归档。
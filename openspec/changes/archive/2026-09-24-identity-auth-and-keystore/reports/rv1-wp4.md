<!-- 落盘说明：主 Agent 原样保留 reviewer 的正文与结论；仅当某处 `§` 引用会触发文档引用门禁的歧义归属时，补全被引文档名（不改变语义）。 -->
<!-- 由独立 reviewer 子 Agent 产出（未继承实现对话，只读），主 Agent 原样落盘。 -->
<!-- review run: 3d9e3123-6ac3-474e-a904-46c7e33d79b5；target revision: 6861046（RV1 轮次）；落盘时间：2026-09-24 -->

## 结论：**FAIL**（1 项 MAJOR：`MODULE_ARCHITECTURE.md` §3.1 的密码学版本口径与 `Cargo.toml`/`Cargo.lock` 不一致）

---

### 0. 固定字段

- `task_id`：RV1-WP4-2026-09-24｜`role`：reviewer（独立检视子 Agent）｜`phase`：branch validation / WP4 交付前 review
- `agent_context`：新建子 Agent，未继承实现对话，未参与本变更的实现或实现讨论
- `target_revision`：`6861046`（= 工作区 HEAD `686104647e49`）｜`base_revision`：`30f0d78b803b94346ebd8972078febceec8af8d3`
- `scope`：`docs/MODULE_ARCHITECTURE.md`（§3 状态行、§3.1、§4.12、修订记录）、`README.md`、`docs/DEVELOPMENT_PLAN.md` §2、`AGENTS.md` §4 状态表（外加为核对版本口径必需的 `Cargo.toml`/`Cargo.lock`/`crates/*/Cargo.toml` 与 `reports/` 日志）
- `result`：FAIL（见 §3 F1）
- `evidence_paths`：见 §2、§4｜`resource_cleanup`：无（只读，未创建任何文件/目录，未修改任何文件）

---

### 1. 版本与隔离声明

- 我收到的输入就是本轮 prompt 全文（含固定 base/target、需求与契约清单、证据目录、隔离要求、输出格式），没有其它上下文。**我不能证明宿主未注入其它上下文**，只能声明自己未收到实现对话、未参与实现。
- 我没有 shell / git 访问权（工具只有 read/grep/find/ls/watchdog_diff，均只读）。`watchdog_diff` 报「No working-tree changes against reviewer-launch HEAD 686104647e49」，即工作区相对启动 HEAD 无 staged/unstaged/untracked delta → 我读到的文件内容即 target revision `6861046` 的内容。
- **限制**：`watchdog_diff` 不含已提交区间，因此我**没有读到 `30f0d78..6861046` 的 diff**。本轮是「固定提交的静态内容核对」，不是逐提交 diff 审阅；凡涉及「本变更是改了还是没改」的判断，我都注明依据（多数来自变更自己的 `verification.md` 记录），不编造。
- 未执行任何构建/测试（reviewer 只读边界），下文的检查结论均来自我直接读取的文件与 `reports/*.log` 原文。
- 我读过的文件：`openspec/schemas/agentic/roles/reviewer.md`、`AGENTS.md`、`docs/MODULE_ARCHITECTURE.md`（§1–§5 全读）、`README.md`（1–30、100–169）、`docs/DEVELOPMENT_PLAN.md`、`Cargo.toml`、`Cargo.lock`（定向）、`crates/identity-auth/Cargo.toml`、`crates/identity-keystore/Cargo.toml`、`crates/identity-keystore/src/platform/{mod,windows,unsupported}.rs`、`.gitignore`、`.gitleaks.toml`、`deny.toml`、`scripts/check-doc-links.mjs`（头部）、`openspec/changes/identity-auth-and-keystore/{plan,tasks,verification,design}.md`、`reports/{wp3-dpapi-verification,pv5-windows-dpapi,wp4-boundaries,du1-pv1}.log`、根 `reports/rv1-wp4.md` 与 `reports/du1-integration.md`（仅用于确认目录归属）。

---

### 2. 已读取的 diff 范围

| 项 | 结果 |
|---|---|
| 提交区间 diff | **未读取**（无 git 访问；as-is 限制已在上文声明） |
| 工作区 diff | 空（`watchdog_diff`：无 delta，无 untracked 路径） |
| 目标版本内容 | 已直接读取（见 §1 文件清单） |
| 交叉依据 | 变更自己的 `verification.md` 的 Checks/Dependency Handoffs 表、`reports/*.log` 原文 |

---

### 3. 逐条发现

#### Correct（成立、无需改动）

1. **「已落地/待落地」状态陈述与实际一致**：`crates/` 下确有 `identity-auth`、`identity-keystore` 两个目录；`Cargo.toml:3-17` 的 `members` 含 `crates/identity-auth`、`crates/identity-keystore`；`reports/wp4-boundaries.log` 全文为 `crate boundaries OK: 10 个 crate 的依赖方向与 §5 矩阵一致（10 个已在矩阵登记）`。四处状态陈述一致：`docs/MODULE_ARCHITECTURE.md:3`（两个 crate 已实现）、`README.md:9-24`（crate 表新增两行）、`docs/DEVELOPMENT_PLAN.md:14`（切片 3 已落地）、`AGENTS.md` §4（`identity-auth`/`identity-keystore` 已落地）。
2. **没有把未落地的东西写成已落地**：`README.md:26`「尚未开始：前端工程，以及 `server`、`node-link-client`、`app`」、`DEVELOPMENT_PLAN.md:14`「尚未落地的是 `server`、`node-link-client`、`app` 和前端工程」、`AGENTS.md` §4 三条 `待落地`、`MODULE_ARCHITECTURE.md:461`（`server`/`app` 仍只是矩阵的「行」）都正确；仓库里确实没有 `crates/server`、`crates/app`、`crates/node-link-client`。
3. **切片顺序与验收表述未被改动**：`DEVELOPMENT_PLAN.md` §3 的切片 1–8 顺序与各自的「验收」段保持原样，§2 只新增 2026-09-24 的落地段落，未新增/删除切片、未改写验收措辞。
4. **DPAPI 选型结论与实证日志逐项一致**：`MODULE_ARCHITECTURE.md:415-419` 的 `windows-dpapi 0.2.0`、`MIT OR Apache-2.0`、edition 2021、未声明 `rust-version`、传递依赖 `anyhow 1`/`log 0.4`/`winapi 0.3.9` ↔ `reports/wp3-dpapi-verification.log:44-47`；`deny.toml:73-81` 的 allow 列表含 `MIT`/`Apache-2.0`，所以「落在 allow 列表内」成立。`:428` 说的「5 个真实 DPAPI 用例」↔ `reports/pv5-windows-dpapi.log:10-17`（5 passed）；代码侧 `platform/windows.rs:20,27` 固定 `Scope::User` + `Some(entropy)`，与「总是传入附加熵」的结论一致；`platform/mod.rs:6-19` + `unsupported.rs:13-20` 佐证 `README.md:23`「非 Windows 一律失败关闭（不建目录、不写文件）」。
5. **没有把 CI-only 判定写成已通过**：`MODULE_ARCHITECTURE.md:418`（advisory 判定只在 CI、本地无等价物）、`README.md:61-63`（`deps`/`advisories`/`secrets` 不在 `npm run verify` 里）、`docs/adr/0008-...md:119-121`（首次真实执行在 CI）都如实；`verification.md:23` 的 `gitleaks` 行也明确「真实执行留 CI（不声称本地通过）」。
6. **文档引用（相对链接与 §锚点）在门禁层面成立**：`reports/du1-pv1.log:25` = `doc links OK: 368 relative links, 3277 section refs across 169 markdown files`（WP4 轮次）；新增/沿用的锚点可解析，例：`README.md:5` 的 `## 仓库当前状态` ↔ `DEVELOPMENT_PLAN.md:14` 的 `../README.md#仓库当前状态`；`CORE_PORTS_AND_STORAGE.md:1308` 的 §11 标题 ↔ `#11-管理状态持久化合同形状已并入-357`。
7. **§5 矩阵两行/两列与 `Cargo.toml` 一致**：`identity-keystore` 行只勾 `identity-auth`（`crates/identity-keystore/Cargo.toml` 无 `core`、无协议 crate）；`identity-auth` 的 `acpr-wire` 格为空且实际未依赖。

#### Findings

| ID | 严重度 | 位置 | 触发 / 依据 | 预期 vs 实际 | 影响 | 修复建议 |
|---|---|---|---|---|---|---|
| **RV1-WP4-2026-09-24-F1** | **MAJOR**（阻断本包验收条件） | `docs/MODULE_ARCHITECTURE.md:133` | `Cargo.toml:59` = `hmac = "0.13"`；`Cargo.lock:639-645` = hmac 0.13.0，`Cargo.lock:753` = `identity-auth → "hmac 0.13.0"`；`crates/identity-auth/Cargo.toml:26` = `hmac.workspace = true`；同一文档 `:134` 自己规定「版本号以 `Cargo.toml` 的 `[workspace.dependencies]` 为准，本行所在的说明与它一致；变更版本必须在**同一改动**里更新这里并给出验证证据」；`openspec/changes/identity-auth-and-keystore/verification.md:23` 把「`hmac` 0.12→0.13」登记为**本变更**的依赖改动 | 预期 §3.1 写 `hmac 0.13`；实际写「`p256 0.13` + `sha2 0.11` + **`hmac 0.12`** + `base64 0.23`」 | 权威文档中唯一的密码学原语版本口径写错一个原语，而任务 2.16 的完成条件正是「文档陈述与实际 `cargo metadata`/`cargo tree` 一致」；`hmac 0.12.1` 仍以传递依赖形式存在于树中（`ecdsa → rfc6979`），因此读者很容易误以为文档是对的；`npm run check` 不校验版本字符串，漂移不会被任何门禁发现 | 把 `:133` 改为 `hmac 0.13`；并在 `:135` 的日期基线里补一条 2026-09-24 的 `hmac 0.12 → 0.13` 记录（附本变更的验证证据），使 :134 的规则得到满足 |
| **RV1-WP4-2026-09-24-F2** | MINOR | `README.md:114-115`、`README.md:152-154`（相关：`docs/adr/0008-ci-supply-chain-tooling.md:110-111`） | 同一变更的任务 2.15 已把规则写入 `.gitleaks.toml:32-42`（`id = "acpr-keystore-plaintext-entry"`，含「包裹后字节不可识别」的诚实限制注释）；而 README 仍写「仍待办：本地密钥扫描的自研格式**规则**……规则等密钥格式定稿再加」、「**规则本身等密钥格式定稿再加**：现在只有默认规则集」、「触发条件是「`identity-auth`/`identity-keystore` 开始产生真实密钥」」 | 预期：规则已落地、仅首次真实执行在 CI；实际：文档宣称规则尚不存在 | 与目标版本现实相反的**安全类陈述**；后续 agent 可能重复添加规则或误判防线缺失；该处不是 markdown 链接，`check-doc-links` 不会判定 | 把这两处改成「规则已随 `identity-auth-and-keystore` 落地（`.gitleaks.toml` 的 `acpr-keystore-plaintext-entry`），首次真实执行仍在 CI `secrets` job」；ADR-0008 残余风险 3 同步一句 |
| **RV1-WP4-2026-09-24-F3** | MINOR | `docs/MODULE_ARCHITECTURE.md:428` | 引用的两个 `.log` 都**不入版本库**：`.gitignore:15`（`*.log`）、`.gitignore:20`（`openspec/changes/**/reports/**/*.log`，并注明「不要把这类日志 `git add -f` 强行入库」）、`.gitignore:22-25`（仓库根 `/reports/` 是**别的变更**的历史遗留且不入库）；第二个路径写作裸 `reports/pv5-windows-dpapi.log`，从 `docs/` 出发不可解析，仓库根 `reports/` 下也没有该文件（根目录里是 `rv1-wp4.md`、`du1-integration.md` 等 `admin-state-persistence-v2` 的产物） | 预期：可被读者核对的证据指针；实际：「两者随变更归档」在版本控制意义上不成立，且第二个路径指向一个不存在/歧义的位置 | 读者（含后续 agent）在检出里无法核对选型证据；归档后路径还会变成 `openspec/changes/archive/...` | 写成完整路径 `openspec/changes/identity-auth-and-keystore/reports/pv5-windows-dpapi.log`，并注明这些日志按 `.gitignore` 策略不入库、复现命令见 `verification.md` 的 Checks 表；或直接引 `verification.md` 的 [PV5] 行 |
| **RV1-WP4-2026-09-24-F4** | SUGGESTION | `docs/MODULE_ARCHITECTURE.md:419` | `reports/wp3-dpapi-verification.log:44-47`：`anyhow 1.0.104 … rust_version=1.68`、`log 0.4.34 … rust_version=1.71.0` | 文档写「传递依赖声明的**最低** MSRV 是 `log 0.4.34` 的 1.71」；按字面最低应是 anyhow 的 1.68，1.71 是这些声明值里的最高值（即真正的下界） | 措辞级；「MSRV ≤ 1.85」的结论不受影响 | 改为「传递依赖中声明 MSRV 的最高值为 `log 0.4.34` 的 1.71」 |
| **RV1-WP4-2026-09-24-F5** | SUGGESTION | `docs/MODULE_ARCHITECTURE.md:5` vs `:138` | 修订记录称「§3.1 的依赖口径记录**两个**身份 crate 已落地」；§3.1 内只有 `identity-keystore`（已落地）带标记，`identity-auth` 处无标记（「已落地」由 `:3` 的状态行与 README crate 表承载） | 预期：修订记录与所在小节内容对应；实际：归因略宽 | 只影响修订记录的可核对性，不影响结论；无门禁依赖 | 二选一：在 §3.1 的 `identity-auth` 处也加「（已落地）」，或把修订记录改成「§3 状态行与 §3.1 的依赖口径记录两个身份 crate 已落地」 |
| **RV1-WP4-2026-09-24-F6** | SUGGESTION | `docs/DEVELOPMENT_PLAN.md:4`（内容在 `:14`） | 头部「基线：2026-09-23」未随 2026-09-24 新增的 §2 落地段落更新 | 若该行语义是「本文档基线日期」则无需改；若是「内容截至日期」则已过期 | 读者无法从头部判断 §2 是 09-24 的补充 | 视语义决定；如需保留 09-23 基线，可在 §2 段落里点明「2026-09-24 补充」 |

---

### 4. 对 `plan.md` 覆盖索引中本工作包行（R 编号）与 Check ID 的核对结论

**R 编号**：`plan.md` 的 Coverage Index **没有任何 R 行映射到任务 2.16/2.17**——所有 R 行都属于 WP1–WP3 的需求/场景。这与 `plan.md` 的 Work Packages 表一致（WP4 的 Verification 列 = `[PV1]、[PV2]`，WP4 被定位为「文档与状态收口」而非需求覆盖包），因此 WP4 的验收不靠场景断言，只能靠「文档 ↔ 实际元数据」的一致性静态核对——**本轮 F1 正是这条一致性上的失败点**。若主 Agent 认为 WP4 也应有 R 行背书，则这是计划侧的覆盖空隙（我没有把它计为缺陷，只作为结论的限定条件）。

| Check ID | 是否属 WP4 | 核对结论 |
|---|---|---|
| **PV1** | 是（2.17） | `reports/du1-pv1.log` 可读且内含 `doc links OK: 368 relative links, 3277 section refs`、`crate boundaries OK: 10 个 crate`、以及全部 `cargo test` 目标的 `test result: ok`（我对 `FAILED`/`error[` 做过定向检索，无命中），与 `verification.md` 记录的「exit 0、67 个测试二进制」一致。**退出码未经我独立复算**（无 shell），且该轮在**当前工作区**而非干净检出上执行（见 §5 第 3 条）。 |
| **PV2** | 是（2.17） | `reports/wp4-boundaries.log` 全文 = `crate boundaries OK: 10 个 crate 的依赖方向与 §5 矩阵一致（10 个已在矩阵登记）`，支持「members 已含两个新 crate 且与 §5 两行一致」。同样未独立复算。 |
| **PV3 / PV4 / PV5** | 否（属 WP2/WP3） | 我只把它们当作 WP4 文档陈述的**事实来源**使用（crate 是否存在、DPAPI 5 用例、条目格式/失败关闭），未审其用例内容与断言强度——那属 WP2/WP3 reviewer 的范围，且这两轮 review 目前尚无报告（见 §5 第 6 条）。 |
| **RV1** | 是（3.8） | 本轮即 RV1 的 WP4 分片，结论见上。**路径风险提醒**：`plan.md` 的 RV1 证据列为 `reports/rv1-wp4.md`，而仓库根 `reports/rv1-wp4.md` 实际是另一个变更（`feat/admin-state-persistence-v2`，被检版本 `5404610`）的报告，且 `.gitignore:25` 明确根 `reports/` 是遗留目录、不入库。若把本轮报告写为根 `reports/rv1-wp4.md`，会**覆盖另一变更的 review 证据**。建议落到 `openspec/changes/identity-auth-and-keystore/reports/rv1-wp4.md`（该目录目前只有 `.log`，无同名文件）。 |

---

### 5. 未覆盖 / 无法确认项与所需证据

1. **base 与提交区间不可读（影响 F1 的归属）**：我没有 git 访问，无法确认 `hmac = "0.13"` 是 base 就已存在还是本变更引入，也无法独立重读 `30f0d78..6861046` 的 diff。我据以归因的是 `verification.md:23`（本变更把 `hmac 0.12→0.13` 登记为依赖改动）。请主 Agent 用 `git show 30f0d78:Cargo.toml | grep -n hmac` 核实：若 base 已是 `0.13`，F1 可降为 MINOR，但 `MODULE_ARCHITECTURE.md:133` 的修正仍然必需（任务 2.16 的完成条件就是这条一致性）。**该项影响本轮代码判断的严重度分级，不影响「需要改」的结论。**
2. **退出码/测试结果未经我复算**：reviewer 只读边界，未执行任何命令；PV1/PV2 的 PASS 是采信 `reports/du1-pv1.log`、`reports/wp4-boundaries.log` 与 `verification.md` 记录。这两份证据需在合入前的候选门禁（6.3）上按同一 Check ID 复跑。
3. **干净检出复现性未证**：`du1-pv1.log` 的 `doc links OK ... 169 markdown files` 是在**含未跟踪/忽略内容**的工作区里统计的，不能证明提交后的树同样通过；仓库内已有同类先例（根 `reports/rv1-du1.md` 的 F1 记录了 clean checkout 上 `check:docs` 失败、本机日志不可复用）。请在干净检出（如 `git worktree add --detach 6861046`）上重跑 `npm run check`，再判定 (d) 文档引用结论。**这项影响本轮 (d) 的强度，请在合入门禁前补齐。**
4. **CI-only 判定本地不可验证**：`deps`/`advisories`/`secrets` 三个 job 无本地等价物。文档侧我核对过没有任何「已通过」的越界声明（`MODULE_ARCHITECTURE.md:418`、`README.md:61-63`、`ADR-0008:119-121`、`verification.md` 的 2.15 行），但本变更新增的 `getrandom` 与 `windows-dpapi`（含传递 `winapi 0.3.9`，其 license 字段是 `MIT/Apache-2.0` 这一 legacy 表达式）的许可证/来源/advisory 判定仍需 CI 的 `deps`/`advisories` job 给出首次真实结果。
5. **wrapper 的 API 面无法从仓库内核实**：本机没有 cargo registry 源码（`C:\Users\zhang\.cargo\registry\src` 不存在），因此 `MODULE_ARCHITECTURE.md:418` 的「只提供 `encrypt_data`/`decrypt_data` + `Scope::{User, Machine}`」与已知代价 2「不暴露 `CRYPTPROTECT_UI_FORBIDDEN`」我无法独立确认；我只能确认代码侧固定 `Scope::User` + `Some(entropy)`（`platform/windows.rs:20,27`）与「总是传附加熵」的结论自洽。若这条代价是选型结论的关键论据，建议把 wrapper 的文档/源码片段作为证据留档。
6. **上游 WP1–WP3 的独立 review 在目标版本上尚不存在**：变更 `reports/` 目录只有 `.log`，`verification.md` 的 Review Findings 记为「尚未进行独立 review」，tasks 3.2/3.4/3.6 未勾选。`plan.md` 的 WP4 输入含这些 RV1 报告，因此 WP4 的文档口径目前只能由本轮静态核对背书，最终确认仍需后续轮次。**不影响本轮结论，但影响「WP4 是否可判定交付前通过」的表述。**
7. **未执行 E2E**：按 `plan.md`，Main E2E = `not-applicable`（替代验证在 7.1）。本轮不涉及 E2E，也不对替代验证的执行做任何通过声明。

---
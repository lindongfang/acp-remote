# RV2-WP4 · 复核检视报告（WP4 修复后复核，作用于候选 90c816a）

> 由 reviewer 子 Agent 输出工件原样落盘（运行 2a4b09ed），主 Agent 未改动结论。
> 复核对象：RV1-WP4 的 F1–F4 与抽样 #12（修复见 2.27 / b4f7f81）。
> 编辑说明（主 Agent 加入，披露而非改写结论）：为通过仓库内 markdown 的 `check:docs` 引用扫描，本转录版把个别「文件名 + 章节号」混在同一子句的表格单元拆开表述（语义不变）；报告的全部结论、严重度与修复建议均为 reviewer 原文。

## result: **PASS**

## Review

- **Correct（已核实为好的部分）**
  - **F1 已解决**：`crates/app/src/daemon.rs:1340`（测试 1）与 `:1385`（测试 2）都在 `tasks.cancel_all(...)` **之后**对冻结三元组断言（`:1341-1345` `frozen.1 == frozen.0*2`、`:1346` `frozen.2 == 0`、`:1348-1352` 100 ms 后三元组不变；`:1386-1387` 测试 2 两条）。实现侧确认快照语义真的成立：合并窗口循环（`daemon.rs:671-716`）只在 `tokio::select!` 顶部检查 `task_cancel.notified()` / `ticker.tick()`，`shutdown.is_requested()` 检查在每轮**开始前**，因此一轮「枚举 + 逐会话 pump」在两次取消检查之间原子完成 ⇒ 取消返回后计数必然轮对齐。覆盖强度无损失：`while spy.scans() < 2`（运转证明）、`while spy.failures() < 2`（失败不中止）、`scans <= elapsed/20+2`（非热循环）、`failures == 0` / `failures == scans`、取消后冻结（新增把 `failures` 也纳入冻结）。
  - **F2 已解决**：`docs/MODULE_ARCHITECTURE.md:382` 现为「broker 的 delta 合并窗口（`storage.flush_interval_ms`；由组合根定时器枚举非终态会话并逐个驱动 `Broker::pump`，`CORE_PORTS_AND_STORAGE.md` §6 第 10 条）」，`:383` 现为「停接入层（排空在途连接至宽限上限）→ 取消周期任务与信号监听 → 停 Agent → `wal_checkpoint(TRUNCATE)` → 清理 endpoint/释放锁（与 §7.1 第 4 条、§12.1 一致）」。逐字比对权威：`CORE_PORTS_AND_STORAGE.md:1220`（§7.1 第 4 条）顺序完全一致；实现 `daemon.rs:1013-1064` 一致（`ingress_stopped` → `drain_connections` → `cancel_all`/`audit_writer.close` → `shutdown_all`+`drop(host)` → `composition.close()`=checkpoint → `cleanup_endpoint`/`remove_record`/`drop(lock)`）；「取消周期任务与信号监听」措辞也准确（`daemon.rs:458` `signal_watcher` 与 `:511` `maintenance`、`:671` `merge_window` 都在 `OwnedTasks` 内）。
  - **F3 已解决**：`docs/MODULE_ARCHITECTURE.md:147` 新增 `rpassword 7.5`（实际 7.5.4）条目，含单许可/`rust-version`/传递依赖 `rtoolbox 0.0.6` 残余风险/「不进入 `core` 闭包」；`crates/app/Cargo.toml:10-11` 陈旧注释已改为当前事实。可核部分一致：`Cargo.lock:1354-1356` rpassword `7.5.4`、`:1385-1386` rtoolbox `0.0.6`；`deny.toml:88` allow 含 `"Apache-2.0"`；`rpassword` 在 `crates/` 内只被 `app` 引用（`app/Cargo.toml:49`、`app/src/cli/input.rs`）。根 `Cargo.toml:97`「见 WP4b 报告与 §3.1」现指向真实存在的小节与 `reports/wp4b-handoff.md`。
  - **F4 已解决**：target `plan.md` 的 R10/R11/R59–R72（`:105/113/482/490/498/505/513/520/528/536/543/551/559/566/574/582`，共 17 处）全部为 `reports/wp4b-cli.log`，且 `reports/wp4b-cli.log` 实读存在（同目录 `ls` 亦列出 `wp4-app-daemon.log`、`wp425.log`、`wp426.log`，PV4 行 `:676` 新增的两条证据都存在）。`plan.md` 内 `wp4-app-cli.log` 零命中。主 Agent 还把 `tasks.md:24-26` 的完成条件一并改成 `wp4b-cli.log`；残留命中只在 `verification.md:154`、`tasks.md:46`、`reports/rv1-wp4.md`（历史记录/任务书自述），符合 2.27 §5.1 的判断。
  - **#12 已解决**：`crates/app/src/config.rs:1083-1104` 新增用例断言单行、无源码片段、点明键名、≤ `ERROR_DETAIL_MAX_CHARS`；RED/GREEN 有实证（`reports/wp427.log`：还原为原样多行诊断 → EXIT 101，panic 在「诊断必须是单行」；修好谓词前又 RED 一次，panic 在「诊断不得回显配置源码片段：2 | endpoing = "wp427-marker"`；最终 50 lib + 1 + 11 + 10 全绿）。
  - **未发现断言被弱化**：`rv-input-rv2.diff.log` 中 `config.rs` 只有一个新增测试（无删改），`daemon.rs` 的改动是把断言**搬家并加强**（`frozen` 三元组把 `failures` 也纳入、取消后复查含 `failures`）；`managed_sections_and_unknown_keys_are_rejected` 未被改动，仍断言 `message.contains("data_diry")`（grep 目标文件确认），全仓也没有任何测试固定过旧诊断文本（`TOML parse error` 全仓只有 `config.rs` 的 doc comment 命中）。
  - **无回归**：`is_toml_locator_line` 唯一调用点是 `config.rs:468`；错误码映射按变体判定（`cli.rs:891-899`）；`compose.rs:436-441` 的 `seed_invalid_reason` 虽然也按 `detail` 子串分类，但其输入来自 `SeedProfile::to_profile`→`AgentProfile::try_new`（`compose.rs:381-388`），不是 `toml_error_detail`，故不受影响。
- **Fixed**：无（本任务为只读复核，未做任何编辑）。
- **Finding**：2 条 P2（report-only，均不阻断；见下 RV2-WP4-F1/F2）。
- **Merge verdict: OK with notes**（P0/P1：无）。

---

## 1. 逐项复核结论

| 发现 | 状态 | target 位置 | 判定依据（读 target 全文，非仅 diff） |
|---|---|---|---|
| RV1-WP4-F1（合并窗口 spy 非快照一致读） | **已解决** | `daemon.rs:1340-1352`、`:1385-1387` | 断言全部移到 `cancel_all` 之后并对同一 `frozen` 三元组比较；循环（`:671-716`）的取消检查只在轮边界 ⇒ 冻结读自洽；原断言集（`pumps==scans*2`、`failures==0`/`==scans`、间隔上界、取消后冻结、失败不中止）无一条丢失 |
| RV1-WP4-F2（§4.10 关闭顺序与「存储批量刷盘」措辞） | **已解决**（一处同节措辞残留见 RV2-WP4-F1） | 该 `MODULE_ARCHITECTURE.md` 处（`382-383` 行） | 与关闭顺序条目（`CORE_PORTS_AND_STORAGE.md:1220`）逐字同序、与 `SECURITY_DESIGN.md:358`、与实现 `daemon.rs:1013-1064` 一致；合并窗口语义与 §6 第 10 条（`:725`）、`CONFIG_REFERENCE.md:110` 一致 |
| RV1-WP4-F3（rpassword 未登记 / Cargo.toml 陈旧注释） | **已解决** | `MODULE_ARCHITECTURE.md:147`；`crates/app/Cargo.toml:10-11` | 登记条目含用途、许可、MSRV、传递依赖残余风险、闭包边界与「只被 app 使用」（可核部分见上文 Correct）；许可/MSRV 的**离线不可核**部分列入未覆盖声明 |
| RV1-WP4-F4（plan.md 17 处不存在的证据路径） | **已解决** | `plan.md:105…582`（17 处）+ `:676` | 全部为 `reports/wp4b-cli.log`（文件实读存在）；`plan.md` 内旧名零命中；PV4 行新增 `wp425.log`/`wp426.log` 均存在 |
| 抽样 #12（`toml_error_detail` 断言强度） | **已解决**（连带实现修正已独立复核，见 §2；长度上限子断言强度不足见 RV2-WP4-F2） | `config.rs:1083-1104` + `:460-493` | 新增用例覆盖「单行 / 无源码片段 / 点明键名 / ≤240」，且 RED/GREEN 有日志实证 |

## 2. 重点复核：#12 的连带实现改动（`is_toml_locator_line`，`config.rs:484-490`）

**① 是否真达成「一行、不含源码片段、≤长度上限」— 是（对已观测形态）。**
谓词（`:463-471`）取首个「非空 + 不缩进 + 非 `TOML parse error` 开头 + 非定位行」的行；`is_toml_locator_line` 判定「首个 `|` 之前的部分 trim 后为空或全为 ASCII 数字」。对 `wp427.log` 实测的真实形态（`TOML parse error at line 2, column 1` / `  |` / `2 | endpoing = "wp427-marker"` / `  | ^^^^^^^^` / `unknown field …`）逐行套用：1 被前缀排除、2/4 被「不缩进」排除、3 被新助手排除、5 命中 ⇒ detail = ``unknown field `endpoing`, expected `endpoint` ``，单行、不含源码片段、远小于 240。且该修复确有契约依据：`config.rs:481-483` doc comment 与 `ConfigError::Invalid`（`:44-48`「不含配置值的简短说明」）此前被真实违反（WP427 §4 的 RED 证据）。

**② 过度过滤 / 其他片段形态 — 未找到反例；两类无法离线证伪的形态列为残余风险。**
- 过度过滤：助手只能命中「行首（可带空白）是数字后紧跟 `|`」的行；定位行本来就被「不缩进」排除（`  |`、`  | ^^^`），所以新增排除的实际对象只有 `N | <源码>`。正文行要到「以数字开头且首个 `|` 之前全是数字」才会被误伤，toml 的诊断正文（`unknown field …`、`invalid key`、`invalid string`…）不具该形状；反例尝试（含 `|` 的值、以 `|` 结尾的正文如 ``unexpected `|` ``——其首个 `|` 前的 head 是 ``unexpected ` ``）均不被误伤。**没有反例。** 且「过度过滤到全体」会被用例的第三条断言抓住（此时回落到 `text.lines().next()`＝位置行，`detail.contains("endpoing")` 失败）。
- Windows 路径 / 多字节字符：源码行 `2 | data_dir = "C:\…"`、`2 | 中文 = 1` 的首个 `|` 仍在定位符上 ⇒ head 为数字，仍被排除；值里含 `|` 同理（`split_once` 取第一个 `|`）。
- 无法离线证伪的两类（**未验证，不是发现**）：(a) 多行 span 的续行若以非空白开头且**不带** `N |` 定位符，则不会被排除；(b) `serde`/`toml` 的**正文**本身可能回显配置值（如类型错误的 `invalid type: string "x", expected i64`），这类泄漏不经源码片段行，当前断言与助手都不覆盖。仓库内没有任何此类输出的实证（全仓 `invalid type: ` / `TOML parse error` 只命中 `config.rs` 的 doc comment），故按「无证据不立为发现」处理，改列为需调度者执行的一次性探针（§5）。

**③ 是否有固定旧文本的断言被顺手改掉 — 无。** diff 中 `config.rs` 只有新增（`:1078-1104`），`daemon.rs` 只有断言搬家/加强；`managed_sections_and_unknown_keys_are_rejected` 仍断言 `contains("data_diry")`/`contains("unknown_section")`（目标文件实读），且修复后该文本仍成立（正文含键名）。全仓无测试固定过 `2 | …` 形式的旧诊断。

## 3. 回归复核（本轮改动 vs 其余代码）

| 关注点 | 结论 | 证据 |
|---|---|---|
| `is_toml_locator_line` 影响面 | 无扩散 | 唯一调用点 `config.rs:468`；`ERROR_DETAIL_MAX_CHARS` 只在 `:473-475` 与该新用例使用 |
| 错误码 | 未变 | `cli.rs:891-899` 按变体判定（`Invalid`/`ManagedSectionInStartupConfig` → `invalid_params`），不读 detail 文本 |
| 其他 detail 消费者 | 未受影响的另一生产者 | `compose.rs:436-441` 的 `seed_invalid_reason` 只吃 `into_seed`/`to_profile` 产生的 detail |
| 关闭序列 / 周期任务 | 未变（该 diff 未触碰 `daemon.rs` 非测试代码） | `daemon.rs:1013-1064`、`:511`、`:671` |
| spy 快照改动的覆盖强度 | 保留且略增 | 见 §1 F1 行；取消后复查现含 `failures` |

## 4. 抽样断言（与本次修复相关，判定「实现退化时能否失败」）

| # | 位置 | 断言 | 反例能力 |
|---:|---|---|---|
| 1 | `daemon.rs:1341-1345` | `frozen.1 == frozen.0 * 2` | 强：漏 pump 某会话、提前退出、取消吞掉半轮都会失败 |
| 2 | `daemon.rs:1346` | `frozen.2 == 0` | 强（无失败会话时任何失败计数都失败） |
| 3 | `daemon.rs:1348-1352` | 100 ms 后三元组仍等于 `frozen` | 强：取消后仍在 tick（枚举或 pump 或失败）即失败 |
| 4 | `daemon.rs:1386` / `:1387` | `pumps == scans*2` / `failures == scans` | 强：需「每轮枚举整批、失败恰好一次」同时成立 |
| 5 | `daemon.rs:1333-1336` | `scans <= elapsed/20 + 2` | 强：间隔被忽略退化成热循环即失败（配合 `scans ≥ 2` 前置等待） |
| 6 | `config.rs:1091` | `!detail.contains('\n')` | 强，已有实证（`wp427.log` RED EXIT 101） |
| 7 | `config.rs:1092-1095` | 不含 `endpoing = ` 与 `wp427-marker` | 强，已有实证（RED 中间态 panic 正指向该断言） |
| 8 | `config.rs:1096-1099` | `detail.contains("endpoing")` | 中–强：反过度过滤/反回落吞判据（断言「必须点明键」） |
| 9 | `config.rs:1100-1104` | 长度 ≤ `ERROR_DETAIL_MAX_CHARS`(240) | **弱**：本输入正文约 40 字符，删掉截断逻辑也照过；仅能捕获「把多行/整段拼进 detail」这类退化 ⇒ RV2-WP4-F2 |

## 5. 新发现

### RV2-WP4-F1 — §4.10 状态行仍把合并窗口叫「刷盘」（P2，report-only）
- 位置：`docs/MODULE_ARCHITECTURE.md:375`（§4.10 的 `[现状]` 行：「…周期任务（清理/刷盘）装配…」）。
- 证据：同节 `:382` 已按 F2 修正为「broker 的 delta 合并窗口…（§6 第 10 条）」，`CONFIG_REFERENCE.md:110` 明写该键「**不是**存储刷盘间隔」，`config.rs:170` 同口径；实现里周期任务只有 `maintenance`（清理，`daemon.rs:511`）与 `merge_window`（`:671`）两类，`storage-sqlite` 不参与周期写。全仓 `刷盘` 其余命中（`CONFIG_REFERENCE.md:110`、`config.rs:170`、`daemon.rs:660`、`daemon_lifecycle.rs:585`）均指关闭时的 `wal_checkpoint`（§12.1「刷新存储」），语义正确——只有 `:375` 是 F2 同类的残留误名。
- 最小修复：把 `:375` 的「周期任务（清理/刷盘）」改为「周期任务（清理/合并窗口）」。不影响任何门禁；不阻断。

### RV2-WP4-F2 — 长度上限断言在所选输入下无区分能力（P2，report-only）
- 位置：`crates/app/src/config.rs:1100-1104`。
- 证据：输入 `[daemon.local_admin]\nendpoing = "wp427-marker"\n` 的正文（``unknown field `endpoing`, expected `endpoint` ``）远短于 240，故该断言对「删掉 `:473-475` 的截断」这类退化恒真；全仓唯一涉及 `ERROR_DETAIL_MAX_CHARS` 的实现点就是 `:473-475`，无第二条用例覆盖截断分支（`wp427-handoff.md` §3 的 72 passed 里也只有这一条新用例）。
- 最小修复：补一条输入让正文必然超限（例如 `[daemon.local_admin]\n` + 300 字符的未知键名 + ` = 1\n`，正文会回显该键名与 expected 列表），并断言 `detail.chars().count() == ERROR_DETAIL_MAX_CHARS + 1` 且以 `'…'` 结尾。（回显键名不是配置**值**，与 `ConfigError::Invalid` 口径不冲突。）

## 6. 需调度者执行

| # | 命令 | 目的 |
|---:|---|---|
| 1 | `cargo test --locked -p app --all-features` @ `90c816a`（候选 PV1 的一部分，`reports/candidate-verify.log`） | 确认 lib 50（含新增 1 条）/audit_export 1/cli_commands 11/daemon_lifecycle 10、0 failed；我读到的最新实证 pin 在 `b4f7f81`（`wp427.log`），候选自身证据以集成 Agent 的日志为准 |
| 2 | `npm run check`（或 `npm run verify`） | 若采纳 RV2-WP4-F1 的文本改动，必须重跑合同门禁（doc-links 对 `§`/相对链接敏感） |
| 3 | 一次性探针（用完即删、不入库）：在 `crates/app/src/config.rs` 的测试模块临时打印 `Config::from_toml` 对 `[storage]\nflush_interval_ms = "abc"\n`、`a = """\nunterminated\n`、`[daemon]\ndata_dir = "C:\\Users\\x"\n` 三种输入的 `ConfigError::Invalid{detail}` | 证伪/证实 §2-② 的两类未覆盖形态（正文值回显；无定位符的续行）。若确认「正文回显配置值」，则 `ConfigError::Invalid`「不含配置值」口径仍有缺口，需单独裁定（属新发现，不在本轮范围） |
| 4 | CI `deps` / `advisories` / `secrets` | `rpassword 7.5.4`、`rtoolbox 0.0.6` 的许可与 advisory（本地无等价物；§3.1 已如实登记该残余风险） |
| 5 | Linux CI `checks` | `#[cfg(unix)]` 用例（`an_unusable_endpoint_refuses_start_without_degrading`）本机不编译 |

## 7. 未覆盖 / 无法确认（明确声明）

1. 我是结构只读子 Agent（无 shell），**未运行任何 `git`/`cargo`/`npm`**；结论来自 target `90c816a` 的文件全文 + 调度者提供的 `rv-input-rv2.diff.log` / `wp427.log` / `wp427-handoff.md` / `wp4wp5-final-verify.log`。`watchdog_diff` 报告 worktree 相对 `90c816a` 干净。
2. `rv-input-rv2.diff.log` 是**限定路径** diff（`docs`、`README.md`、`scripts`、`crates/app`、`plan.md`），不含 `openspec/changes/**`（tasks.md/verification.md）、根 `Cargo.toml`/`Cargo.lock` 的差异。这些路径的候选改动我未作为检视对象（只按需读了 target 版本片段用于判定 F4/#12 的登记一致性，不据此给范围外结论）。
3. `wp427.log` 的 app 测试证据 pin 在 `b4f7f81`（2.27 第五个提交），候选是 `90c816a`；范围 diff 的全部内容与 2.27 自述的修复一一对应、无多余代码改动，故判定证据适用——但这不是对「90c816a 上实跑」的独立复算。
4. `rpassword 7.5.4` 的许可与 `rust-version = 1.85`、`rtoolbox 0.0.6` 的许可/advisory **离线不可核**（仓库不含 registry 元数据/上游清单）；我只能确认 `Cargo.lock` 的版本号与 §3.1 登记一致。
5. `config.rs:1100-1104` 的截断分支未被任何用例执行（见 RV2-WP4-F2）；`is_toml_locator_line` 只在「未知键」这一种 toml 输出形态下被实证（`wp427.log`），其余形态属推断。
6. RV1 §5 的未覆盖项依旧成立且我未改变结论：合并窗口的端到端效果（本切片无 owned 会话）、真 TTY 凭据录入、真实设备 claim 配对、平台 keystore 路径、`#[cfg(unix)]/#[cfg(windows)]` 跨平台证据。
7. `plan.md:673` 的 PV1 证据含 `…/reports/du1-main-verify.log`，该文件当前只存在于**仓库根** `reports/`（变更目录内尚无）；它对应尚未开始的 `tasks.md:67`（6.7 主分支回归），属计划内将来产物，与仓库既往 review 对同类项的判定口径一致（`archive/2026-09-24-identity-auth-and-keystore/reports/rv1-wp2.md:138`：属计划项、可接受），**不作为发现**。

```yaml
handoff_index:
  task_id: "3.8 复核轮 / 6.4 候选检视"
  role: reviewer
  phase: work-package recheck
  stage: RV1 → RV2 复核（WP4 修复后，作用于候选提交）
  work_package: WP4（+ 任务 2.27）
  base_revision: 7e37f9b
  target_revision: 90c816a6f88fb7925c84fbca269b1c946745e9e3
  evidence_type: REVIEW
  evidence_id: RV2
  report_path: openspec/changes/daemon-cli-and-local-admin/reports/rv2-wp4.md
  result: PASS
  evidence_status: NEW
  applicability_basis: >
    RV2-WP4：对 WP4 修复轮候选 90c816a 的独立复核（base 7e37f9b）。逐项复核 RV1-WP4-F1/F2/F3/F4 与抽样 #12：
    全部「已解决」（F2 留一处同节措辞残留、#12 留一处断言强度不足，均 P2 report-only）。依据 target 版本文件全文
    （crates/app/src/{config.rs,daemon.rs}、crates/app/Cargo.toml、docs/MODULE_ARCHITECTURE.md、docs/CONFIG_REFERENCE.md、
    plan.md、tasks.md、verification.md、Cargo.lock、deny.toml）与工件 reports/rv-input-rv2.diff.log、reports/wp427.log、
    reports/wp427-handoff.md、reports/wp4wp5-final-verify.log；watchdog_diff 确认 worktree 固定在 90c816a 且干净。
    独立判定 #12 的连带实现改动（is_toml_locator_line）真达成「一行、不含源码片段、≤240」且未过度过滤（无反例）、
    未弱化任何既有断言、未污染其他调用点/错误码；新增 2 条 P2 报告级发现，无 P0/P1，故 result=PASS、merge verdict=OK with notes。
  source_evidence: NOT_APPLICABLE
```
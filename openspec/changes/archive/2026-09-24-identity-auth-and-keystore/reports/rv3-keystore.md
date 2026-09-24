<!-- 落盘说明：主 Agent 原样保留 reviewer 的正文与结论；仅当某处 `§` 引用会触发文档引用门禁的歧义归属时，补全被引文档名（不改变语义）。 -->
<!-- 由独立 reviewer 子 Agent 产出（RV3 轮，target revision 5ed869f），主 Agent 原样落盘。 -->
<!-- review run: rv3-keystore（见 RV3 工作流 b80022df-27b1-49e1-8695-face55f9a776）；落盘时间：2026-09-24 -->
## 结论

**PASS（无阻断项）**。我对 target revision `5ed869f` 的 WP3 范围（`crates/identity-keystore/**` + `.gitleaks.toml`）做了独立静态复核：**RV2-WP3 的唯一 P1（G1）已真修复**，RV2 的 8 条 P2 有 6 条实质闭合、2 条部分闭合；未发现新的 P0/P1。本轮新发现全部是 P2（报告项），其中 3 条与「记录/证据的可核对性」有关（与上一轮 RV2-WP2 F3 / RV2-WP3 G9 同一族，但严重度远低于上一轮：日志这次确实落在变更目录，只是 revision 标注与内容不符）。

**与上一轮 P1 的逐条对照**

| 上一轮问题 | 本轮判定 | 关键证据 |
| --- | --- | --- |
| **RV2-WP3 G1（P1）**：`failed_reference_commit_keeps_the_old_entry_resolvable` 的「引用提交失败」是恒假死分支，且记录声称已修 | **已修复** | `tests/keystore.rs:196-298` 用 `ReferenceHolder` 替身重写为真实两阶段；全 crate 检索 `reference_commit_failed` 零命中；阶段①断言提交失败/引用不切换/旧条目可读可签，阶段②断言切换成功后才回收且回收后 `EntryMissing`；`reports/rv3-mutation.log` 的 mutation 10 证明阶段①真被执行（把替身改成「不失败」即用例失败，来自既有日志） |
| **RV2-WP2 F1/F2（P1）**（范围外，仅确认无回归） | 未复核实现（属 WP2 范围） | `reports/rv3-pv3.log` 的存在与 verification.md 的 RV3 行仅作记录核对，我不代为判定 |
| **RV2-DOCS F1/F2/F3（P1）** | 未复核（属 WP1 文档轨道） | 只顺带确认 `docs/IDENTITY_AND_AUTH_CONTRACT.md` 不属我的 target；见 §6 的跨范围提示 |

---

## 1) 隔离与版本声明

| 字段 | 内容 |
| --- | --- |
| task_id / Review ID | RV3-WP3（本轮）；recheck 轮次（RV1 → RV2 → RV3） |
| role | 独立代码检视子 Agent，**只读**：未修改/新建任何文件（含 tasks/verification/progress），未提交、未合并、未修复 |
| phase | 交付前独立复核（plan W4 / tasks 3.6，该任务仍为 `[ ]`） |
| agent_context | 新建子 Agent，未继承实现对话；输入仅为调度者任务文本、`openspec/schemas/agentic/roles/reviewer.md`（全文）、`AGENTS.md`（项目指令）与仓库文件。我只能声明自己收到的输入与限制，**不能证明宿主未注入其它上下文** |
| Repository | `D:\Project\acp-remote`，分支 `feat/identity-auth-and-keystore` |
| Base / Target Revision | 上一轮 target = `fe52694`（RV2）、`6861046`（RV1）；**本轮 target = `5ed869f`** |
| scope | `crates/identity-keystore/**`、`.gitleaks.toml`（含其与 `identity-auth/src/port.rs`、`entry`/`store` 公开面的交互、以及本 crate 的变更资产记录） |
| changes | 无（只读检视，未产出任何改动） |
| checks | **我没有执行任何命令**；所有 cargo/npm/gitleaks 结果均引自既有日志（见 §2），并逐条标注「来自既有日志」 |
| issues | 新发现 F1–F5（P2）；上一轮 G1–G8 复核结论见 §3 |
| result | **PASS**（无 P0/P1；P2 × 5） |
| evidence_paths | `crates/identity-keystore/{src,tests}/**`、`.gitleaks.toml`、`Cargo.toml`/`Cargo.lock`、`openspec/changes/identity-auth-and-keystore/{verification.md,plan.md,tasks.md,design.md,specs/platform-keystore/spec.md}`、`reports/{rv1-wp3.md,rv2-keystore.md,rv3-pv1.log,rv3-pv2.log,rv3-pv4.log,rv3-pv5.log,rv3-linux-clippy.log,rv3-mutation.log,rv3-selfcheck-rg.log}` |
| resource_cleanup | 不适用（只读；未创建临时目录/worktree，未写 `target/`，报告由调度者按 runtime 指定路径落盘） |

**版本可信度与限制（如实声明）**

- `watchdog_diff(stat)` 返回「No working-tree changes against reviewer-launch HEAD `5ed869f453be`」→ 我读到的文件内容**就是 `5ed869f` 的内容**（无 staged/unstaged 改动可污染结论）。
- **限制**：`watchdog_diff` 不含 committed range，我也没有 Git 访问能力，因此**没有读过 `fe52694..5ed869f` 的逐提交 diff**，无法区分「哪些行属 RV3 修复轮新增」；下文结论只对 `5ed869f` 的文件现状负责。任务允许的 `git show --stat 5ed869f` 我也无法执行（无 shell），因此文件清单由目录枚举获得。
- 证据日志**来自既有日志**（`rv3-*.log`），我**没有**执行 `npm run verify`、`cargo test`、`cargo clippy` 或 `gitleaks`。凡引用均写明来源。

---

## 2) 已读取范围

**代码（全文通读）**：`src/{lib,entry,store,error,entropy,ephemeral}.rs`、`src/platform/{mod,windows,unsupported}.rs`；`Cargo.toml`（本 crate + 根 workspace 的 `p256`/`getrandom` 登记）、`Cargo.lock` 的 `elliptic-curve`/`p256`/`zeroize` 条目。
**测试（全文通读）**：`tests/{keystore,fail_closed,entry_format,dpapi}.rs`、`tests/support/mod.rs`；**`tests/permissions.rs` 已不存在**（目录枚举确认）。
**配置**：`.gitleaks.toml`（全文）。
**需求与记录**：`specs/platform-keystore/spec.md`（全文）、`design.md` D6/D7/D8/D9 与 Risks、`plan.md`（Coverage Index R72–R90、Local Checks 自检行 801、Project Verify 805–812、Work Packages 729）、`tasks.md`（2.10–2.15、3.5、3.6）、`verification.md`（全文）、`AGENTS.md` §3/§4/§7/§10/§12（项目指令内置）。
**既有日志（读取，未复算）**：`rv3-pv1.log`（头部 + 关键行：`crate boundaries OK: 10 个 crate`、`doc links OK: 369 relative links, 3589 section refs`、identity-keystore 各目标 2/5/6/8/12 = 33 用例全 ok）、`rv3-pv2.log`（2 行 + EXIT=0）、`rv3-pv4.log`（全文）、`rv3-pv5.log`（头部 + 5 用例 ok）、`rv3-linux-clippy.log`（全文 4 行）、`rv3-mutation.log`（全文）、`rv3-selfcheck-rg.log`（全文）、上一轮报告 `rv1-wp3.md`、`rv2-keystore.md`（全文，作为复核基线）。
**未读**：`rv3-pv3.log`（WP2 全量）、`docs/**` 其余章节、`schemas/`、`fixtures/`、WP2/WP4 实现细节。

---

## 3) 上一轮发现的逐条复核（编号对齐）

严重度换算：P0 = CRITICAL，P1 = MAJOR，P2 = MINOR。

### G1（RV2-WP3 P1）R86 死分支 + 记录声称已修 → **已修复**

- **事实**：`tests/keystore.rs:196-236` 新增最小替身 `ReferenceHolder { current, committed }`，`:233-298` 的两阶段用例：
  - 阶段①（提交失败）：`let commit = references.commit(&fresh, true); assert!(commit.is_err(), "替身必须模拟提交失败");` → `assert_eq!(references.current().as_deref(), Some(old.as_str()), "提交失败时引用不得切换")` → `public_key(&old)` ok → `sign(&old, transcript)` 恰 64 字节 → `public_key(&fresh)` ok（注释明确「它是孤儿，不是有效身份」）→ `assert_ne!(old, fresh)`。
  - 阶段②（提交成功）：`commit(&fresh,false).expect("提交成功后必须切换")` → `commit_now(&fresh)` → `assert_eq!(current, fresh)` → `store.delete(&old).await.expect("引用切换成功后回收旧条目")` → `public_key(&fresh)` ok → `matches!(public_key(&old), Err(EntryMissing))` → `list(...) == ["primary-next"]`。
- **无恒假分支**：全 crate 检索 `reference_commit_failed` **零命中**（`grep` 结果仅剩 `store.rs:474` 对已删除 `tests/permissions.rs` 的说明性注释）。`:282` 仍有一个 `if references.current().as_deref() == Some(fresh.as_str())`，但它是**恒真**（紧邻 `:281` 的 `assert_eq!` 已断言同一条件），**分支体会执行**，且若引用未切换则 `:291-295` 的 `EntryMissing` 与 `:296` 的 `list` 断言会失败——因此不是空转。
- **可失败性**：`reports/rv3-mutation.log`（**来自既有日志**）的 mutation 10「R86 替身不再模拟提交失败 → 用例必须失败」实际失败于 `替身必须模拟提交失败`，证明阶段①的断言真的被执行；恢复后复跑 12 passed。
- **残余（不阻断，F5/§4）**：该用例走 `keystore()` = `FileKeystore::new`，在非 Windows 上于 `let Ok(old) = … else { return }` 处早退，因此 R86 仍只在 Windows 上真实执行（与 RV2 相同，但这次记录不再宣称别的）；替身字段 `committed` 只写不读（`:219` 写入，无读取点），替身的语义靠 `commit_now` 完成。
- **记录**：`verification.md` 的 RV2-WP3 行已改为「**当轮声称的「R86 死分支改为真实两阶段替换语义」并不成立**……RV2 才真正改掉」，并在 RV1-WP3 行如实登记这条自我纠正 → **不再存在「与文件内容不符的已修声明」**。

### G2（RV2-WP3 P2）`tests/permissions.rs` 空跑 + Unix 模式位无断言 → **已修复**

- `tests/permissions.rs` **已删除**（目录枚举：只剩 `dpapi.rs`/`entry_format.rs`/`fail_closed.rs`/`keystore.rs`/`support/`）。
- `src/store.rs:444-468` `#[cfg(test)] mod mode_tests`：**任何平台**断言 `PRIVATE_DIRECTORY_MODE == 0o700`、`PRIVATE_FILE_MODE == 0o600` 且 `& 0o077 == 0` → 真能失败（`reports/rv3-mutation.log` mutation 7 把它退化成 `0o755` 后用例失败，**来自既有日志**）。
- `src/store.rs:470-567` `#[cfg(all(test, unix))] mod unix_modes`：两个用例直接调用**私有**的 `create_private_directory`（`:503`）与 `write_atomic`（`:511`、`:549-550`），断言真实 `0700`/`0600`、读回内容、**「已存在目录也被收紧」**（先把目录设成 `0o755` 再重复调用并断言回到 `0700`，`:515-522`）、成功路径不留 `.tmp-` 残留（`:525-534`）。
- **「在 Linux CI 上是否真会执行」——我的独立判断：会**。理由（源码级）：这两个用例只触达 `create_private_directory`（`fs::create_dir_all` + `set_permissions`）与 `write_atomic`（`File::create` + `set_permissions` + `write_all` + `sync_all` + `rename`），**完全不经过 `wrap_secret`/`unwrap_secret`**（`src/platform/unsupported.rs` 的必然失败与它们无关）；它们又是 `src/` 内的 lib 单测模块，随 `cargo test -p identity-keystore` 的 `unittests src/lib.rs` 目标在 Linux 上编译并执行。因此 RV2「Unix 权限路径在任何构建里都不可达」的结论已被这段改动实质推翻。
- **本机（只做 clippy、不链接）我能确认到什么程度**：只能确认「Linux 目标可编译、无 lint 告警」——`reports/rv3-linux-clippy.log`（**来自既有日志**）只有 `Checking identity-auth`/`Checking identity-keystore` + `EXIT=0`。该命令若带 `--all-targets`（记录如此写）会以 `cfg(test)` 编译 lib，因此 `unix_modes` 参与类型检查；但**编译≠执行**，且该日志**没有回显命令与 target triple**（见 F5(c)）。执行证据只能来自 CI 的 Linux runner，我无法在本机产生。
- 残余（不阻断）：`unix_modes` 的第二条用例名 `write_atomic_replaces_content_without_leaving_temporaries_on_failure` 的「失败」场景是「父目录不存在」（`store.rs:558-560`），该场景**按构造不可能留下临时文件**，所以 RV1-F4 指出的「`write_all`/`sync_all` 失败残留临时文件」路径仍无执行证据（实现侧有 `TempFileGuard`，`store.rs:337-345`/`:413-427`，属静态可判成立）→ F2。

### G3（RV2-WP3 P2）明文中间副本未清零 → **主要部分已修，残余已部分登记**

- `store.rs:218-232`：`let mut secret = unwrap_secret(...)?;` → 长度不符时先 `fill(0)` 再返回 `Corrupt`（`:222-225`），正常路径 `let copy = SecretBytes::new(&secret); secret.fill(0); drop(secret);`（`:226-231`）→ RV2 指出的「`Vec<u8>` 未清零即 drop」**已消除**。
- `ephemeral.rs:23-25`：`struct Entry { secret: SecretBytes }`（不再持 `derive(Clone)` 的明文 `Vec<u8>`）；`get()` 用 `SecretBytes::new(entry.secret.as_bytes())`（`:82`）按需复制，副本随 `SecretBytes::drop` 清零（`identity-auth/src/port.rs:128-132`）。
- **仍无法清零的残余**（实现与记录对比）：
  1. 平台 wrapper 内部缓冲（`windows-dpapi::decrypt_data` 自己的明文 `Vec`）——`src/platform/windows.rs:28-36` **已如实登记**；
  2. **未登记**：本 crate 自己的**栈上 `[u8; 32]` 副本**——`store.rs:246-248`（`candidate`，失败轮次未清零）、`store.rs:265`（`generate` 的 `scalar`）、`ephemeral.rs:51/59`（`with_seed(seed)` 形参）、`ephemeral.rs:90`（`generate` 的 `scalar`）。`p256::SecretKey` 的内部分量不在残余之列：`Cargo.lock` 显示 `elliptic-curve 0.13.8` 的依赖含 `zeroize`，即该 feature 在本 workspace 工程中被启用，`SecretKey` 析构会清零。→ F4（P2）。
- 记录真实性：`verification.md` 残余清单写了「(b) 平台 wrapper 内部的明文缓冲无法清零」，**没写**栈副本 → 记录口径与实现一致但不完整（不构成过度声明）。

### G4（RV2-WP3 P2）`list()` 未过闸门 / `with_seed` 无调用方 → **部分修复**

- `list()` **已过闸门**：`store.rs:140-143` 第一段即 `self.availability.require().map_err(|_| StoreError::PlatformUnavailable)?`，不可用平台返回 `PlatformUnavailable` 而非空列表；文档也写明了这一点（`:136-139`）。
- **但该闸门没有任何用例**：全测试目录检索 `PlatformUnavailable` 零命中（只有 `dpapi.rs`/`entry_format.rs` 的 `Corrupt`、`HeaderInvalid` 断言）→ 新增的失败关闭语义无回归护栏 → F1（P2）。
- `entry_path` 仍是 `pub` 且**签名上不强制校验**（`store.rs:122-134`），注释明确「调用方必须先校验标签」，但未提大小写折叠（F5(d)）。`lib.rs:9-11` 的自述已收窄为「**所有**入口（生成/读公钥/签名/删除/秘密读写）」——枚举的是 7 个端口入口，措辞自洽；`list` 的约束单列在它自己的文档注释里。
- `with_seed` 仍是**公开且无任何调用方**（全仓库检索：只有定义 `ephemeral.rs:51` 与文档链接 `:8`）——文档注释已如实写「仅供测试与本地开发」，但按 `AGENTS.md` §7「保持公开 API 最小」，RV2 建议的两种收口（`#[cfg(test)]`/feature 或补真实用例）都未做 → F5(e)（P2，遗留）。

### G5（RV2-WP3 P2）标签校验边界与 `decode` 负例 → **基本修复，有一处不完整**

- `entry.rs:46-64`：`is_valid_label` 拒 `/<`、`\`、`\0`、控制字符，**新增**拒 `WINDOWS_RESERVED = ['<','>',':','"','|','?','*','/','\\']`、首尾空格/点（`!label.starts_with([' ', '.']) && !label.ends_with([' ', '.'])`）。
- `EntryHeader::new`（`entry.rs:139-146`）与 `decode`（`:181-183`）共用同一份校验；`decode` 侧**有了可失败的负例**：`tests/entry_format.rs:150-166` 手工构造字节把标签首字节改成 `/` 后断言 `decode(...) == Err(StoreError::HeaderInvalid)`；`reports/rv3-mutation.log` 的 mutation 8/9/9b（**来自既有日志**）分别证明「去掉保留字符检查」「decode 跳过校验」都会让对应用例失败。
- 大小写折叠说明**如实**：`entry.rs:55-58` 明确「本 crate 不做大小写归一……调用方必须自行保证同一用途下大小写不冲突」——但**指向的落点不存在**（`store.rs` 的 `entry_path` 文档没有大小写内容）→ F3（P2）。
- **不完整处**：(a) Windows **保留设备名**（`CON`/`PRN`/`AUX`/`NUL`/`COM1`/`LPT1`…）不在拒绝集合，而 `entry.rs:52-53` 的注释正是以「否则会在 `File::create` 处报 `Io`（映射为 `Unavailable`），把「标签非法」误报成「后端不可用」」为理由——该理由对设备名同样成立（F6，P2，Windows-only，未在本机执行验证）；(b) `parse_handle`（`store.rs:243-259`）仍是**第三份、更弱**的标签形状检查（只查非空/长度/分隔符/控制字符），保留字符与首尾空白/点放行 → 读/删路径上这类标签会落到 `fs` 层，Windows 上 `Io → Unavailable`、Linux 上 `NotFound → EntryMissing`，而不是 `EntryInvalid` → F4b（P2）。

### G6（RV2-WP3 P2）`.gitleaks.toml` 的 `keywords` 可能使规则永不评估 → **已改，本地无法证实命中**

- `.gitleaks.toml` 现在 `keywords = ["QUNQS",]`，与 regex `(?i)QUNQS[A-Za-z0-9+_-]{16,}={0,2}` **一致**；注释显式登记了 RV2-WP3 G6 与理由（「写 `ACPK` 会让规则永不评估」），并**如实登记未验证项**：「本机无 gitleaks，因此「规则确实会在合成样例上命中」这一点**没有**本地证据，只能由 CI 的 `secrets` job 承担」。
- 前缀正确性我做了独立核算：`base64("ACPK") = "QUNQSw=="`，`QUNQS` 确为其前 5 字符，且规则要求的 16+ 后续字符对任何 43 字节起的真实条目（magic 4 + 版本 2 + 用途 1 + 长度 2 + 标签 ≥1 + 盐 32 + 包裹 ≥1）都满足。
- 诚实限制段仍完整：只覆盖 base64/base64url 编码形态；原始二进制与被 DPAPI 包裹的密文**无法**模式识别；误提交主要靠路径约定与轮换流程 → 无过度声明。
- **残余不确定（无法本地判定）**：若 gitleaks 的 `keywords` 前置过滤是「先把分片转小写再 `contains`」，则大写关键字 `QUNQS` 会再次永不命中（= G6 原形复发）。我无法本地验证，也不据此判缺陷；建议以低成本对冲，见 F5(f)。

### G7（RV2-WP3 P2）durability 口径 → **已如实登记**

- `store.rs:9-14` 明确「`write_atomic` 只对文件本身做 `sync_all`，**不**对父目录 fsync……「原子替换」保证「要么旧内容、要么新内容，不会半截」，**不**保证掉电后新条目一定可见。keystore 语义没有崩溃持久性要求；需要时应在此处补父目录 fsync（unix）或 write-through 替换（Windows）」。与实现（`:397-409`）一致，未夸大。**无新发现**。

### G8（RV2-WP3 P2）弱/不可失败断言 → **已实质改进，但残留一条恒真断言**

- `tests/fail_closed.rs:168-217` 已改为：可用平台断言 `Err(EntryMissing)`、不可用平台断言 `Err(Unavailable)`（`:186-201`），并用 `fn label(&KeystoreError) -> &'static str`（`:205-214`）的**穷尽匹配**做类型层结构性断言——新增带载荷变体会导致匹配不再穷尽/元数不符而**编译失败**，这是有判别力的（RV2 建议的方向）。
- **残留**：`:216` `assert!(!label(&KeystoreError::EntryMissing).contains("super-secret-provider-token"))` —— `label()` 对固定输入返回字面量 `"entry_missing"`，该断言**结构上不可能失败**，与上一轮被批评的那条同型（只是换了个位置）→ F1（P2）。

### 任务第 8 项：7 个入口的 `require()` 闸门 → **无回归、无绕过路径**

- 7 个入口的第一条语句仍是 `self.availability.require()?`：`generate` `store.rs:262-263`、`public_key` `:270-271`、`sign` `:287`（其前仅 `use` 与空行）、`delete` `:301-303`、`get_secret` `:309-314`、`put_secret` `:323-329`、`delete_secret` `:335-336`；`list()` 也过闸门（`:140-143`）。
- 无绕过路径：`read_entry`/`write_entry`/`read_secret`/`remove_entry`/`generate_scalar`/`handle_of`/`parse_handle`/`create_private_directory`/`write_atomic` 全部私有，只能经上述入口到达（`grep` 调用点确认）；`wrap_secret`/`unwrap_secret` 只被 `write_entry`/`read_secret` 调用。
- 反向风险仍被排除：即使组合根误用 `with_availability(..., Platform)`，Linux 上 `write_entry` 会在 `wrap_secret` 处得到 `PlatformUnavailable → Unavailable`（`store.rs:202-206` + `platform/unsupported.rs:17-19`），**不会**降级为明文或内存实现；`fail_closed.rs:146-166` 也把这条反向断言保留为「声明可用也不能无条件成功」。

---

## 4) 新发现（P0/P1/P2）

严重度换算：P2 = MINOR（非阻断，报告项）。**本轮无 P0/P1**。

**F1（P2）`tests/fail_closed.rs:216` 仍是结构上不可能失败的断言（G8 残留）**
- 事实：`fn label(&KeystoreError) -> &'static str` 对固定变体返回字面量 `"entry_missing"`，紧接着 `assert!(!label(&KeystoreError::EntryMissing).contains("super-secret-provider-token"))` 恒为真。
- 影响：给读者「错误文本已断言不含明文」的超额印象（正是上一轮 G8 要消除的那类），而同用例真正有判别力的两层（具体分类断言 + 穷尽匹配）已经成立，故无实际功能风险。
- 建议（最小）：删掉 `:216`，或把它替换成有判别力的形式（例如断言 `StoreError::Io` 只携带 `ErrorKind` 文本：对同一 `ErrorKind` 构造两次并断言相等且不含路径）。

**F2（P2）`src/store.rs:538-560` 的「失败不留临时文件」用例没覆盖会留临时文件的失败路径**
- 事实：用例名 `write_atomic_replaces_content_without_leaving_temporaries_on_failure`，其失败场景是「父目录不存在」（`fs::File::create` 于不存在的目录），此路径按构造不可能产生临时文件；`assert!(!root.join("no-such-dir").exists())`（`:560`）也因此近乎不可失败（`File::create` 不会建目录）。真正需要护栏的是 `write_all`/`sync_all` 失败（ENOSPC/EIO）时的残留，RV1-F4 的那条路径至今无执行证据。
- 影响：实现侧已由 `TempFileGuard`（`:413-427`，任意 `?` 返回路径都会删临时文件）静态成立，因此不是缺陷；但「失败清理」的回归护栏缺一条。
- 建议：把失败场景改成「目标路径已存在且为**目录**」或「临时名指向已存在目录」，使 `rename`/`sync_all` 真失败并断言目录内无 `.tmp-` 残留；同时把 `:560` 的断言换成对 `directory` 内容的断言。

**F3（P2）`src/entry.rs:57` 的交叉引用落点不存在**
- 事实：`is_valid_label` 注释写「调用方必须自行保证标签在同一用途下大小写不冲突（**见 `store.rs` 的 `entry_path` 说明**）」，但 `store.rs:122-134` 的 `entry_path` 文档只讲「必须先校验标签 / 目录穿越」，**没有**大小写折叠内容。
- 影响：读者按指引找不到约束；约束本身（不做大小写归一、由调用方保证）**是如实陈述**，无功能影响。
- 建议：把该句改为自解释（直接写出「本 crate 不做大小写归一……」），或在 `entry_path` 文档补一句大小写语义并保留反向链接。

**F4（P2）清零残余登记不完整 + 与 `parse_handle` 的标签校验强度不一致**
- F4a（清零）：`store.rs:246-248`（`candidate`）、`store.rs:265`（`scalar`）、`ephemeral.rs:51/59`（`seed`）、`ephemeral.rs:90`（`scalar`）在栈上留未清零的 32 字节私钥副本；`src/store.rs:226-231` 与 `platform/windows.rs:28-36` 只登记了「平台 wrapper 内部缓冲」这一种残余。
- F4b（标签）：`store.rs:243-259` 的 `parse_handle` 只校验非空/≤128/分隔符/控制字符，**不复用** `is_valid_label`，于是 `public_key`/`sign`/`delete`/`get_secret`/`delete_secret` 收到含 `< > : " | ? *`、首尾空格或点的标签时，会在 `fs` 层失败：Windows 上多为 `Io → Unavailable`、Linux 上 `NotFound → EntryMissing`，均非更准确的 `EntryInvalid`——这正是 G5 注释（`entry.rs:52-53`）声称要避免的误报，只是被移到了读/删入口。
- 影响：均为错误分类/纵深防御层面，无秘密外泄、无路径穿越（分隔符仍被拒）、无当前生产调用方传入此类标签（`handle_of` 只产出经 `EntryHeader::new` 校验过的标签）。→ 两条都建议以最小改动闭合。
- 建议：(a) 在残余登记处补一句栈副本，或让 `generate_scalar` 返回 `SecretBytes`；(b) `parse_handle` 内改为 `if !is_valid_label(label) { return Err(KeystoreError::EntryInvalid) }`（同一份校验，`entry.rs` 已 `pub`），并把「decode/`new`/`parse_handle` 共用」写进注释。

**F5（P2）记录/证据的可核对性（4 处，均不影响本轮代码判定）**
- (a) **日志 revision 标注与内容不符**：`reports/rv3-pv2.log`/`rv3-pv4.log`/`rv3-pv5.log`/`rv3-selfcheck-rg.log` 头部都写 `revision fe526941614dbd5fbe365802593c93a3cc49438a`（= RV2 的 target），`rv3-pv1.log` 额外写了「工作区有未提交改动，见 git status」。这些日志的内容**只能**来自修复后的工作树（`mode_tests`/`unix_modes`/`ReferenceHolder` 在 `fe52694` 的提交内容里不存在），我按行号/用例名交叉核对确认它们与 `5ed869f` 的文件内容一致（如 `store.rs:487/503/511/513/517/518/527/546/549/550/552`、`ephemeral.rs:53` 与 `rv3-selfcheck-rg.log` 逐条吻合）。结论：证据内容可信，但**日志头部本身不能自证在 `5ed869f` 上执行**。
- (b) **自检行的计数与其引用的日志不一致**：`verification.md` 自检行写「identity-keystore 3 处：`entropy.rs:37-38`…、`ephemeral.rs:50`…」，而它引用的 `rv3-selfcheck-rg.log` 列出 **14 处** identity-keystore 命中（其中 12 处位于 `store.rs` 的 `#[cfg(test)]` 模块 `:487-552`），且该日志的行号是 `ephemeral.rs:53`（记录写 `:50`）。按 `plan.md:801` 收窄后的判据「测试内部断言不计入」，这些命中可不登记，但记录未说明「不计入的 12 处」，读者按记录去核对命令输出会得到不一致的印象。
- (c) **Linux clippy 日志未回显命令与 target**：`rv3-linux-clippy.log` 只有 `Checking …` 与 `EXIT=0`，没有命令行或 target triple，因此「这是 `--target x86_64-unknown-linux-gnu` 的运行」只能靠记录文本，不能由日志独立证实（本机 host 运行会产生同样输出）。
- (d) **Coverage Index 证据指针仍指修复前日志**：`plan.md` R72–R90 的 `evidence` 仍是 `reports/wp3-identity-keystore.log` / `reports/pv5-windows-dpapi.log`（RV1 前的工作包轮次日志）；R86 也不例外。verification.md 已改用 `rv3-*.log`，两者口径不一致。
- 影响：不影响代码正确性判断，但会直接影响最终验收的证据归因（尤其 (a) 若被读成「已在 5ed869f 执行」即为不实陈述）。
- 建议：在每个 `rv3-*.log` 头部补「HEAD=fe52694，工作区=将成为 5ed869f 的改动」这一事实（或重跑并落盘 `5ed869f`）；把自检行改为「命中 14 处，其中 13 处为 `#[cfg(test)]` 内部断言（列出）、1 处为不可变构造 + 2 处测试内断言」之类可逐一核对的形式；在 clippy 日志头部写入完整命令；把 Coverage Index 的 WP3 行改指 `reports/rv3-pv4.log`/`rv3-pv5.log`。

**F6（P2）Windows 保留设备名未被 `is_valid_label` 覆盖（与注释自述的目标不符）**
- 事实：`entry.rs:47` 的 `WINDOWS_RESERVED` 只含 9 个保留**字符**；`CON`/`PRN`/`AUX`/`NUL`/`COM1..9`/`LPT1..9` 这类保留**名**可通过校验（`is_valid_label("CON")` 为真）。
- 影响：与 `entry.rs:52-53` 注释自称要避免的「在 `File::create` 处报 `Io`（端口层映射为 `Unavailable`），把「标签非法」误报成「后端不可用」」是同一后果；写路径的临时名以 `.` 前缀，不会直接落到设备名，但 `rename` 到 `CON.1` 在 Windows 上会失败 → `Io → Unavailable`。具体 Windows 行为我**没有执行验证**（本机无法运行该断言）。
- 建议：如果保留该注释理由，把保留设备名一并拒（一行常量 + 一次不区分大小写的匹配），或把注释收窄为「保留字符（不含保留设备名）」。

---

## 5) plan.md 覆盖索引与 Check ID 核对

### 5.1 Coverage Index（WP3 行 R72–R90）对 `5ed869f` 现状的核对

| R | 场景 | 本轮判定 | 依据（`5ed869f` 文件内容） |
| --- | --- | --- | --- |
| R72 | 生成/读公钥/签名同源、无私钥导出 | 成立 | `store.rs:262-299`；`keystore.rs:501-521`（64 字节 P1363 + 公钥验签通过）；端口无私钥导出方法 |
| R73 | 生成后读公钥与签名一致 | 成立 | 同上 + `dpapi.rs:71-94` |
| R74 | 不同条目是不同密钥 | 成立（**仅 Windows 执行**） | `keystore.rs:414-443`（两公钥 `assert_ne!` + 交叉验签失败 + 自验通过）；非 Windows 在 `:386-388` 早退 |
| R75 | 删除后条目不可用 | 成立 | `keystore.rs:85-111`（重复删除报 `EntryMissing`） |
| R76 | 签名只接受待签内容、无 DER | 成立 | `store.rs:280-299`；`src` 无 `from_der`；`Cargo.toml` 未开 `pkcs8` |
| R77 | 平台不可用失败关闭 | 成立 | 7 入口闸门（§3 第 8 项）+ `fail_closed.rs:78-143`（含「不可用不得伪装成条目缺失」与顺序断言「可用性先于句柄形状」） |
| R78 | 缺条目时不生成新身份 | 成立（两平台真实执行） | `keystore.rs:149-168`（`Availability::Platform`）、`keystore.rs:392-404` |
| R79 | 损坏条目不被静默替换 | 成立 | `keystore.rs:113-147`（篡改后不覆盖）、`fail_closed.rs:240-266`（未知用途 token → `EntryCorrupt`） |
| R80 | 非 Windows 明确失败 | 成立（Linux 真实执行的那条在 CI） | `fail_closed.rs:39-76`（Linux 上真断言 `Unavailable` + 零写入；Windows 早退并注明由 `:78-143` 覆盖） |
| R81/R82 | 调试输出不含密钥材料 | 部分（WP3 侧无 `Debug` 断言；载体在 `identity-auth/tests/ports.rs`） | `entry.rs:135-146` 的 `EntryHeader` 无秘密字段、`ephemeral.rs:23-25` 的 `Entry` 无 `Debug`、`SecretBytes` 无 `Debug`（compile_fail 文档测试） |
| R83 | 记录与错误不携带秘密 | 成立（`fail_closed.rs:168-217`；`error.rs:10-40` 的 `Io` 只存 `ErrorKind` 文本） | 见 F1：其中一条断言恒真，但分层断言已成立 |
| R84 | 普通数据库只保存引用 | **本变更无载体**（切片 4），不得记为已覆盖 | 本 crate 不写数据库；plan 指向 `PV3/PV4` 日志无法支撑 |
| R85 | 无分布式事务但可恢复 | 部分成立（语义靠 `list`/`delete` + R86 用例表达） | `store.rs:236-243`、`keystore.rs:196-298` |
| R86 | 引用提交失败不影响旧条目 | **成立（Windows 执行；G1 已真修）** | `keystore.rs:233-298` 两阶段 + `rv3-mutation.log` mutation 10（**来自既有日志**） |
| R87 | 孤儿可回收且不影响现有身份 | 成立（Windows 执行） | `keystore.rs:170-194` |
| R88/R89/R90 | 非硬件保护实现默认不启用 | 成立 | `ephemeral.rs` 无 `Default`（全文件通读确认）；`keystore.rs:313-333`、`:363-400`（无进程内兜底） |

**证据指针时效（F5(d)）**：R72–R90 的 `evidence` 仍全是修复前的 `reports/wp3-identity-keystore.log` / `reports/pv5-windows-dpapi.log`；R86（上一轮唯一 P1）现在**有**可用证据（`rv3-pv4.log` + `rv3-mutation.log`），但索引没有指过去。

### 5.2 Check ID 核对

| Check ID | 本轮核对结论 | 依据 / 限制 |
| --- | --- | --- |
| PV1（`npm run verify`） | 记录称已执行且绿；**来自既有日志**，我只读到 `rv3-pv1.log` 的关键行：`crate boundaries OK: 10 个 crate`、`doc links OK: 369 relative links, 3589 section refs`、identity-keystore 各目标 2/5/6/8/12 全 ok | **只在 Windows**；`plan.md:810` 的「无 0 用例」按字面在 Linux 上不可满足（`dpapi.rs` 目标天然 0），应读作「无被掩盖的待验证断言」 |
| PV2（依赖方向） | 绿（`rv3-pv2.log`：10 个 crate 与 §5 一致；`rv3-pv1.log:33` 同结论） | 本轮 WP3 未引入新依赖（`.gitleaks.toml`、测试与文档），`Cargo.toml` 未变 |
| PV3（`identity-auth`） | **范围外**，我不代为判定 | `rv3-pv3.log` 存在；verification.md 记录 78 用例 |
| PV4（`identity-keystore` 全量） | 记录 33 用例（2 + 5 + 6 + 8 + 12）与 `rv3-pv4.log` 逐目标一致；**`permissions` 目标已消失**（G2 修复的间接证据） | Windows 运行；`tampered`/`orphan`/`salts`/`failed_reference_commit` 在 Linux 上会早退（平台依赖，不可避免） |
| PV5（真实 DPAPI） | 绿（5 用例，`rv3-pv5.log`）；「真实 DPAPI」由源码支撑（`dpapi.rs:21` 直调生产 `wrap_secret`/`unwrap_secret`，`platform/` 无测试替身） | 仅 Windows；`filtered out: 2` 是 `dpapi` 过滤器本身造成，属正常 |
| Linux 目标编译 + lint（Check Plan Changes 新增项） | 记录称无告警；日志仅 `Checking … / EXIT=0`，**未回显命令与 target**（F5(c)） | 只能确认「Linux 目标可编译」，**不**证明执行 |
| Mutation（Check Plan Changes 新增项） | 10 个 mutation 全部被对应用例捕获；其中 7（模式常量）、8/9/9b（标签写入/读取路径）、10（R86 两阶段）直接支撑本轮 G1/G2/G5 的「真能失败」判定 | **来自既有日志**，我没有复算 |
| RV1/3.6 | 本轮即 3.6 的 RV3 复核；`tasks.md:35` 的 3.6 仍为 `[ ]`（主 Agent 维护项） | 不得把本轮结论当成 tasks 勾选或最终验收 |

---

## 6) 未覆盖 / 无法确认项与所需证据

1. **Linux 运行时执行（本轮最关键的剩余不确定性）**：本机无 Linux 运行时（无 WSL/Docker）且无法链接 Linux 二进制，我**未执行**任何非 Windows 分支。因此「`unix_modes` 在 Linux CI 上真跑并断言 `0700`/`0600`」这一点，我提供的是源码级推导（不经 `wrap_secret`、属 lib 单测）而不是执行证据。**所需证据**：CI `checks` job 的 Linux 日志，期望 `identity-keystore` lib 目标出现 **4** 条（Windows 上为 2 条）：`store::mode_tests::private_modes_are_restrictive`、`store::unix_modes::private_directory_and_entry_file_modes_are_restrictive`、`store::unix_modes::write_atomic_replaces_content_without_leaving_temporaries_on_failure`、`entropy::tests::…`，且 `fail_closed` 8 passed（含 `unavailable_platform_writes_nothing_at_all`）、`dpapi` 目标 0 用例。该证据**不影响**本轮对 G1/G2 的静态判定，但影响最终验收对「Unix 权限位已覆盖」的表述。
2. **`fe52694..5ed869f` 的 literal diff**：无 diff 工件、我无 Git/shell 访问 → 「哪些行属本轮新增」未核对，本报告只对 `5ed869f` 文件现状负责。若需按 diff 复核，请提供 `git diff fe52694..5ed869f -- crates/identity-keystore .gitleaks.toml` 的输出工件。
3. **`.gitleaks.toml` 规则的真实命中能力**：本地无 gitleaks（`plan.md` 已声明 CI-only），我未执行，也**不能**判定 gitleaks 的 `keywords` 前缀过滤是否大小写敏感。**所需证据**：CI `secrets` job 首次执行的结论；若团队想本地证明，可用一份合成的 base64 条目文本跑一次 `gitleaks detect --no-git`（期望命中 `acpr-keystore-plaintext-entry`）。对冲建议（低成本）：`keywords = ["QUNQS", "qunqs"]`，两种实现都能命中。
4. **F4a/F6 的 Windows 行为**：栈副本清零「是否真的只剩这些残余」需要内存级工具（本 reviewer 无），F6 的 `CON.1` 具体错误类别需在 Windows 上执行一次标签用例（`generate(NodeIdentity, "CON")`）才能确定映射。二者均不影响本轮 PASS。
5. **R84（普通数据库只保存引用）**：本变更内无载体，需在切片 4 的 `owned_provider_ref`/`local_admin` 写路径补断言；本轮**不**记为已覆盖（与 RV1/RV2 一致）。
6. **跨范围提示（属文档轨道，非我 target，仅登记）**：`design.md` D6/D7 仍使用实现里不存在的名字——`PlatformKeystore::open(root)`（实际是 `FileKeystore::new(root, entropy)`/`with_availability`）与 `KeystoreUnavailable`（实际是 `KeystoreError::Unavailable`），且 D6 写「引用缺失/解包失败一律 `KeystoreUnavailable`」与实现/用例（缺失 → `EntryMissing`，见 `keystore.rs:149-168`）不符。RV2-DOCS F7 只修了 `IdentityAuthority`/`pairing_sas`/magic `ACPRKS`，未覆盖这三处。请文档 reviewer/主 Agent 在 WP1·WP4 轨道确认。
7. **我未执行任何命令**（reviewer 只读）。若主 Agent 要在 `5ed869f` 上重建本轮证据，建议按序执行并留证（并写入变更目录）：`cargo test --locked -p identity-keystore --all-features`（Windows 期望 33；Linux 期望 lib 4 + 6 + 8 + 12、dpapi 0）、`cargo test --locked -p identity-keystore --all-features dpapi -- --nocapture`、`cargo clippy --locked -p identity-auth -p identity-keystore --target x86_64-unknown-linux-gnu --all-targets --all-features -- -D warnings`（**在日志头部回显命令与 target**）、`node scripts/check-crate-boundaries.mjs`、`npm run verify`。

### 资源与清理

未创建任何文件、临时目录、worktree 或构建产物；未写 `target/`；未修改 tasks/verification/progress；报告按要求由 runtime 落盘到指定路径。

---
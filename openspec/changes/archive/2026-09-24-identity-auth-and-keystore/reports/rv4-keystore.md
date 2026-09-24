<!-- 由独立 reviewer 子 Agent 产出（RV4 定向确认轮，target revision 892e5f5），主 Agent 原样落盘（仅歧义 `§` 引用补全被引文档名）。 -->
## Review

- **task_id / role / phase / agent_context**：RV4（定向确认）· 独立 reviewer 子 Agent · 检视阶段（只读复核，recheck）· 新会话、未继承实现对话、未参与本变更任何实现或讨论；本轮未修改任何文件、未提交、未合并。
- **target_revision**：`892e5f52b75b6bbd16c99d55023bdcdd9d4d32a2`（分支 `feat/identity-auth-and-keystore`，工作区干净）。
- **scope**：`crates/identity-keystore/**`（src + tests）与 8 份既有证据日志；只按原问题 ID **RV3-WP3 F1–F6** 复核，并顺带核对「7 入口 `require()` 首位」与「36 用例计数」。
- **changes**：无（只读）。
- **checks**：PV1–PV5 / 边界 / 自检 / Linux clippy / mutation —— **全部引用既有日志，均为「来自既有日志」**；本轮我未执行任何命令（会话无 shell 工具，取版本用 `.git/logs/HEAD` 与 `watchdog_diff` 定位）。

---

## 结论：**PASS**（F1–F6 在代码层面全部确认落地，无 P0/P1 新发现；1 条证据工件缺失导致 3 条 mutation 声明无法从工件确认，改以静态推演替代，结论一致）

| 原问题 ID | 位置（target `892e5f5`） | 判定 | 证据 |
| --- | --- | --- | --- |
| **F1** 恒真断言 | `tests/fail_closed.rs:231-279` | **已修**（残留 1 条无判别力的常量比对，非阻断，见 §3 注 1） | 全文件已无 `contains` 断言行（`contains` 只出现在 `:276` 的注释里，且注释明确写「这里**不**写 `label(...).contains(secret)` 为假」）；改后同一用例内有**有判别力**的断言：`:247-262` 对 `store.sign(...)` 真实调用结果断言具名分类 `EntryMissing`/`Unavailable`；`:265-274` 穷尽匹配（加带载荷变体会编译失败） |
| **F2** 失败不留临时文件 | `src/store.rs:551-605`（`#[cfg(test)] mod atomic_tests`，`:551` 无平台 cfg） | **已修** | 失败路径 ① 在 `:578-596`：`blocked` 是**已存在的目录**（`:582 fs::create_dir`），`write_atomic` 必失败（`:583`），随后断言 `leftovers` 无 `.tmp-`（`:592-596`）。实现侧：临时文件在 `:425 File::create` 真的被创建，失败点在 `:439 fs::rename`（覆盖到目录），清理靠 `:445 TempFileGuard` + `:450 impl Drop` + `:440 armed=false` → 该用例确实把「清理」这一动作执行了，且守卫退化为 no-op 时 `.blocked.1.tmp-<pid>-<seq>` 会残留而使 `:592` 失败（**静态推演**；`rv4-mutation.log` 工件缺失，见 §4）。任何平台执行：`atomic_tests` 无平台门禁，`rv4-pv4.log:9` 在 Windows 上列出该用例 ok |
| **F3** 大小写交叉引用 | `src/entry.rs:62-65` ↔ `src/store.rs:126-129` | **已修** | `entry.rs:65`「同一口径也写在 `store.rs` 的 `entry_path` 文档里」→ 落点存在：`store.rs:126-129`「**大小写语义**：本 crate 不做大小写归一……`Primary` 与 `primary` 是同一条目，后写者会覆盖前者」。两处口径一致（唯一性按文件系统语义、调用方负责不冲突） |
| **F4a** 栈副本清零 | `src/store.rs:253-268`、`src/ephemeral.rs:51-62`（另 `:68-82`） | **已修**（残余登记有一处指针悬空，见 §3 注 2） | `store.rs`：`:257 candidate.fill(0)`（熵源失败）、`:263 candidate.fill(0)`（成功返回前）、`:265 candidate.fill(0)`（本轮标量不可用），返回 `SecretBytes`；`ephemeral.rs:61 seed.fill(0)`（`with_seed` 形参副本），`generate_scalar` 三处同样清零。残余**已如实登记**：`src/platform/windows.rs:35-37`「`windows_dpapi::decrypt_data` 内部还有一份自己的明文缓冲……本 crate 无法触及」 |
| **F4b** `parse_handle` 复用校验 | `src/store.rs:369-379` | **已修** | `:377 if !crate::entry::is_valid_label(label)`；旧式「非空 + 长度 + 分隔符」校验已无残留（`:369-379` 只有 `split_once` + 用途匹配 + `is_valid_label`）；新增用例 `tests/fail_closed.rs:177-206` 覆盖 `CON`/`nul`/`bad:label`/`bad?label`/` leading`/`trailing `/`.hidden` 并断言 `EntryInvalid` 且零文件；**可失败性**：退化为旧校验后这些标签会走到 fs 层 → `Missing → EntryMissing`（或 `Io → Unavailable`）≠ `EntryInvalid`，断言必失败（静态推演；mutation 1 工件缺失，见 §4） |
| **F5** `list()` 闸门 | `src/store.rs:143-146`、`tests/fail_closed.rs:148-175` | **已修** | `:145 self.availability.require()` → `:146 StoreError::PlatformUnavailable`（不是「空仓库」）；回归用例 `list_is_gated_by_availability_too`（`fail_closed.rs:149`）双向断言：不可用 → `Err(PlatformUnavailable)` + 零写入，可用空目录 → `Ok(vec![])`；`rv4-pv4.log:43` 显示该用例 ok。去闸门后（返回 `Ok(vec![])`）第一处断言必失败（静态推演；mutation 2 工件缺失，见 §4） |
| **F6** Windows 保留设备名 | `src/entry.rs:48-55`、`:69-72` | **已修** | 注释 `:48-50` 列 `CON/PRN/AUX/NUL/COM1..9/LPT1..9`（4+9+9=22）↔ 实现 `:51-55 const WINDOWS_DEVICE_NAMES: [&str; 22]`（数组长度由类型标注强制）；`:69-72` 取首个 `.` 之前的 stem 做 `eq_ignore_ascii_case` 比对（带扩展名/大小写均拒）；用例 `fail_closed.rs:186-197` 覆盖 `CON`、`nul`。注释与实现一致（唯一瑕疵见 §3 注 3） |

**顺带核对**

1. **7 个端口入口 `require()` 仍是第一步**：**成立**。`src/store.rs:274`（generate）、`:282`（public_key）、`:298`（sign）、`:314`（delete）、`:325`（get_secret）、`:340`（put_secret）、`:347`（delete_secret），每处均为函数体第一条语句（`sign` 之前只有 `use` 声明 `:296`）；`public_key`/`sign`/`delete` 中 `require()` 都先于 `parse_handle`，与设计口径「可用性先于句柄形状」一致（`delete` 注释 `:309` 明写）。
2. **36 用例 vs `verification.md`**：**部分一致**。`reports/rv4-pv4.log`（头部 `revision de61aa5，干净`）实测 `running 3 tests`(:7) + `5`(:16，dpapi) + `6`(:27，entry_format) + `10`(:39，fail_closed) + `12`(:55，keystore) = **36**，`EXIT=0`(:77) → 与 `verification.md:37`（PV1–PV5 行「36 用例（3 lib + 5 + 6 + 10 + 12）」）和 `verification.md:84`（RV3-WP3 行「36 用例」）**一致**；但 `verification.md:81`（RV2-WP3 行的 Recheck Evidence）写「`reports/rv4-pv4.log`（**33 用例，含 2 条 lib 单测**）」，与同一工件**矛盾**（见 §3 注 4）。
3. **RV3-F5（记录可核对性）四子项**：日志 revision 绑定 **已修**（`rv4-pv1/pv2/pv3/pv4/pv5/linux-clippy/selfcheck` 头部均含 `revision：de61aa518c…（de61aa5，干净）` + `### 命令：…`；`rv4-pv3.log:1-2`、`rv4-linux-clippy.log:2-3` 我逐条实读）；Linux clippy 回显命令与 target **已修**（`rv4-linux-clippy.log:2`）；Coverage Index 的 R86 证据指针 **已修**（`plan.md:689` → `reports/rv4-pv4.log, reports/rv4-mutation.log`）；**自检计数未修**（`verification.md:38` 写「共 15 处命中……`ephemeral.rs:53`……`store.rs` 的 `#[cfg(test)]` 模块内 11 处」，而其引用的 `rv4-selfcheck-rg.log` 实为 **17 处**：`types.rs:256`、`entropy.rs:37/38`、`ephemeral.rs:**54**`、`store.rs:499/515/523/525/529/530/539/567/570/571/573/581/587`（**13** 处）；我用同模式对当前树独立检索，**逐行复现了这 17 处**，见 §3 注 5）。

---

## 1) 隔离与版本声明

- **角色与隔离**：本会话是新建 reviewer 子 Agent，未继承父 Agent 的实现对话、未参与本变更实现；本轮**只读**（未编辑任何文件、未提交/合并/切分支、未运行测试或构建）。
- **版本定位**：`.git/HEAD` → `refs/heads/feat/identity-auth-and-keystore`；`.git/logs/HEAD` 末条 `de61aa5 → 892e5f5`（`chore(identity): 回写 RV3 复核记录并绑定修复轮证据到 de61aa5`）→ 本轮 target = **`892e5f5`**；`watchdog_diff`（stat）返回「No working-tree changes against reviewer-launch HEAD 892e5f52b75b」→ 工作区干净。
- **限制（如实声明）**：`watchdog_diff` **不含已提交区间**，本会话亦无 shell/git 工具，因此我**没有**得到 base→target 的 diff；结论基于 **target 版本的完整文件内容** + 原问题 ID 描述（`reports/rv3-keystore.md`）+ 变更目录内的记录（`verification.md`、`plan.md`）与 `target/rv4-commit.txt`（该提交信息工件，非 git 输出）。我没有独立验证 `892e5f5` 是否只改了记录/日志；但有一处**内容级交叉证据**：自检日志（`de61aa5`）列出的 17 处行号与我此刻对 `892e5f5` 源码树的检索**完全一致**（`store.rs:499…587`、`ephemeral.rs:54`），说明本轮源码与日志产生时一致。
- **证据口径**：`reports/rv4-*.log` 均为**既有日志**（我未执行、未复算）；本轮日志头部 revision 是 **`de61aa5`（target 的父提交）**，不是 `892e5f5` 本身（差异可解释：`892e5f5` 是记录回写提交）。
- **缺失证据**：`reports/rv4-mutation.log`（轮次说明与 `verification.md` 所指的 mutation 工件）**在工作区不存在**（直接读取报 `ENOENT`；`ls` 该目录仅列出 `rv4-pv1..pv5.log`、`rv4-linux-clippy.log`、`rv4-selfcheck-rg.log`；仓库根 `reports/` 与 `target/` 下亦无同类文件）。故 F2/F4b/F5 的 mutation 部分只能靠**静态推演**（结论：三者在对应实现退化时都会失败），不能在报告中声称「来自既有的 mutation 日志」。

---

## 2) 逐条复核

见上文结论表（F1–F6 逐条 + 顺带核对三项），每条都给出 `file:line` 与日志行号。补充两点定位细节：

- F1 的原始缺陷形态（对固定变体返回的字符串做 `contains`）**已不存在**：`tests/fail_closed.rs` 内 `contains` 仅在 `:276` 的说明性注释中出现；替换物包括一条**真调用结果**断言的具名分类（`:247-262`）加一条编译期约束（穷尽匹配 `:265-274`）。
- F2 的「触发清理」是**真实**的：临时文件在 `store.rs:425` 创建成功，`store.rs:439` 的 `fs::rename` 因目标是已存在目录而失败，唯一清理点是 `store.rs:445/450` 的守卫析构；因此该用例的「无 `.tmp-` 残留」断言（`store.rs:592-596`）对守卫退化是敏感的。

---

## 3) 新发现

**无 P0/P1。** 以下 5 条为 **P2 记录/注释类（非阻断，仅登记供主 Agent 处置，不构成本轮新发现）**：

1. **注 1（F1 残留）**：`tests/fail_closed.rs:274` 的 `assert_eq!(label(&KeystoreError::EntryMissing), "entry_missing")` 同样**结构上无法失败**（对固定变体返回字面量）。它的实际作用是「锚定穷尽匹配使其被编译」，且 `:265` 的匹配确实是编译期判别；建议在 `:274` 加一句说明（例如「此行只用于锚定穷尽匹配，本身无判别力」），以免读者再次把它当成行为断言。
2. **注 2（F4a 交叉引用悬空）**：`src/store.rs:232` 写「平台 wrapper 内部仍有一份自己的缓冲区（第三方实现，**见 `platform/mod.rs` 的残余说明**）」，但 `platform/mod.rs` 全文**没有**该残余说明——真正的登记在 `src/platform/windows.rs:35-37`。同类问题（引用无落点）正是 RV3-WP3 F3 的形态，建议把指针改指 `platform/windows.rs`。
3. **注 3（F6 函数级文档偏窄）**：`src/entry.rs:44` 的摘要只写「非空、≤ 128 字节、不含路径分隔符与控制字符」，而实现还拒 Windows 保留字符（`:61`）、首尾空白/点（`:66-67`）、保留设备名（`:69-72`）——后者在行内注释里已写清，摘要建议补齐。
4. **注 4（36 用例计数）**：`verification.md:81` 的「`reports/rv4-pv4.log`（33 用例，含 2 条 lib 单测）」与 `rv4-pv4.log` 实际的 36 用例 / 3 条 lib 单测矛盾（33/2 是 RV3 时代 `rv3-pv4.log` 的数字）。
5. **注 5（自检计数）**：`verification.md:38` 的「共 15 处命中」与其引用的 `rv4-selfcheck-rg.log`（17 处，其中 `store.rs` 13 处）不一致，且行号写 `ephemeral.rs:53`（日志为 `:54`）；`verification.md:45` 仍写 `ephemeral.rs:50`。日志头部把命令记成未转义的 `grep -rn "unwrap()|expect(|panic!|…"`，与 `verification.md:38` 的转义写法、`plan.md:801` 的 `rg -n` 三种写法并存（结果与「正则交替」一致，但工件不自证命令）—— RV3-DOCS-3 的同类项未完全收口。

---

## 4) 未覆盖 / 无法确认项

1. **mutation 1/2/3 的可失败性**：`reports/rv4-mutation.log` 在 target 工作区**不存在** → 无法确认。替代结论（**静态推演，未经执行**）：mutation 1（`parse_handle` 退回旧校验）会使 `fail_closed.rs:178-206` 的 7 个标签拿到 `EntryMissing`/`Unavailable` 而非 `EntryInvalid`；mutation 2（`list()` 去闸门）会使 `fail_closed.rs:149` 的 `Err(PlatformUnavailable)` 变成 `Ok(vec![])`；mutation 3（`TempFileGuard` 不清临时文件）会留下 `.blocked.1.tmp-<pid>-<seq>` 而使 `store.rs:592` 失败。另注：F2 注释（`store.rs:578`）把失败机制写成「`File::create` 失败」——实际是 `:439` 的 `rename` 失败；若注释所述为真（临时文件根本没建），该用例对 mutation 3 就**不再**有判别力，故该注释值得订正（列为注 2 之外的同一类记录瑕疵，不单列）。
2. **Linux 运行时**：`rv4-linux-clippy.log` 只有 `Checking/Finished` + `EXIT=0`，**不链接、不执行**；因此 `atomic_tests`「任何平台执行」这一表述在 Linux 上只有**编译**证据，`unix_modes`（`0700/0600` 真实 `chmod`）与 Linux 下的失败关闭运行时行为仍待 CI 的 Linux runner。
3. **我未执行任何命令**：`npm run verify`、`cargo fmt/clippy/test`、`node scripts/check-*`、mutation 复跑均未由我执行；`gitleaks` / `cargo-deny` 本地本来也无等价物（只在 CI）。上文所有 PASS/计数均来自既有日志或源码静态阅读。
4. **CI 依赖面未验证**：`.gitleaks.toml` 规则的真实命中能力、`deny.toml`/advisory 判定仍只能由 CI 证明（未在本轮范围内）。
5. **低置信观察（不计入发现，且非本 diff 引入）**：`get_secret`/`delete_secret` 直接接 `key: &str` 而未过 `is_valid_label`（`put_secret` 经 `EntryHeader::new` 会拒）——Windows 上 `CON`/`NUL` 之类键名在**读/删**路径可能落到设备名语义，报 `Ok(None)`/`Unavailable` 而非 `EntryInvalid`。当前 crate 无生产调用方（`app` 未落地），不可达；如后续接线，建议在读/删路径也复用同一校验。

**残余风险（本轮范围内）**：① mutation 证据工件缺失，回归防护的「可失败性」仍只有静态推演（建议在 `de61aa5`/`892e5f5` 上重跑 mutation 并保留日志，或在记录里改指真实文件名）；② `verification.md:81` 的 33/36 计数矛盾与 `:38` 的自检计数不一致会让后续核对者再次得到「记录与日志不符」的印象；③ 上述均为记录/注释级，不影响产品代码正确性与 §3 不变量，也无回归证据。
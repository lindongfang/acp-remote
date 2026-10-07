# IV1 独立验证报告（覆盖充分性与风险盲区）

- **Task**：tasks.md 7.1 `[validation]`，判据取自 `plan.md` 的 `## Independent Validation` 表 IV1 行
- **Target Revision**：`aed96a270441e9e9271a4c1ea9cd1ebcaadf5f9c`（`refs/heads/main`，MU1a–MU3e 8/8 已合入）
- **Validator**：独立验证角色（只读；未提交、未改任何分支/代码/规划文件）
- **日期**：2026-10-07
- **证据路径**：`reports/IV1.md`（本报告）+ `reports/IV1.log`（validator 亲自执行的命令与原始输出，23,815 字节；`plan.md:1012`/`tasks.md:164` 指定的证据路径）

---

## 0. 结论

**IV1 = PASS（带缺口）**。IV1 点名的三项重点**都**有机器可判定的断言，且我**自己做过变异验证**证明它们真的能红（见 §3–§5）。判 PASS 的依据是：三项重点的覆盖充分性成立，没有「只在文档里声称」的情况。

但本变更的证据链存在 **12 处可证的缺口**（§7），分三档：

**P1（证据完整性，2 条）** —— 会影响 9.1 的结论：

1. `reports/PV1-main-mu3e.log` 是 **FAIL**（`test result: FAILED. 10 passed; 2 failed`，全文无 `EXIT=` 行），却被登记为 MU3e 的主分支回归证据；`verification.md:630` 只记 sha256 不记结果，其尾部的「复跑 1 次即 12 passed」**在仓库内没有任何复跑日志**（`.target-wt/` 下无 `mu3e-main`，`reports/` 下无任何复跑日志）。
2. `reports/PV3-main-mu3e.log`（`tasks.md:159`/6.62 明文要求）**完全不存在**（`reports/` 下 `PV3-main-*` 一条都没有），`verification.md` 也未登记其缺失。

**P2（覆盖缺口，4 条）** —— 不影响当前正确性，但门禁在未来回归时不报警：

3. **没有任何前端构建产物标识绑定最终提交**（IV1 行明文要求「附前端构建产物标识与依赖版本」）——`PV3-mu3e.log` 无 `expo export`，无 `dist/`，`receipt-mu3e.md:15` 的 command 不含它。
4. `crates/core/src/derive.rs:311` 的前导 `..` 逃逸守卫**在 core 自己的单元测试里没有独立判别力**——我禁用该守卫后 `derive::` 34 条全绿（变异 M-A 存活）；只有把它与 `join_lexically` 同时退化（M-B）才会红，实际兜底者是 `storage-sqlite` 的 TP2 行为测试。
5. `routeContent` 的逐类覆盖落在一个**无生产调用方**的分类器上，且**没有存储级逐类断言**——我给 imported 的 `prompt` 开缺口后 19 条逐类断言全绿（变异 R20a 存活）。`routeContent` / `mayMarkInputAsSent` / `putImportedMetadata` / `PlatformPorts.importedContent` 的方法**均无生产调用方**。
6. `session.read` 分页的「同刻取最早」逻辑**不在生产路径上**：`toLoadedPage().nextBefore` 未被 store 消费，生产续取游标来自 `domain/conversation.ts:133-141` 的 `messages[0]`（直接取首条，无时间比较），该路径的同刻情形无测试。

**P3（文档与证据不一致 / 登记薄弱，6 条）** —— 不影响运行，影响验收可追溯性：

7. `verification.md:117`/`:121` 仍把 `reports/review-w3-r2.md` 记为 Report Path，而该文件**全仓不存在**，且无一句说明缺失原因。
8. `check:fixtures` 是一个**不存在的 Check ID**，却被 `plan.md:1003`/`:1063`、`tasks.md:16`/`:63`、`verification.md:181`/`:751` 六处判据引用。
9. `verification.md:535`（F13「未采纳的 MINOR…留作后续非阻断项」）与 `:363-365`（F13「已解决且彻底」）**同一文档内自相矛盾**（stale 文本）。
10. merger 无法 `dispatch --ack` 的结构性缺陷在 `verification.md` **无登记节**（只在各 merger 报告里）。
11. **Chromium 版本号全仓缺失**（实测是 `msedge.exe`，且无版本号）。
12. （本次已闭合）`reports/IV1.log` 已产出。

除第 12 条外，其余 11 条都是可行动的、非文档风格的实质问题，**必须进入任务 9.1 的复述清单**（§8）。

---

## 1. 目标版本与环境的机械核实

| 项 | 实际值 | 核实命令 |
| --- | --- | --- |
| 仓库 | `D:\Project\acp-remote` | `pwd` |
| HEAD | `aed96a270441e9e9271a4c1ea9cd1ebcaadf5f9c` | `git rev-parse HEAD` |
| `refs/heads/main` | 同上（等于 HEAD） | `git rev-parse refs/heads/main` |
| 工作区 | 仅 3 个预期未跟踪项：`.target-wt/`、`.worktrees/`、`openspec/changes/sync-scope-and-pwa-client/` | `git status --porcelain` |
| AC3 的 cwd 绑定 | `.worktrees/mu3e-merge` HEAD = `aed96a2`；`git diff --stat main .worktrees/mu3e-merge` **为空**；该 worktree 工作区干净 | `git -C .worktrees/mu3e-merge rev-parse HEAD`、`git worktree list` |
| Node / npm | `v22.22.0` / `10.9.4` | `node --version`、`npm --version` |
| cargo / rustc | `1.98.1` / `1.98.1` | `cargo --version`、`rustc --version` |
| 磁盘 | `D:` 652G，已用 125G，可用 528G；`.target-wt` 累计 65G | `df -h .`、`du -sh .target-wt` |

**结论**：目标提交可机械确认，AC3 的 cwd 确实与最终提交同源，**不存在**「拿别的版本结论覆盖最终提交」的问题。

---

## 2. IV1 三项重点的独立核对（逐条）

### 重点 1：路径越界判定是否覆盖规范化前缀边界 —— **PASS**

**实现**（`crates/core/src/derive.rs`）：

| 位置 | 职责 |
| --- | --- |
| `:304-352` `display_path` | 相对路径分支（`:306-334`）与绝对路径分支（`:335-352`） |
| `:415-447` `normalize` | 词法规范化 + **显式计数**前导 `..`（`Normalized.escapes`） |
| `:354-362` `join_lexically` | 先拼接再折叠，使片段内部的 `..` 能真正回退 |
| `:450-476` `canonical_path` | `canonicalize` 失败时逐级上溯到第一个存在的祖先 |
| `:478-496` `strip_verbatim` | 剥 `\\?\` / `\\?\UNC\` 前缀 |

**机器断言**（不是文档声称）：

- `crates/core/src/derive.rs:760-895`：`display_path_relativizes_inside_the_workspace`、`display_path_marks_outside_and_keeps_only_the_file_name`、`display_path_does_not_treat_a_string_prefix_as_inside`（`:779`）、`display_path_without_a_workspace_root_is_outside`、`display_path_escapes_a_relative_parent_traversal`、`display_path_escapes_many_leading_parent_traversals`、`display_path_escapes_multi_level_traversal_to_the_declared_file_names`、`display_path_keeps_an_inner_parent_traversal_that_stays_inside`、`display_path_treats_the_workspace_root_itself_by_the_spec`、`normalize_counts_leading_traversals_on_relative_paths`。
- `crates/core/src/derive.rs:905-1009`（Windows/UNC）：`normalize_marks_the_root_under_a_windows_prefix`、`normalize_counts_leading_parent_traversals_without_popping_the_root`、`display_path_traversal_shapes_stay_outside_on_every_platform`、`display_path_treats_a_windows_parent_traversal_as_outside`。
- `crates/storage-sqlite/tests/derived_events_behaviour.rs:784-1237`（TP2，真实 SQLite 落库 + 真实 `file.changed` view 字节）：`an_inside_path_is_relativized_without_the_root_fragment`(`:784`)、`an_outside_path_is_marked_and_carries_only_the_file_name`(`:829`)、`a_string_prefix_is_not_treated_as_inside_the_workspace`(`:866`)、`an_unregistered_workspace_root_falls_back_to_outside`(`:916`)、`a_relative_path_escaping_the_root_is_outside`(`:955`)、`a_relative_form_inside_the_workspace_is_kept_relative`(`:986`)、`the_workspace_root_itself_never_leaks_its_fragment`(`:1018`)、`an_inner_dotdot_that_stays_inside_is_inside`(`:1045`)、`an_absolute_path_with_an_inner_dotdot_that_stays_inside_is_inside`(`:1075`)、`multiple_leading_parent_traversals_keep_only_the_file_name`(`:1113`)、`another_absolute_outside_path_keeps_only_the_file_name`(`:1155`)、`two_same_named_outside_files_derive_two_changes_on_real_storage`(`:1193`）。
- `crates/core/src/broker.rs:9012-9190`（派生入口 + view 字节）。

**我实际执行的命令与结果**：

```
set CARGO_TARGET_DIR=D:\Project\acp-remote\.target-wt\iv1
cargo test --locked -p core --all-features derive::
→ test result: ok. 34 passed; 0 failed; 0 ignored; 155 filtered out
```

**逐情形核对**（IV1 点名的三种）：

| 情形 | 是否有机器断言 | 断言位置 |
| --- | --- | --- |
| `/a/b` vs `/a/bc` 前缀边界 | **有**（且不止一处） | `derive.rs:779`、`derive.rs:905-925`、`derived_events_behaviour.rs:866-911` |
| 前导 `..` 越过根 | **有** | `derive.rs:794-843`（含 `../../../../../../a/b/c`）、`derive.rs:898-906`、`derived_events_behaviour.rs:955-984`、`:1113-1152` |
| 根未登记（`workspace_root = None`） | **有** | `derive.rs:787-791`、`derived_events_behaviour.rs:916-952` |
| 绝对路径盘符根 `..` | **有**（`#[cfg(windows)]`） | `derive.rs:899-921`、`:1000-1009` |

**判定：PASS。** 三项情形都有机器可判定的断言，且断言绑定的**不是实现细节**（断言的是「下发的 view 字节里 `outsideWorkspace:true` 与 `displayPath` 只含文件名」，不是「调用了某个函数」）。

---

### 重点 2：会话明细分页是否覆盖不重不漏 —— **PASS（覆盖成立，但只覆盖客户端一侧）**

**实现现状（必须先说清楚）**：`session.read` 的 `(createdAt, messageId)` 复合游标**在服务端没有实现**。

| 事实 | 位置 |
| --- | --- |
| `server::sync` 模块不存在（只有 `local_admin`/`node_link`/`transport`） | `crates/server/src/lib.rs:27-29` |
| `/sync/v1*` 未注册，一律 404 | `crates/server/src/transport/net/route.rs:5,250-252` |
| core 端口的分页游标是 `Option<GlobalCursor>`（事件全局序号），**不是** `(createdAt,messageId)` | `crates/core/src/ports.rs:350-357` |
| `HistoryPage` **没有** `has_earlier` 字段（只有 `head` + `next: Option<GlobalCursor>`） | `crates/core/src/ports.rs:392-404` |
| 存储层唯一 `read_session` 实现按 `global_sequence > ?2 ORDER BY global_sequence ASC LIMIT ?3` 取页 | `crates/storage-sqlite/src/session_store.rs:2581-2702`（SQL 在 `:2608-2610`） |
| wire DTO `SessionReadBefore` / `SessionRead.before` / `SessionReadResult.has_earlier` **无任何生产消费方** | `crates/sync-protocol/src/command.rs:391-412`、`:895-942` |

这与 `design.md:145` 的自陈一致（「快照与 `session.read` 尚未有实现（`server::sync` 未落地）」），且是本变更的**显式 Non-Goal**（`proposal.md` 的 `non_goals` 第 1 条）。**不是缺陷**，但决定了覆盖的边界：断言只能落在**契约层**（schema/fixture 形状）与**客户端层**（纯函数 + 内存 store）。

**我实际执行的命令与结果**：

```
cd .worktrees/mu3e-merge/clients/app
npx --no-install vitest run --config vitest.config.ts \
  src/sync-client/paging.test.ts src/platform/r20-no-content-cache.test.ts src/protocol/contract.test.ts
→ 3 passed (3)；Tests 52 passed (52)   [paging 17 / r20 13 / contract 22]

set CARGO_TARGET_DIR=...\.target-wt\iv1
cargo test --locked -p sync-protocol --all-features
→ snapshot_scope_vectors 8/8、contract_vectors_r1_r9 17/17、schema_drift 6/6、
  envelope_fixtures 7/7、view_projections 3/3 … 全 ok，0 failed
```

**逐情形核对**（IV1 点名的三种）：

| 情形 | 是否有机器断言 | 断言位置 |
| --- | --- | --- |
| 同毫秒多条不重不漏 | **有** | `clients/app/src/sync-client/paging.test.ts:60-74`、`:86-94`；`crates/sync-protocol/tests/contract_vectors_r1_r9.rs:256-292`、`:296-345`；`snapshot_scope_vectors.rs:355-441` |
| 页序异常（不按 messageId 排序） | **有** | `paging.test.ts:76-84`；`contract_vectors_r1_r9.rs:277-282`（`assert_ne!(tied, sorted(tied))`）；`snapshot_scope_vectors.rs:437-441` |
| 跨页边界不重不漏 | **有** | `clients/app/src/features/conversation-paging.test.ts:83-101`、`:103-117`；`snapshot_scope_vectors.rs:355-441`；`contract_vectors_r1_r9.rs:296-345` |
| 单字段游标被拒 | **有**（三个独立入口） | `crates/sync-protocol/src/command.rs:1436-1443`；`snapshot_scope_vectors.rs:274-301`；`contract_vectors_r1_r9.rs:496-522`；`paging.test.ts:51-58`、`:190-195` |
| `assertPagesDoNotOverlap` 的「重」与「漏」两向 | **各有真实断言** | 见下 §3 变异 P1/P3 |

**判定：PASS（覆盖充分），但有一条必须在 9.1 复述的边界**：`toLoadedPage().nextBefore`（唯一使用 `earliestCursor` 平票逻辑的地方）**在生产 store 中未被消费**——`clients/app/src/features/client-store.ts:613-616` 取的是 `page.earliestCursor`，而它来自 `clients/app/src/domain/conversation.ts:133-141` 的 `messages[0]`（直接取首条，不做时间比较）。生产路径上「同刻首条即最早」这一前置条件**无测试**（`domain/domain.test.ts:199-250` 只有单条与空页）。见 §7 缺口 6。

---

### 重点 3：imported 资源是否不落盘 —— **PASS（实现上不存在落盘路径，且有真实存储级断言）**

**实现**：

| 事实 | 位置 |
| --- | --- |
| imported 正文的内存载体只有 JS `Map` | `clients/app/src/platform/local-cache.web.ts:38-56` |
| 持久层唯一物理写 `#replaceAll`（`store.clear()` + `store.put`） | `clients/app/src/platform/local-cache.web.ts:127-138` |
| imported 侧唯一持久入口只接受 7 字段的 `ImportedMetadata`（无正文字段） | `local-cache.web.ts:186-209`；类型 `local-cache.ts:77-87` |
| 落盘守门 `assertPersistableForOrigin` 在**两个**端口方法里真的接线 | `local-cache.ts:219-247`；调用点 `local-cache.web.ts:168`、`:193` |
| 七类 `ContentKind` 定义 | `clients/app/src/sync-client/imported-content.ts:18-32` |

**机器断言**：

- **真实（替身）IndexedDB 事务管线 + 直接翻查存储**：`clients/app/src/platform/r20-no-content-cache.test.ts:92-115`（`:98` 正文进内存 → `:102-103` dump 不含正文 → `:105` dump **含** `contentDigestSha256`（反平凡通过）→ `:107-109` 读回条目不含正文）；`:117-128`（夹带正文被拒 + `:126` dump 不含 + `:127` `importedEntries === 0`）；`:130-141`（local 夹带 metadata 被拒）。
- **真实浏览器（Chromium）**：`clients/app/src/platform/testing/browser-checks.ts:181-207`（sentinel 写入 → 持久层不含）、`:288-315`（F4 守门接线）。证据 `reports/PV3-mu3e.log:22`「PASS F4 imported 正文夹带被守门拒绝且未落盘 —— rejected=true imported=0」「16/16 passed」。
- **纯函数逐类**：`r20-no-content-cache.test.ts:77-90`；`imported-content.test.ts:33-50`。

**我实际执行的命令与结果**：

```
cd .worktrees/mu3e-merge/clients/app
npx --no-install vitest run --config vitest.config.ts \
  src/platform/r20-no-content-cache.test.ts src/platform/local-cache.test.ts src/sync-client/imported-content.test.ts
→ 3 passed (3)；Tests 32 passed (32)   [r20 13 / local-cache 13 / imported-content 6]
```

**判定：PASS。** 七类 imported 正文**没有任何持久化路径**是结构性的（持久入口的类型与运行期守门双重约束），且有「查过存储、一条都没有」的存储级断言与真实浏览器断言。

**但必须同时登记两条边界**（见 §7 缺口 7）：

1. `routeContent`（逐类覆盖的**唯一**载体）与 `mayMarkInputAsSent` **全仓无生产调用方**（`clients/app/src/sync-client/index.ts:86` 是它们定义外的唯一出现处；调用点全在 `*.test.ts` 与 `browser-checks.ts`）。`putImportedMetadata` 同样**无生产调用方**；`PlatformPorts.importedContent`（`clients/app/src/platform/index.ts:71`）的方法在生产中也**零消费**。生产里唯一落盘写入是 `clients/app/src/composition.ts:680` 的 paired-host 摘要。也就是说「七类逐类」的覆盖落在一个**尚未接线**的分类器上，与 `review-wp7-r2` 登记的 `mayMarkInputAsSent` 空洞同类。
2. 守门允许清单只拦**键**（`local-cache.ts:231-238` 的 `Object.keys`），不拦**值**；现有两条「夹带」断言都只加**新键** `body`。见 §3 变异 R20a 的存活。

---

## 3. 我亲自做过的变异验证（前红 / 后绿 / 是否还原）

全部变异为**临时**注入，用完 `cp` 还原，并以 `md5sum` + `git status --porcelain` 自证。

### 3.1 Rust 侧（`crates/core/src/derive.rs`，基线 md5 `936150fcbdcdbc251f55e9908d8555b0`）

| 变异 | 内容 | 结果 | 还原 |
| --- | --- | --- | --- |
| **M-A** | `:311` 的 `if normalized.escapes > 0 {` → `if false {`（禁用前导 `..` 逃逸守卫） | **存活**：`cargo test -p core --all-features derive::` → `34 passed; 0 failed` | 已还原 |
| **M-B** | 同时去掉逃逸守卫并把 `join_lexically(&root, &normalized.path)` 换回朴素 `root.join(&normalized.path)`（等价于修复前实现） | **变红**：`4 failed` —— `display_path_traversal_shapes_stay_outside_on_every_platform`、`display_path_escapes_a_relative_parent_traversal`、`display_path_escapes_multi_level_traversal_to_the_declared_file_names`、`display_path_escapes_many_leading_parent_traversals` | 已还原 |
| **M-C** | 绝对路径分支 `:342` 的 `reported_canonical.strip_prefix(&root_path)` 换成字符串 `cand_str.starts_with(&root_str)`（去掉组件边界） | **core 单元测试存活**（`derive::` 34 passed）；但 **`storage-sqlite --test derived_events_behaviour` 变红**：`a_string_prefix_is_not_treated_as_inside_the_workspace ... FAILED`（`derived_events_behaviour.rs:902`） | 已还原 |

**结论**：M-B 证明「前导 `..` 越过根」这一支**确实被覆盖**；M-C 证明「同名前缀不构成区内」**确实被覆盖**，但**只在 TP2 行为测试里**，core 自己的单元测试覆盖不到它（因为 `display_path_does_not_treat_a_string_prefix_as_inside` 传的是**相对**路径 `"/work/api-tools/file.txt"`，在 Unix 语义下被当作相对路径走相对分支；Windows 上走绝对分支但 `strip_prefix` 与 `starts_with` 对「组件边界相同」的输入**恰好同结果**，故变异在该用例上不可区分）。

**M-A 存活是本报告最重要的发现之一**：`:311` 的逃逸守卫在 core 单元测试里**没有独立判别力**——只有把守卫与 `join_lexically` **同时**退化（M-B）才会变红。这意味着未来若有人只删守卫、保留 `join_lexically`，`derive::` 的 34 条会全绿（但 TP2 的 12 条路径用例仍会兜住，故非 P0）。

### 3.2 行为探针（临时测试，已删除）

在 `derive.rs` 临时插入一条探针测试（随后整体还原）：

```
display_path(Some("/work/api"), "../api/x")  →  (?, true)
```

**实测 `outside = true`**。即：相对路径 `../api/x` **规范化后恰好等于工作目录根本身**（`/work/api`），被 `:342-345` 的 `candidate == root` 分支判为越界并下发 `"."`。
`design.md` D5 与 `derived_events_behaviour.rs:1045` 的注释写的是「`..` 回退到根**之下**仍判区内」，而该输入回退到的不是「根之下」而是「根本身」，故实现是自洽的。**但 `specs/workspace-resolution/spec.md` 的场景文字与 `design.md` 均未点名「恰好等于根」这一情形**（只有 `derive.rs:860-877` 的单元测试用绝对路径形态覆盖了它）。`[INFERENCE]` 口径差异，非缺陷。

### 3.3 TypeScript 侧（`clients/app/src`，在 `.worktrees/mu3e-merge` 内，用完还原）

| 变异 | 文件 | 内容 | 结果 | 还原 |
| --- | --- | --- | --- | --- |
| **P1** | `sync-client/paging.ts` | 删除 `assertPagesDoNotOverlap` 的 messageId 交集检测（「重」方向） | **变红**：`1 failed` —— `相邻两页出现同一 messageId 时抛错（防止重复呈现）` | 已还原（md5 `1a0ac5ede162cd8e2afcb7265e2a9351`） |
| **P2** | `sync-client/paging.ts` | `earliestCursor` 的 `if (ms < earliestMs)` → `if (ms <= earliestMs)`（同刻取**最后**一条） | **变红**：`2 failed` —— `同一时刻的多条消息不重不漏：游标取页内最早一条并保留其标识`、`页内同刻消息按出现顺序取第一条（前置条件：服务端按 (createdAt, messageId) 升序）` | 已还原 |
| **P3** | `sync-client/paging.ts` | 删除「漏读」边界检测（`newestOlder > oldestNewer`） | **变红**：`1 failed` —— `较早页含有比最新页最早一条更新的消息时判漏读并抛错` | 已还原 |
| **R20a** | `sync-client/imported-content.ts` | `if (origin.kind === "remote")` → `if (origin.kind === "remote" && kind !== "prompt")`（给 imported 的 prompt 开一个缺口） | **存活**：`imported-content.test.ts` + `r20-no-content-cache.test.ts` → `19 passed` | 已还原（md5 `811861801a0e159dcca9b91551ee4926`） |
| **R20b** | `sync-client/imported-content.ts` | `PERSISTABLE_LOCAL_KINDS.prompt: false` → `true` | **变红**：`2 failed` | 已还原 |
| **R20c** | `platform/local-cache.web.ts` | 去掉 `putImportedMetadata` 里的 `assertPersistableForOrigin({kind:"imported"}, input)` 接线 | **变红**：`1 failed` —— `落盘守门被真的接线：imported 元数据夹带正文即拒绝，且存储里一条都没有` | 已还原（md5 `7c60042225f57eb33ca67ccbd38c3bc0`） |

**P1/P2/P3 的意义**：IV1 重点 2 点名的「同毫秒多条」「页序异常」「跨页边界」三项**都真的能红**，不是恒真断言。**R20b/R20c 的意义**：落盘守门的接线与「prompt 不得可持久化」都真的能红。

**R20a 存活**：给 imported 的 `prompt` 开一个分类缺口后，**逐类覆盖（19 条）全绿**。根因是**没有任何断言把 imported 的某一类正文真的送进持久层再翻查存储**——存储级的两条断言（`r20-no-content-cache.test.ts:95`/`:119`）推的都是**同一个合成字符串**，且走的是「metadata 多一个键 `body`」这一条路径。**这是本报告的第 2 个最重要发现**：逐类覆盖只落在 `routeContent` 的纯函数返回值上，而 `routeContent` 无生产调用方；一旦真的接线，分类缺口不会有存储级断言兜住。

### 3.4 独立复核（非变异）

- **跨语言固定向量**：`npx vitest run src/r11-fixed-vectors.test.ts` → `11 passed`（`.worktrees/mu3e-merge`，= `aed96a2`）。
- **我另用 Node WebCrypto 独立复核**了 `fixtures/sync/v1/transcripts/device-proof.json`：
  - `transcriptBase64url` 解码后 303 字节，`SHA-256` = `2032808837cb95956fcf12a81cd957d4dfb404bd7bfab7c0be49e8a03003faff`，**逐字等于** fixture 的 `transcriptSha256Hex`；
  - 用 fixture 的 `privateJwk` 公钥对 `p1363Signature` 验签 transcript → **`true`**；把 transcript 第 11 字节翻一个 bit 后同一签名 → **`false`**。
  → **该对拍是真实字节比对，不是恒真**（若 TS 侧编出的字节与向量差一个字节，验签必然失败）。AC3 的「字节级对拍」判据成立。
- **AC1 独立复跑**：`cargo test --locked -p app --test node_link_e2e` → `6 passed; 0 failed`（含三条 `ac1_*` 断言）。
- **flake 表征独立复现（两个数据点）**：
  - 第 1 次：`cargo test --locked -p app --test daemon_lifecycle` → `FAILED. 11 passed; 1 failed`，失败者是 `the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown`，而同一次运行里 `the_periodic_task_runs_again_after_one_full_cycle` **通过**。
  - 第 2 次（记录在 `reports/IV1.log:395-417`）：同一命令 → `ok. 12 passed; 0 failed`（60.46s）。
  - 失败成员在两次运行间**互换**，且 `reports/PV1-main-mu3e.log:518`/`:520` 又是**另一对**（两条同时红），确证非确定性。同一提交上 `reports/AC2.log:517-539` 也是 `12 passed; 0 failed`。
  → **flake 真实存在且与本次变更无关**（该测试文件相对 base 逐字节未改）；但**「复跑 1 次即绿」这一具体说法在仓库内没有对应的日志**（见 §7 缺口 1）。

### 3.5 还原自证

```
md5sum crates/core/src/derive.rs                  → 936150fcbdcdbc251f55e9908d8555b0   （与备份一致）
md5sum .worktrees/mu3e-merge/clients/app/src/sync-client/paging.ts           → 1a0ac5ede162cd8e2afcb7265e2a9351
md5sum .worktrees/mu3e-merge/clients/app/src/sync-client/imported-content.ts → 811861801a0e159dcca9b91551ee4926
md5sum .worktrees/mu3e-merge/clients/app/src/platform/local-cache.web.ts     → 7c60042225f57eb33ca67ccbd38c3bc0
git status --porcelain                        → 只有 ?? .target-wt/ ?? .worktrees/ ?? openspec/changes/...
git -C .worktrees/mu3e-merge status --porcelain → 空
```

**所有变异已还原，最终 worktree 干净。**

---

## 4. 前端构建产物标识与依赖版本（IV1 行明文要求）

### 4.1 构建产物标识 —— **最终提交上不存在**

**结论：`reports/` 中不存在任何绑定最终提交 `aed96a2` 的构建产物标识。**

- `reports/PV3-mu3e.log` 全文 30 行，只有 `npx tsc --noEmit`、`npx vitest run`（37 文件 / 458 passed）、`node scripts/run-browser-check.mjs`（16/16）、`node e2e/run-e2e.mjs`（21/21）——**没有 `expo export`**。
- `reports/receipt-mu3e.md:15` 的 `command` 也不含 `expo export`。
- `clients/app/dist/` 与 `.worktrees/mu3e-merge/clients/app/dist/` **均不存在**（两处 `ls` 均 `No such file or directory`）。
- 唯一与 `aed96a2` 相关的「产物」是 e2e 在临时目录里建的 `bundle.js` / `index.html` / `e2e-evidence.json`（`/tmp/tp4-e2e/`，`run-e2e.mjs:47` 的 `WORK_DIR = join(tmpdir(), "tp4-e2e")`），**无文件名哈希、无字节数被登记**。
- 历史上确实存在的产物体标识（**均属其它提交，不可当 IV1 证据**）：
  - `_expo/static/js/web/entry-e0bd83126c72b94d57282ce6ecb5f6ff.js`（1.03 MB，684 modules）—— `PV3-wp5a-r1.log:43`、`PV3-wp5a-r2.log:43`、`PV3-wp5b-r1.log:48`、`PV3-wp6-r1.log:53`、`PV3-wp6-r4.log:57`（642 modules 版见 `PV3-wp5b-r2-export.log:2,19`）
  - `entry-998b3e41d6d12738f30a6fc56c95d693.js`（1.08 MB）—— `PV3-wp7-r1.log:57`
  - `index.html` 1.18 kB / `metadata.json` 49 B
  - 「产物中 `node:` 内建命中数 0」—— `PV3-wp7-r1.log:65-66`、`deliver-wp7-r1.md:16`

**这是 IV1 行判据的直接不满足**：Target Revision 列要求「**并附**前端构建产物标识与依赖版本」，而最终提交上没有产物体标识。见 §7 缺口 5。

### 4.2 依赖版本实际取值

**声明值**（`clients/app/package.json`）：

| 位置 | 值 |
| --- | --- |
| `:8` | `"node": ">=22.12.0"` |
| `:13` | `"export:web": "expo export --platform web"` |
| `:20-27` | `expo ~54.0.0`、`expo-constants ~18.0.0`、`expo-router ~6.0.0`、`expo-status-bar ~3.0.0`、`react 19.1.0`、`react-dom 19.1.0`、`react-native 0.81.4`、`react-native-web ~0.21.0` |
| `:30-33` | `@types/react ~19.1.0`、`typescript ~5.9.2`、`vite ^7.1.0`、`vitest ^3.2.0` |

**解析值**（`clients/app/package-lock.json`，`lockfileVersion 3`，`:4`）：

| 包 | 解析版本 | 行 |
| --- | --- | --- |
| expo | `54.0.37` | `:5955` |
| expo-constants | `18.0.14` | `:6007` |
| expo-router | `6.0.24` | `:6135` |
| expo-status-bar | `3.0.9` | `:6425` |
| react / react-dom | `19.1.0` / `19.1.0` | `:9478` / `:9497` |
| react-native | `0.81.4` | `:9534` |
| react-native-web | `0.21.3` | `:9627` |
| @types/react | `19.1.17` | `:4384` |
| typescript | `5.9.3` | `:10971` |
| vite | `7.3.6` | `:11408` |
| vitest | `3.2.7` | `:11537` |

**`npm ci` 安装量**：`added 856 packages`（`reports/provision-mu3e.md:24`、`provision-wp6.md:26`、`provision-wp7.md:27`、`verification.md:51-54`）；我实测 `.worktrees/mu3e-merge/clients/app/node_modules/.package-lock.json` 的 `.packages` 项数 = **856**，与之一致。

**lockfile 摘要**：`clients/app/package-lock.json` / `package.json` 的 **sha256 或任何内容摘要全文 0 处被引用**（报告只引用「`package-lock.json` 11939 行」：`deliver-wp5a-r1.md:25`、`review-wp5a-r1.md:53`）。

**工具链实际报出值**：Node `v22.22.0`、npm `10.9.4`、vitest `3.2.7`（`AC3.log:18` 的 `RUN v3.2.7` 与 lockfile 一致，`^3.2.0` 满足 `3.2.7`，**无偏差**）、expo `54.0.37`、typescript `5.9.3`、rust/cargo `1.98.1`。

**Chromium 版本号：全仓无任何版本号。** 只有「真实 Chromium 内核（headless=new，127.0.0.1 安全上下文）」这类描述与可执行文件路径探测（`clients/app/scripts/run-browser-check.mjs:35-39`、`e2e/run-e2e.mjs:46-49`）。我在 `/tmp/tp4-e2e/e2e-evidence.json` 里看到实际浏览器是 `"C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe"`——**是 Edge 而非 Chrome，且无版本号**。浏览器版本是 E2E 审计的常规输入项，此处置空。

---

## 5. AC1 / AC2 / AC3 证据的可读性与提交绑定

| Check | 文件 | 行数 | target revision 绑定 | 我独立复核的结果 |
| --- | --- | --- | --- | --- |
| AC1 | `reports/AC1.log` | 16 | `:2` = `aed96a270441e9e9271a4c1ea9cd1ebcaadf5f9c` ✓ | **可读**；我复跑 `cargo test -p app --test node_link_e2e` → `6 passed; 0 failed`，与日志逐条一致 |
| AC2 | `reports/AC2.log` | 1927 | `:2` = `aed96a2…` ✓；`EXIT=0`（`:1927`） | **可读**；`daemon_lifecycle` 12/12（`:517-539`）；`check` 十道 + `check:rust` 三条全绿 |
| AC3 | `reports/AC3.log` | 29 | `:2` = `aed96a2…` ✓；`:3` cwd = `.worktrees/mu3e-merge`（我核实该 worktree HEAD = `aed96a2` 且与 main 无差异）✓ | **可读**；我复跑 `contract.test.ts` 22/22 与 `r11-fixed-vectors.test.ts` 11/11；并**独立验签**（见 §3.4） |

**三条替代检查证据均存在、可读、且绑定当前提交。** 无失效引用。

**但 AC2 的判据文本有一处不实**：`plan.md:1003`、`:1063`、`tasks.md:16`、`:63`、`verification.md:181`、`:751` 均写「`check:schemas` 与 `check:fixtures` 双向门禁」。**`check:fixtures` 这个脚本在 `package.json` 里根本不存在**（`scripts` 只有 `check:schemas` / `check:commands` / `check:errors` / `check:features` / `check:assets` / `check:acp` / `check:docs` / `check:boundaries` / `check:drift` / `check:agentic` 十道 + `check:rust`），`git log -S "check:fixtures" -- package.json` 为空（从未存在过）。实际的双向门禁由 `scripts/check-schema-fixtures.mjs` 承担（`:8` 的注释「`valid` fixtures must pass, `invalid` must fail」）。**判据引用了一个不存在的 Check ID**——`npm run verify` 仍然全绿，故非功能性缺陷，但 AC2 的判据文本与仓库实际不符，属「文档声称 ≠ 证据显示」。见 §7 缺口 8。

---

## 6. `verification.md` 须复述清单的核对（是否完整、是否诚实）

`## Final Assessment`（`:757-759`）目前**仍是 `NOT_APPLICABLE（由主 Agent 在 apply 内使用 … 入口填写）` 占位**，尚未填写。全文共 6 处明文「须在 `## Final Assessment` 复述」义务句：`:361`、`:387`、`:401`、`:535`、`:595`、`:613`（另有 `reports/merge-mu2-r3.md:214`、`:405`）。

### 6.1 用户点名的 9 组登记项

| 登记项 | 仍在？ | 被包装为「已解决」？ | 有复述义务？ |
| --- | --- | --- | --- |
| W2 跨角色并行窗口（用户 2026-10-04 裁决 b） | 是（`:375-387`） | **否**，明写「已知偏离」（`:383`） | 有（`:387`） |
| W3 跨角色并行窗口（沿用 W2 裁决 b） | 是（`:389-401`） | **否**（`:397`） | 有（`:401`） |
| RF-12 保持 FAIL（WP6 r4） | 是（`:613` Result=FAIL；`:630-632`「结论不改写」） | **否**；仅其载 2 项 P1 标为已由 WP7 r3 修复（`:617-618`） | 有（`:613`） |
| F47/F48/F49（Round 13 三项 MINOR） | 是（`:363-373`） | **否**，明写「接受为已知项、不再开新轮修正」（`:367`） | 有（`:387`） |
| `reports/review-w3-r2.md` 缺失 | 是，**但只在 `:387` 的括号里出现一次** | **否**（且无独立登记节、无原因说明） | 有（`:387`） |
| TP2 F1/F2（Windows/UNC 在 CI 零运行、跨窗口去重未覆盖） | 是（`:349-361`、`:615`；源码核实未修：`crates/storage-sqlite/tests/derived_events_behaviour.rs:527-556` 仍只 flush 一次、全文无 `C:\`/UNC 输入） | **否**，明写「留作后续非阻断项」（`:361`） | 有（`:361`、`:387`） |
| F13（Contract Freeze 多路径单元格） | 是（`:535`） | **是，且自相矛盾**：`reports/ddr-...-r13.md:12` 与 `verification.md:363-365` 判「已解决且彻底 / freeze 错误 3→0」，而 `:535` 仍是「未采纳的 MINOR…留作后续非阻断项」 | 有（`:387`、`:535`） |
| dispatch 台账缺执行者接收确认（用户 2026-10-03 裁决 a） | 是（`:595`） | **否**，明写「已知偏离」 | 有（`:595`） |
| 六项已知非阻断项（flipSignature 算据、`__pageErrors` 累积、配对不可达、`authenticatedInfo` 门控、`dispose()` 不可复活、CI-only job） | 全部仍在（`:60`/`:62`/`:618`/`:622`/`:631`/`:660`/`:751`） | **否** | 前两项无独立义务句（经 `:387` 的 F47/F48/F49 间接承载）；后四项**无明文义务句** |

**核对结论**：**登记是诚实的**——9 组中没有一组被包装成「已解决」（F13 除外，见下），flipSignature 的「行为正确、理由算错」、`__pageErrors` 的「纯诊断噪声」、配对不可达的「计划级归属遗漏」都如实写出（细节在 `reports/review-mu3e-candidate-r1.md:134-138`、`review-tp4-r2.md:277-278`、`review-wp7-r2.md:336-342`/`:299`/`:341-342`）。

**但有三处必须如实指出的问题**：

1. **F13 同文档自相矛盾**（`:535` vs `:363-365`）。`:535` 是 stale 文本，9.1 复述时不可原文照抄「留作后续非阻断项」。
2. **`review-w3-r2.md` 缺失的登记极薄弱且与其它行冲突**：`:387` 只有一句括号义务；但 `:117` 与 `:121` 两行仍把 `reports/review-w3-r2.md` 记为 **Report Path**，`reports/deliver-wp3-r3.md:274` 也把它作为 `source_evidence`，而 `:589` 的 Dispatch Reconciliation 把 WP3 第 3 轮的 Evidence 改指 `reports/review-mu2-candidate-r1.md`——**同一份缺失报告同时被引用为证据又被代证，且没有一句说明缺失原因**。
3. **merger 无法 `dispatch --ack` 的结构性缺陷在 `verification.md` 完全无登记节**（只在 `reports/merge-mu2-r1.md:74`、`merge-mu2-r3.md:47-59`、`merge-mu1b-r1.md:68`/`:396`）。它与 `:595` 登记的「缺执行者接收确认」是**两件不同的事**，不该被后者吸收。

---

## 7. 风险盲区清单（按严重度分级）

### P1 —— 证据完整性缺口（会影响 9.1 的结论，必须处理或如实复述）

**缺口 1：`PV1-main-mu3e.log` 是 FAIL，却被登记为 MU3e 的主分支回归证据，且复跑证据不在仓库内。**

- 事实：`reports/PV1-main-mu3e.log:561` = `test result: FAILED. 10 passed; 2 failed`；`:563` = `error: test failed, to rerun pass '-p app --test daemon_lifecycle'`；**全文无 `EXIT=` 行**（对比 `AC2.log:1927` 有 `EXIT=0`）。
- `verification.md:630` 只写「主分支回归 `reports/PV1-main-mu3e.log`（sha256 `fca5f057…797f`）」——**不记结果**；同行末尾写「**主分支首次回归 exit 101** 系 … 已知非确定性 flake … **复跑 1 次即 `12 passed; 0 failed`**」。
- 我核实：`.target-wt/` 下**没有** `mu3e-main` 目录，`reports/` 下**没有**任何 `PV1-main-mu3e` 的复跑日志。**「复跑 1 次即 12 passed」在仓库内无证据。**
- 对照：MU3d 与 MU3c 的对应行都显式写「**exit 0、零 FAILED**」（`verification.md:632`、`:633`），MU3e 行**没有**这句——登记方式与前序单元不一致，读者无法从行内判断该回归是否通过。
- 影响：`acceptance.md` 的「Project Checks and Resources」组要求「逐项读取实际完整命令、工作目录、**退出码**、运行环境…日志」。「主分支回归」是 `plan.md` Completion Criteria（`:1054`）要求的证据之一，现在这条证据是 FAIL 且无复跑留证。
- 我独立复现了 flake（§3.4），所以**结论大概率正确**（非产品缺陷），但**证据链断裂**。

**缺口 2：`reports/PV3-main-mu3e.log` 完全不存在。**

- `tasks.md:159`（6.62）明文要求「证据 `reports/PV1-main-mu3e.log`、`reports/PV3-main-mu3e.log`」；`tasks.md:151`/`:143`/`:135`/`:127` 对 MU3d/MU3c/MU3b/MU3a 有同构要求。
- `reports/` 下 **`PV3-main-*` 一个都没有**（`ls | grep "PV3-main"` = 0 条）。`verification.md:630` 也未登记其缺失。
- 影响：MU3e 是**最后一个**交付单元，其主分支回归缺了前端那一半。这与缺口 1 叠加，使「最终主分支固定提交的前端侧检查」只剩候选阶段的 `PV3-mu3e.log`。

### P2 —— 覆盖缺口（不影响当前正确性，但门禁在未来回归时不报警）

**缺口 3：`crates/core/src/derive.rs:311` 的前导 `..` 逃逸守卫在 core 单元测试里无独立判别力。**

- 我的变异 M-A（把守卫改成 `if false`）→ `derive::` **34 条全绿**。
- 只有 `storage-sqlite` 的 TP2 行为测试兜住（M-B 证明）。
- 与 `verification.md:325` 自己登记的教训同类（「本缺陷未被 `review-w3-r1` 与 `review-w3-r2` 两轮独立检视发现」）。建议在 `derive.rs` 里补一条**相对路径**形态的越界断言，使该守卫在 core 层也有一对一判别力。

**缺口 4：`routeContent` 的逐类覆盖落在一个无生产调用方的分类器上，且没有存储级逐类断言。**

- 我的变异 R20a（给 imported 的 `prompt` 开缺口）→ 19 条逐类断言**全绿**。
- 根因：存储级两条断言只推**同一个合成字符串**，且走「metadata 多一个键 `body`」路径（`r20-no-content-cache.test.ts:119-120`、`browser-checks.ts:300`）。
- 叠加事实：`routeContent`、`mayMarkInputAsSent`、`putImportedMetadata`、`PlatformPorts.importedContent` 的方法**均无生产调用方**（见 §2 重点 3）。
- 这与 `verification.md:618` 已登记的「配对无 UI 入口」属同一类（计划级接线遗漏），但 **`routeContent` 这一条没有被登记**。

**缺口 5：没有任何前端构建产物标识绑定最终提交（IV1 行明文要求）。**

- 见 §4.1。`PV3-mu3e.log` 无 `expo export`；无 `dist/`；`receipt-mu3e.md:15` 的 command 不含它。
- 历史产物体标识（`entry-e0bd8312…js` / `entry-998b3e41…js`）属**其它提交**，不能充当 IV1 证据。
- 另外：`run-browser-check.mjs` 与 `e2e/run-e2e.mjs` **均未接入任何门禁**（`clients/app/package.json` 的 `check` = `typecheck && test`；根 `package.json` 十道脚本与 `.github/workflows/ci.yml` 均不引用它们）。即「真实 Chromium 16/16」与「e2e 21/21」是**手工门禁**。

**缺口 6：`session.read` 分页的「同刻取最早」逻辑不在生产路径上。**

- `toLoadedPage().nextBefore`（`paging.ts:143`，唯一使用 `earliestCursor` 平票逻辑处）**未被 store 消费**（`client-store.ts:385-407` 只用 `loaded.result/messages/hasEarlier`）。
- 生产续取游标来自 `client-store.ts:614` → `page.earliestCursor` → `domain/conversation.ts:133-141` 的 `messages[0]`（直接取首条，**无时间比较**）。
- 我变异验证过的 P2（同刻平票）**覆盖的是非生产路径**。生产路径的同刻情形无测试（`domain/domain.test.ts:199-250` 只有单条与空页）。
- `[INFERENCE]` 因服务端按 `(createdAt, messageId)` 升序返回是契约前置条件，`messages[0]` 在契约成立时**结果正确**；但「前置条件被违反时前端仍能取到真正最早一条」这层保护在生产上不生效。

### P3 —— 文档与证据不一致 / 登记薄弱（不影响运行，影响验收可追溯性）

**缺口 7：`verification.md:117`/`:121` 引用不存在的 `reports/review-w3-r2.md`。**
- 该文件全仓不存在（`find . -name "review-w3-r2*"` 为空，`reports/` 只有 `review-w3-r1.md`）。
- 同一行组里 `:589` 用 `reports/review-mu2-candidate-r1.md` 代证，但**无一句说明缺失原因**。
- 影响：`acceptance.md` 的「Handoff Traceability」组要求「核对路径可读」——这条不满足。

**缺口 8：`check:fixtures` 是一个不存在的 Check ID，却被四处判据引用。**
- `plan.md:1003`、`:1063`、`tasks.md:16`、`:63`、`verification.md:181`、`:751` 都写「`check:schemas` 与 `check:fixtures` 双向门禁」。
- `package.json` 无 `check:fixtures`；`git log -S` 确认从未存在。实际承担者 `scripts/check-schema-fixtures.mjs` 已含双向语义（`:8`）。
- 属判据文本缺陷，非功能性。

**缺口 9：`verification.md:535` 的 F13 条目与 `:363-365` 自相矛盾（stale）。**

**缺口 10：merger 无法 `dispatch --ack` 的结构性缺陷在 `verification.md` 无登记节。**
- 只在 `reports/merge-mu2-r1.md:74`、`merge-mu2-r3.md:47-59`、`merge-mu1b-r1.md:68`/`:396`。

**缺口 11：Chromium 版本号全仓缺失。**
- 只有可执行文件路径（实测为 `msedge.exe`，即 Edge 而非 Chrome）与「headless=new」描述；无版本号。E2E 审计的常规输入项。

**缺口 12（本次已闭合）：`reports/IV1.log` 已产出。**
- `plan.md:1012`/`tasks.md:164` 指定的证据路径已由 validator 亲自执行并落盘（23,815 字节），含目标版本核实、三项重点的测试命令与原始输出、独立验签、flake 表征与变异摘要。

---

## 8. 对任务 9.1（最终验收）的具体输入

**A. 结论口径**

1. IV1 三项重点**均 PASS**，且经独立变异验证（不是复述报告）。
2. `verification.md` 的 `## Final Assessment` 目前仍是占位；填写时**必须覆盖** `:361`、`:387`、`:401`、`:535`、`:595`、`:613` 六处义务句，外加 `reports/merge-mu2-r3.md:214`/`:405` 的二次落列。
3. **不可**把缺口 1/2 当作「已通过」：`PV1-main-mu3e.log` 是 FAIL，`PV3-main-mu3e.log` 不存在。9.1 需明确记录：MU3e 的主分支回归是**以「已知非确定性 flake + 复跑」的口径**接受的，且**复跑证据不在仓库内**（要么补留证，要么在 Final Assessment 如实写成未留证）。
4. **不可**把缺口 5 当作已满足：IV1 行要求「附前端构建产物标识与依赖版本」，而**产物体标识在最终提交上不存在**。要么在 9.1 前跑一次 `npx expo export --platform web` 并登记 bundle 文件名与字节数，要么在 Final Assessment 明确记「该字段未满足」。

**B. 必复述清单（按 `verification.md` 的义务句）**

| # | 条目 | 出处 |
| --- | --- | --- |
| 1 | W2 跨角色并行窗口（用户 2026-10-04 裁决 b） | `:375-387` |
| 2 | W3 跨角色并行窗口（沿用 W2 裁决 b） | `:389-401` |
| 3 | RF-12 如实记为 FAIL（其所载 2 项 P1 已由 WP7 r3 修复） | `:613`、`:630-632` |
| 4 | F47 / F48 / F49（Round 13 三项 MINOR，接受不再修） | `:363-373` |
| 5 | `reports/review-w3-r2.md` 缺失（**并说明 `:117`/`:121` 的失效引用如何处置**） | `:387`、`:117`、`:121` |
| 6 | TP2 F1/F2（Windows/UNC 在 CI 零运行、跨窗口去重未覆盖） | `:349-361` |
| 7 | F13 —— **以 `:363-365` 与 `ddr-r13:12` 为准（已解决且彻底）**，并指出 `:535` 为 stale | `:363-365` vs `:535` |
| 8 | dispatch 台账缺执行者接收确认（用户 2026-10-03 裁决 a） | `:595` |
| 9 | 六项已知非阻断项（flipSignature 算据、`__pageErrors` 累积、配对不可达、`authenticatedInfo` 门控、`dispose()` 不可复活、CI-only job 未执行） | `:60`/`:62`/`:618`/`:622`/`:631`/`:660`/`:751` |
| 10 | **本报告新增**：缺口 1（PV1-main-mu3e FAIL 无复跑留证）、缺口 2（PV3-main-mu3e.log 缺失）、缺口 5（无产物体标识）、缺口 3/4（两处覆盖缺口）、缺口 8（`check:fixtures` 不存在） | `reports/IV1.md` §7 |

**C. 检查项**

1. 运行 `npx --quiet --no-install openspec-agentic e2e check --change sync-scope-and-pwa-client --json`（只读加 `--no-write`），确认 `not-applicable` 的降级批准可追溯（`plan.md:1028` 记录了用户 2026-10-03 的原话与来源）。
2. 运行 `workflow check --stage final --json`，核对 `## Final Assessment` 的 `agentic-assessment` 块。
3. 按 `acceptance.md` 的「Delivery and Versions」组核对每个 Merge History 的 Candidate 都有 premerge PASS（`receipt-mu1a/mu1b/mu2/mu3a/mu3b/mu3c/mu3d/mu3e.md` 与 `premerge-*-gate.json` 均在）。
4. `## Dispatch Reconciliation` 中每个工作包状态须为 `merged`（当前 11 行全部 `merged`）。
5. 逐项核对 `plan.md` 的 User Deliverables（`:1061-1066`）：机器合同资产、权威文档、前端工程源码三项已完成；「静态托管入口」明确为**不产出**。

**D. 明确不构成阻断的项（避免 9.1 过度判 FAIL）**

- `server::sync` 未落地、`session.read` 分页无服务端实现：**显式 Non-Goal**（`proposal.md` 的 `non_goals`），不是缺口。
- 节点级事件无投递通道：用户 2026-10-03 裁定「甲」方案，已收窄需求（`design.md` 的 Risks 第 1 条）。
- `deps`/`advisories`/`secrets` 三个 CI-only job 未本地执行：`verification.md:660`/`:751` 已明写「未执行、未声称通过」，**不得**反过来声称通过。

---

## 9. 只读纪律自证

- 未提交任何内容；未改任何分支、代码或规划文件。
- 所有临时变异（`crates/core/src/derive.rs` ×4 次、`clients/app/src/sync-client/paging.ts` ×3 次、`clients/app/src/sync-client/imported-content.ts` ×2 次、`clients/app/src/platform/local-cache.web.ts` ×1 次）均已 `cp` 还原并以 `md5sum` 逐一对齐备份值。
- 最终 `git status --porcelain`（主检出）= `?? .target-wt/`、`?? .worktrees/`、`?? openspec/changes/sync-scope-and-pwa-client/`（**均为预期未跟踪项**）；`.worktrees/mu3e-merge` 的 `git status --porcelain` 为**空**。
- 唯一写入是两份任务交付物：`openspec/changes/sync-scope-and-pwa-client/reports/IV1.md` 与 `reports/IV1.log`。
- 为跑通 AC1，在隔离的 `CARGO_TARGET_DIR=.target-wt/iv1` 内构建了 `acpr-fake-acp-agent`（AC1 的**前置**，`crates/app/tests/node_link_e2e.rs:1092` 要求它与测试同 target 目录）；未触碰仓库源码。

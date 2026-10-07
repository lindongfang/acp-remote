# MU3e 候选独立只读检视（merge 类型，Round 1）

## 0. 检视输入与版本

| 项 | 值 |
| --- | --- |
| 候选 worktree | `D:\Project\acp-remote\.worktrees\mu3e-merge`（分支 `merge/mu3e-candidate-r1`） |
| 候选提交 | `aed96a270441e9e9271a4c1ea9cd1ebcaadf5f9c`（`git rev-parse HEAD` 复核一致） |
| 目标基线 | `1e50bdeacccfa257901c88a7277e9aae818da395`（`refs/heads/main`） |
| 共同祖先 | `f6ea252e09fd65834a67fe7123c786beec339209` |
| 来源 1 | `feat/tp3` = `6e8307096629dcfd3fa63ac20559e25b0a3e7a7a` |
| 来源 2 | `feat/tp4` = `0a19d39b4c63b723c7e7d7edbaaf64e60cd82e83` |
| 组装 | `1e50bde` --no-ff→ TP3（`0d23b4c`）--no-ff→ TP4（`aed96a2`） |
| 本轮性质 | **只读**。未提交、未切换/改任何分支、未新建/删除 worktree、未做任何实现或测试文件编辑。仅新增本报告文件。 |

结论先行：**PASS，建议合入。**

- **0 CRITICAL、0 MAJOR、0 MINOR（就候选交付物本身）。** 三处已知 MINOR 均为**既存的、已登记的、非阻断**项，在候选上形态未变，**不阻塞合入**。
- 候选路径集 == 三来源并集（24 = 12 + 9 + 3，逐路径唯一归属），**无夹带、无缺失**。
- 两个合并提交均为**纯合并**（用 `git merge-tree --write-tree` 复算，树逐字节相同）。
- `R15-6` 在候选上**存在、断言未放松，且因实现被修好而转绿**。

---

## 1. 合并语义未变（核心）

### 1.1 逐文件内容一致性（blob 级三方比对）

命令：

```
git diff --name-only f6ea252 aed96a2            # 候选相对共同祖先的 24 路径
git rev-parse <rev>:<path>                      # 每路径在 aed96a2 / 6e83070 / 0a19d39 / 1e50bde 的 blob SHA
```

对全部 24 个路径，候选上的 blob SHA **逐字节等于且仅等于**它在唯一来源上的 blob SHA：

| 唯一来源 | 路径数 | 路径 |
| --- | --- | --- |
| `feat/tp4` (`0a19d39`) | 12 | `clients/app/e2e/`（`run-e2e.mjs`、`tsconfig.json` 与 `page/` 下 10 个文件） |
| `refs/heads/main` / WP7 r3 (`1e50bde`) | 3 | `src/features/directory-model.ts`、`src/features/r15-directory-model.test.ts`、`src/components/r15-directory-list.test.ts` |
| `feat/tp3` (`6e83070`) | 9 | `src/r11-fixed-vectors.test.ts`、`src/r14-rebuild-snapshot.test.ts`、`src/r17-r19-store-contract.test.ts`、`src/features/r20-input-mark-production-path.test.ts`、`src/platform/r20-no-content-cache.test.ts`、`src/state/r12-connection-transitions.test.ts`、`src/state/r13-command-idempotency.test.ts`、`src/sync-client/r13-dispatch-idempotency.test.ts`、`src/sync-client/r14-replay-recovery.test.ts` |

**UNMATCHED = 0**：没有任何路径的内容既不属于 TP3 也不属于 TP4/main-r3——即没有任何文件在合并中被改写或产生 conflict-resolution 语义。由于三来源两两无重叠（见 §1.3），被合并的文件根本不存在需要 resolve 的 hunk。

### 1.2 两个合并提交是纯合并（无夹带 conflict resolution 编辑）

`git show --no-patch --pretty=raw`：

- `0d23b4c`：parent = `1e50bde`、`6e83070`（两个 parent，纯合并）
- `aed96a2`：parent = `0d23b4c`、`0a19d39`（两个 parent，纯合并）

用查询式复算合并结果（临时索引，不写工作区）证明**未夹带额外编辑**：

```
git merge-tree --write-tree 1e50bde 6e83070   → 1b822370c1c1920c3784c4f12dc7970078a62739
git rev-parse 0d23b4c^{tree}                    → 1b822370c1c1920c3784c4f12dc7970078a62739   # 逐字节相同
git merge-tree --write-tree 0d23b4c 0a19d39    → 732b2e16cfaade2a59fcc0850b12121d7591660f
git rev-parse aed96a2^{tree}                    → 732b2e16cfaade2a59fcc0850b12121d7591660f   # 逐字节相同
```

两次复算树与候选实际树**完全相同**，且 `merge-tree` 无冲突报告 → 两个合并提交的内容恰为「首 parent 内容 ∪ 次 parent 内容」，**没有任何第三个来源的编辑**。

另：`git numstat f6ea252 aed96a2` 中**只有 2 个文件有删除行**，且删除行数恰为该文件在 `f6ea252→1e50bde` 上的删除行数（`directory-model.ts` −2；`r15-directory-list.test.ts` −3），新增行数亦与 `1e50bde` 侧一致 → 与 §1.1 的 blob 相等互为佐证。

### 1.3 路径集恰为三来源并集，且无夹带

```
comm -23 <候选24> <并集>   → 空（无多余）
comm -13 <候选24> <并集>   → 空（无缺失）
参数统计：tp3=9  tp4=12  main-r3=3
两两交集：tp3∩tp4=0  tp3∩r3=0  tp4∩r3=0（grep -Fx -f 计数均为 0）
24 = 9 + 12 + 3（三方交集 = ∅）
```

候选相对基线 `1e50bde` 的差异为 **21 路径**（TP3 的 9 + TP4 的 12），全部落在上表内，**无夹带**。`git log --graph aed96a2` 显示候选历史恰为 `f6ea252 → 221e76e → 4ada2a2 → 1e50bde → (0d23b4c←6e83070←16ef844) → (aed96a2←0a19d39←b07a863)`，即 main 链 + 两条特性链，无其它提交。

### 1.4 跨边界分派

候选引入的跨边界值均为**测试资产**（vitest 用例、e2e 检查与 fake-host），不进入生产代码的两条改线是：

- `directory-model.ts` 的 `buildDirectoryDetailModel` → `DirectoryDetail.tsx`。消费侧分派点已读：三支 `loading` / `unknown_directory` / `ready` 均有显式分支（`DirectoryDetail.tsx:32` loading / `:40` unknown_directory / `:50` ready）；修复后「目录存在但无会话」走 `ready` 支，落在 `model.sessions.length === 0` 的 `data-directory-sessions="empty"` 分支（`DirectoryDetail.tsx:70`），**不再被折叠进 `unknown_directory`**。无静默丢弃。
- e2e 检查注册表 `e2e/page/index.tsx:19` 的 `ALL_CHECKS = [...COMPOSITION_CHECKS, ...R15_CHECKS, ...R16_CHECKS, ...R18_CHECKS]`，由 `runChecks`（`support.tsx:134`）顺序执行，逐条 try/catch；检查数为 4+7+3+7 = **21**，与运行结果一致。

---

## 2. 断言在候选上仍然成立且未被放松

### 2.1 用例数：与来源自报一致，无丢失

- `vitest`：候选 **37 文件 / 458 passed**（`reports/PV3-mu3e.log`）。文件数分解经独立复核：`f6ea252` 28 → `6e83070` **37**（+9，全为 TP3 新增 `.test.ts`）→ `aed96a2` 37（TP4 只加 `e2e/`，不增 vitest 文件）。**无 `.test.ts` 在合并中被覆盖**。
- `e2e`：21 条检查（4 CR + 7 R15 + 3 R16 + 7 R18），与 `0a19d39` 上作者自报一致。
- 新增测试文件中 **`.only` / `.skip` / `it.todo` / `xit` / `xdescribe` 出现数为 0**（grep 命中仅 `run-e2e.mjs:317` 的 `process.exit(1)`，非跳过标记）。

### 2.2 `R15-6`：仍在、未放松、因实现被修好而绿

**(a) 用例内容与 `feat/tp4` 一致（未被削弱）。**
`git diff 0a19d39 aed96a2 -- clients/app/e2e/page/checks/r15-directory.ts` **输出为空** → 候选上该文件与 TP4 tip 逐字节相同。R15-6 的两条断言原样保留（`r15-directory.ts:235-241`；用例名见 `:226`）：

- `expectThat(textOf(query(container, '[data-directory-sessions="empty"]')).length > 0, "目录存在但无会话时必须给出「还没有会话」")`
- `expectThat(query(container, '[data-directory-detail-state="unknown_directory"]') === null, "目录存在时不得呈现「链接已失效」")`

`0a19d39` 相对 `b07a863` 对该文件的改动只有 2 处等待条件（`+8/−2`），且都是**收紧**（见下），断言本体一字未动。

**(b) 等待条件已收紧，不是放松。** `0a19d39` 把 R15-5/R15-6 的等待从 `[data-route="dir-detail"]` 改为 `[data-route="dir-detail"][data-directory-alias]`（`r15-directory.ts:209` R15-5 / `:232` R15-6）。理由经独立核对成立：占位 `RuntimeUnavailable` 也发 `data-route`，故旧条件在首帧占位即真、与 `start()` resolve 形成竞态，**会把「本用例要抓的缺陷」偷换成一个时序红**；`data-directory-alias` 只在真实详情页算出模型后出现（`DirectoryDetail.tsx:42` / `:51`；`loading` 支 `:32-35` **不发**该属性）→ 等待点后移，断言只会更严格。

**(c) 转绿是因为实现被修好，不是断言被放松。** 修复在实现侧、属 main（WP7 r3，`1e50bde`），**不在 TP4 分支**：

- `f6ea252`（缺陷态）：`const directories = list.kind === "ready" ? list.directories : []; const directory = directories.find(...)` → 「有目录但无会话」时 `buildDirectoryViews` 返回 `empty/directories_without_sessions` 且**不携带目录项**（`src/domain/directory.ts:119-121`），于是 `directory === undefined` → 落到 `unknown_directory` → 渲染「链接已失效」，`data-directory-sessions="empty"` **永不可达**。
- `1e50bde`（修复态）：新增 `findDirectoryByAlias(list, workspaces, alias)`（`directory-model.ts:187-205`），在 `directories_without_sessions` 态按快照 `workspaces` 投影 zero-count 行（该态定义即「没有任何会话归属任何已登记目录」，计数可证为零）；`no_directories` 仍返回 `undefined`（别名确实失效，保留 `unknown_directory`）。调用点改为 `const directory = findDirectoryByAlias(list, input.resources.workspaces, input.alias)`（`:222`）。
- 判别力佐证：把实现回退到 `f6ea252` 后单元层 **2 failed / 18 passed**（`reports/PV3-wp7-r3.log:132-133`），失败原文为渲染出 `data-directory-detail-state="unknown_directory"` + 「它可能已被删除」。
- 我在候选上实跑 R15 两个单元文件：**2 files / 20 passed**（含新增的两条对照用例）。

**因此：R15-6 绿 = 实现被修好；断言未被放松、用例未被删除。** 且 `R15-7`（别名失效仍是 `unknown_directory`、不得呈现 `empty`）作为**对照**仍在，证明修复没有把两种呈现反向合并。

### 2.3 我自跑的 e2e（稳定性）

在候选 worktree `clients/app` 下顺序独立执行 `node e2e/run-e2e.mjs`：

| 运行 | 结果 | 退出码 | 红集合 | R15-6 |
| --- | --- | --- | --- | --- |
| #1 | **21/21 passed** | 0 | ∅ | **PASS** |
| #2 | **21/21 passed** | 0 | ∅ | **PASS** |
| #3 | **21/21 passed** | 0 | ∅ | **PASS** |

（每次运行含完整 21 条检查；CR-1/CR-2/CR-3/CR-4 与 R15-1..7、R16-1..3、R18-1..7 全绿，单次约 181 s。）与 `PV3-mu3e.log` 的 21/21 独立一致；历史上 flaky 的 CR-1 在候选上 3/3 未复现误红。

---

## 3. 三处已知 MINOR 的处置判定

| # | 项 | 候选上是否仍在 | 登记位置 | 是否阻塞 | 判定 |
| --- | --- | --- | --- | --- | --- |
| M1 | `fake-host.ts` 的 `flipSignature` 注释算据错误 | **仍在**（`clients/app/e2e/page/fake-host.ts:106-107` 原文「置起 r 的最高位后 r ≥ 2^255 > 曲线阶 n」） | `verification.md:60`「待复核的两点」第 1 点；`verification.md:615`（RF-17）「2 MINOR：注释算据、`__pageErrors` 跨用例累积」；`reports/review-tp4-r2.md:277`（TP4R2-1） | 否 | **MINOR，不阻塞** |
| M2 | `support.tsx` 的 `pageSnapshot` 附带的 `window.__pageErrors` 跨用例累积 | **仍在**（`clients/app/e2e/page/support.tsx:73` 把**全局累积**的 `window.__pageErrors` 全文附上；收集器由 `run-e2e.mjs:214-216` 注入，进程内**只初始化一次**、跨检查不清空） | `verification.md:615`；`reports/review-tp4-r2.md:278`（TP4R2-2） | 否 | **MINOR，不阻塞**（纯诊断噪声：只进失败明细文本，不影响任何断言的通过/失败判定） |
| M3 | TP3 侧 4 条非阻断观察 | **仍在**（`r12-connection-transitions.test.ts:136` 的 `ALL_EVENT_KINDS` 仍是手写字面量表、非由允许来源推导；`mayMarkInputAsSent` 仍**全仓无生产调用方**，仅 `sync-client/imported-content.ts:76` 定义 + `sync-client/index.ts:86` re-export；`r20-input-mark-production-path.test.ts` 只覆盖「已发送」一半；`r11-fixed-vectors.test.ts:622` 的负向红法仍走 `field_order` 硬拒绝路径） | `verification.md:614`（RF-16）「4 条非阻断观察记入报告 §6」；正文 `reports/review-tp3-r2.md` §6（4 条） | 否 | **MINOR / 观察，不阻塞** |

**M1 细化**：该注释算据确实错误——P-256 的 `n ≈ 2^256 − 2^224`，故 `2^255 < n`，`r ≥ 2^255 > n` 不成立。真正机理是「`r` 被改变 ⇒ `(r', s)` 不再是该 transcript 的合法签名，x 坐标比对必然不等」。但**行为正确**：`flipSignature` 翻的是解码后首字节最高位（`fake-host.ts:111`），实测新 flip 后 `r'` 仍落在 `[1, n)` 的比例为 4000/4000（`review-tp4-r2.md` §TP4R2-1 独立复算），CR-3 的负向对照仍稳定成立。修复的**效果**成立、仅**书面理由**算错，属可订正的注释问题。

**判定：三处均不阻塞合入。** 理由：(1) 三者均非本次合并引入（M1/M2 由 `feat/tp4` 提供、M3 由 `feat/tp3` 提供），合并只是逐字节搬运；(2) 均已登记在 `verification.md`，具备验收可追溯性；(3) 三者均不改变任何断言的通过/失败语义。M1 建议在最终验收时或后续维护轮就地订正注释（一处注释、无行为改动）。

---

## 4. WP 级复核结论在候选上是否仍然成立

- 路径三方无重叠（§1.3）→ 不存在文本级冲突；`merge-tree` 复算无冲突报告，进一步排除语义级冲突。
- WP 级复核所依赖的前提在合并后仍然成立：`review-tp3-r2` 依赖的 9 个 TP3 文件在候选上逐字节相同（§1.1）；`review-tp4-r2` 依赖的 12 个 `e2e/` 文件在候选上逐字节相同（§1.1）；两者所依赖的 `directory-model.ts` 修复来自 `1e50bde`、在候选上逐字节相同。
- **「各自绿、合起来红」：未出现。** 以候选实跑为准：`tsc --noEmit` **exit 0**（我独立复跑）；`e2e` **21/21、exit 0**（我独立 2 次）；TP3 的 vitest 用例与 main 的 R15 用例共同存在于 37 文件 / 458 passed 的单一运行中（`PV3-mu3e.log`），无交叉污染。合并把 TP3/TP4 **两个独立来源**的测试资产叠加到同一条运行时上，未产生新的失败。

---

## 5. 合入建议

**建议合入。** 0 CRITICAL / 0 MAJOR，候选满足：合并语义未变（逐文件 blob 相等、合并树可复算、无夹带）、用例未丢失未放松（尤其 R15-6 因实现被修好而绿）、e2e 稳定（我 2 次 21/21）。三处 MINOR 均既存、已登记、不阻塞。

**给最终验收（任务 9.1）需要留意的点：**

1. **R15-6 已转绿是修复成功的标志，而非门禁失效。** `run-e2e.mjs` 的退出码逻辑为 `failed.length > 0 → exit 1`，**不区分「已知红」与「真回归」**；修复后期望红集合由 `{R15-6}` 变成 **∅**。验收时**不得**只看退出码 0 就认定无回归——必须核对红集合为空、且 R15-6 **用例仍在**（它在 `clients/app/e2e/page/checks/r15-directory.ts`，共 21 条检查）。作者已刻意不把豁免固化进门禁（`deliver-tp4-r2.md` §6.2、`verification.md:60` 末段），宜沿用该口径。
2. **R15-6 与 R15-7 必须成对保持。** 二者是「两种呈现互不冒充」的正反两面；若后续只保留 R15-6，实现退化成「一律不报 `unknown_directory`」将不再被抓住。合入前已确认两条都在且都绿。
3. **合同/规划摘要的两处待复核**（`verification.md:60`）已在本轮闭环：第 1 点（`flipSignature` 注释算据）确认为 MINOR、建议就地订正注释；第 2 点（CR-1 稳定性）已由 `review-tp4-r2` 的 4/4 绿与我本轮 2/2 绿共同覆盖。验收时可将该「待复核的两点」标记为已复核。
4. **尚未登记的合并证据。** `verification.md` 在各单元合入后会移除内联 premerge 块，MU3e 的候选/门禁证据需按 MU3c/MU3d 先例（`reports/receipt-mu3d.md` 等）持久化，并在合入后以 `--no-ff` 记录合并提交。
5. **MINOR 的后续处置**：M1（注释算据）建议在 main 上单独订正，避免后续有人据此写出错误的上界校验；M2/M3 可留作已知项登记备查。

---

## 6. 只读性与纪律自证

- 未提交、未切换/修改任何分支、未新建/删除 worktree、未做任何实现或测试文件编辑。
- 候选 worktree `git status --porcelain` **为空**；候选 `HEAD` 仍为 `aed96a270441e9e9271a4c1ea9cd1ebcaadf5f9c`。
- 复算合并结果使用 `git merge-tree --write-tree`（查询式，临时索引），未写工作区、未改引用。
- 未创建任何临时探针文件；本报告为唯一新增文件。
- 未运行会对仓库产生副作用的构建/提交；`tsc --noEmit` 与 `vitest run`（只读校验/受限范围）及 `node e2e/run-e2e.mjs`（在 `tmpdir` 内建 profile 与产物）均不写仓库。

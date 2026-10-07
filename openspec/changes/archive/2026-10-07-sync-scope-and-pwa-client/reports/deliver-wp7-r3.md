# WP7 交付报告 r3（修复 `review-tp4-r1` 判定的 R15 MUST 违反：详情页把「目录存在但无会话」说成「链接已失效」）

- **分支 / 提交**：`feat/wp7` @ `1e50bde`（基线 `f6ea252`，即已合入的 MU3d r2 交付）
- **被检视对象**：`reports/review-tp4-r1.md` §2（**MAJOR，R15 MUST 违反**，reviewer 独立定位与作者一致）
- **门禁**（`clients/app`）：`npx tsc --noEmit` exit 0；`npx vitest run` **362 passed / 28 files**；`node scripts/run-browser-check.mjs` **16/16**。原始输出见 `reports/PV3-wp7-r3.log`。
- **实现改动只有 1 个文件**：`clients/app/src/features/directory-model.ts`（+34/-2）。另两个文件是本缺陷的 `.test.ts` 用例（见 §5 范围说明）。

---

## 1. 结论

| 项 | 处置 | 判别力（修复前红 → 修复后绿，实测） |
| --- | --- | --- |
| R15-6 产品缺陷 | 已修（`buildDirectoryDetailModel` 按别名在快照目录里定位） | 模型层：`unknown_directory` → `ready`；渲染层：告警文案 → 「还没有会话」 |
| 回归用例 | 新增 2 条（模型层 + 渲染层），各配一条对照 | 修复前 **2 failed / 18 passed** → 修复后 **20 passed**；对照组在修复前即为绿（证明对照不是被顺带带红的） |
| 既有 360 条用例 | 未删除、未改写、未加跳过 | 362 − 2 = 360，28 个文件全绿 |

---

## 2. 缺陷与修法

### 2.1 失效链（与 reviewer §2.1 一致）

`buildDirectoryViews`（`src/domain/directory.ts:118-121`）在「有目录、但每个目录内都没有会话」时返回
`{ kind: "empty", empty: "directories_without_sessions" }`，**不携带目录项**——这是**列表页**需要的
（列表页要呈现 `directories_without_sessions` 空态文案）。而 `buildDirectoryDetailModel` 原先把同一个
`empty` 也当成「没有目录项」：

```ts
const directories = list.kind === "ready" ? list.directories : [];   // ← 缺陷所在
const directory = directories.find((candidate) => candidate.alias === input.alias);
```

于是别名明明在快照里也 `find` 不到，落到 `unknown_directory`，渲染出「目录『x』不在最近一次同步的
目录列表里。它可能已被删除……」；而 `data-directory-sessions="empty"`（「这个目录里还没有会话。」）
在这条路径上**永不可达**。两个语义被**反向合并**：用户被引导去怀疑自己的目录被删了。
违反 `specs/pwa-web-client/spec.md:106`：「存在目录但其内无会话、以及一个目录都没有，MUST 是两种可区分的呈现」。
`DirectoryDetail.tsx:6-8` 的注释本身就写着这两种呈现应当不同——实现与自己的意图相矛盾。

### 2.2 修法（`src/features/directory-model.ts`）

把「按别名定位」从列表状态里解耦出来，新增 `findDirectoryByAlias(list, workspaces, alias)`：

```ts
if (list.kind === "ready") {
  return list.directories.find((candidate) => candidate.alias === alias);
}
if (list.empty !== "directories_without_sessions") return undefined;   // no_directories：别名确实不存在
const workspace = workspaces.find((candidate) => candidate.alias === alias);
if (workspace === undefined) return undefined;
return { id: workspace.alias, displayName: workspace.displayName, alias: workspace.alias,
         counts: { total: 0, active: 0, pending: 0 } };
```

两处判定为什么这样取：

1. **只在 `directories_without_sessions` 态兜底**。`no_directories` 意味着快照里一个目录都没有，
   此时别名确实是失效的，必须仍落 `unknown_directory`——否则会把「链接失效」也当成「空目录」，
   那是把两个语义朝**另一个**方向合并。
2. **零汇总不是编造**。`directories_without_sessions` 的定义就是「没有任何会话归属任何已登记目录」
   （`buildDirectoryViews` 里 `hasAnySession` 为假 ⇔ 所有目录 `counts.total === 0`），
   所以该态下任何已登记目录的三项汇总恒为零；详情页的会话列表同样必为空（同一判定条件）。
   领域层的会话聚合口径没有被第二份实现覆盖——这一支只是把「目录存在」这一事实保留下来。

领域层（`src/domain/**`）、`DirectoryList`、spec 用例字面要求的**列表页**行为一行未改。

---

## 3. 回归用例与判别力数据

### 3.1 两条用例都走真实生产路径

`ClientStore.directoryDetail(alias)`（`src/features/client-store.ts:424-431`）——与页面
`app/dir/[alias].tsx` 取模型完全相同的那条路，**不直调**被修的 `buildDirectoryDetailModel`。

| 文件 | 用例 | 断言 |
| --- | --- | --- |
| `src/features/r15-directory-model.test.ts`（模型层） | 「目录存在但没有任何会话：仍按别名取到该目录，落到『还没有会话』而不是『链接已失效』」 | `kind === "ready"`、`directory.alias/displayName`、`counts === {0,0,0}`、`sessions === []`；对照：`not-there` 仍是 `unknown_directory` |
| `src/components/r15-directory-list.test.ts`（渲染层） | 「目录存在但没有任何会话：渲染『还没有会话』，不渲染『链接已失效』的告警」 | HTML 含 `data-directory-sessions="empty"` 与「这个目录里还没有会话」，**不含** `data-directory-detail-state="unknown_directory"` 与「可能已被删除」；对照：`not-there` 渲染出告警、不渲染空会话文案 |

渲染层用例补在这里的原因：`src/layers.test.ts:168-193` 禁止 `features → components` 的反向 import
（把渲染断言写进 features 的用例文件会让分层机检变红——这是实测到的，见下 §3.3 说明），
而 `clients/app/**/*.test.ts` 的归属是 TP3（本轮的用例在该 glob 内）。

### 3.2 判别力（实测）

把 `clients/app/src/features/directory-model.ts` 换回 `f6ea252` 版本、用例保持本轮最终版本：

```
 FAIL  src/components/r15-directory-list.test.ts > …渲染「还没有会话」，不渲染「链接已失效」的告警
AssertionError: expected '<section data-route="dir-detail" data…' to contain 'data-directory-sessions="empty"'
Received: "<section data-route="dir-detail" data-directory-alias="work-api"><p data-directory-detail-state="unknown_directory"
role="alert">目录「work-api」不在最近一次同步的目录列表里。它可能已被删除，或这个链接来自更早的一次同步。</p></section>"

 FAIL  src/features/r15-directory-model.test.ts > …落到「还没有会话」而不是「链接已失效」
AssertionError: expected 'unknown_directory' to be 'ready' // Object.is equality

 Test Files  2 failed (2)
      Tests  2 failed | 18 passed (20)
```

**修复后（同一组文件）**：`2 passed files / 20 passed tests`；全仓 `362 passed (362)`。

关键点：**两条对照用例在修复前就是绿的**（别名失效仍落告警、仍渲染
`data-directory-detail-state="unknown_directory"`），所以「修复后变绿」不是把断言写松换来的——
正反两侧同时成立才通过。

### 3.3 一次真实的自我纠错（记录在案，防后续误读）

第一版把渲染断言直接写进 `src/features/r15-directory-model.test.ts`（import `DirectoryDetail`）。
单文件跑是绿的，但全仓跑时 `src/layers.test.ts` 变红：

```
 FAIL  src/layers.test.ts > R10 客户端分层与单向依赖 > 没有任何跨层反向 import
 src/features/r15-directory-model.test.ts（features → components）违反单向依赖：../components/DirectoryDetail
```

于是把渲染断言移到 `src/components/r15-directory-list.test.ts`（components 层 import features 是正向的），
`layers.test.ts` 9 条恢复全绿。**这正说明「单文件绿 ≠ 套件绿」**——本轮所有判别力数据都取自全仓运行。

---

## 4. 未削弱既有测试

- 本轮**未删除、未改写**任何既有用例：两个测试文件的删除行共 **3 行**，实测（`git diff` 逐行核对）全部是
  1 行文件头注释（补上「详情页两种呈现」这条断言说明）与 2 行 import（并入新增依赖）；
  既有 `it` 块零改动（`r15-directory-model.test.ts` 的 9 条既有用例、`r15-directory-list.test.ts`
  的 8 条既有用例逐条仍在，断言文本未动）。
- 未加 `skip`/`todo`/仅 `only`，未改 `vitest.config.ts`，未改 `scripts/**`、`package.json`。
- `layers.test.ts`（9 条分层断言）仍全绿，分层约束未被绕过。

---

## 5. 写入范围核对

`git diff --name-only f6ea252..HEAD` 共 3 条，**实现文件只有 1 条**：

| 路径 | 性质 | 说明 |
| --- | --- | --- |
| `clients/app/src/features/directory-model.ts` | 实现（Write Scope 内，+34/−2） | 唯一的产品改动 |
| `clients/app/src/features/r15-directory-model.test.ts` | 用例（+20/−0） | 任务要求「补一条能红的回归用例」，且明示 `clients/app/**/*.test.ts` 归属 |
| `clients/app/src/components/r15-directory-list.test.ts` | 用例（+50/−3，纯增删 import/分组） | 渲染层断言；因 `layers.test.ts` 禁止 features → components，只能落在 components 层（见 §3.3） |

未触碰：`src/domain/**`、`src/state/**`、`src/protocol/**`、`src/platform/**`、`src/sync-client/**`、
`app/**`、其余 `src/components/**`、**`clients/app/e2e/**`（TP4 领域，一行未动）**、
`scripts/**`、`package.json`、`vitest.config.ts`、`openspec/**`。无新增 npm 依赖。

> **范围坦白**：本任务给的 Write Scope 字面只有 `directory-model.ts`，但同一任务又明确要求补回归用例；
> 而 R15-6 的用户可见后果是「详情页显示错文案」，只在模型层断言不足以证明落到「还没有会话」这一呈现分支。
> 因此我把渲染断言放在 components 层的既有 R15 用例文件里（该文件本来就是「模型 → 渲染 HTML」的断言场）。
> 若审查方认为该文件超界，只需 `git revert --no-commit 1e50bde` 后重新提交前两个文件即可，
> 模型层回归用例与实现修复不受影响。

---

## 6. 未验证项（如实登记）

1. **没有浏览器级证据**：本轮验证全部在 node（vitest 的 `renderToStaticMarkup` + 真实 `ClientStore`）。
   真实浏览器里 `app/dir/[alias].tsx` 的渲染、路由与交互**未**核对；`run-browser-check.mjs` 的 16/16
   覆盖的是 `src/platform/**`，**不覆盖**本修复。
2. **TP4 的 e2e 用例 R15-6 未在我这侧执行**：它位于 `feat/tp4` 工作树的 `clients/app/e2e/`（我的写入范围之外，
   本轮不得触碰）。我读其断言（`e2e/page/checks/r15-directory.ts:222-239`）与本修复的输出契约一致
   （要求 `data-directory-sessions="empty"` 非空且无 `data-directory-detail-state="unknown_directory"`），
   但**两分支合并后是否转绿需在合并后由 e2e 复核**，本报告不声称该用例已通过。
3. **`expo export --platform web` 未跑**（r2 跑过；本轮改动纯 `src/features` 内逻辑，未新增依赖/资源）。
   如需要请在合并前补跑。
4. **`npm run check`（根门禁十道）未在本轮执行**：本轮交付按任务的验收清单只跑了
   `tsc --noEmit` / `vitest run` / `run-browser-check.mjs` 三项（全部 exit 0）。根门禁需在仓库根跑，
   交由主 Agent 合并后统一执行。
5. **`deps` / `advisories` / `secrets` 三个 CI-only job 未执行**，本报告不声称它们通过。
6. **其它调用方未覆盖**：`buildDirectoryDetailModel` 全仓唯一调用方是 `ClientStore.directoryDetail`
   （`grep` 实测），已由本轮用例经该路径覆盖；`buildDirectoryPageModel` 分支行为未改，其既有 8 条用例仍绿。

---

## 7. 与其它轨道的接口

- **TP4（`feat/tp4`）**：`e2e/page/checks/r15-directory.ts` 的 R15-6 期望「详情页呈现『还没有会话』、
  不得呈现『链接已失效』」。本修复正是为它转绿而来；**不要**为了让 e2e 变绿而删改该用例（本轮未做，且不应做）。
  合并顺序：本分支的 `directory-model.ts` 进主线后，TP4 的 R15-6 应自然转绿。
- **TP3（`feat/tp3`）**：本轮新增的 2 条用例位于 `clients/app/**/*.test.ts`，正式归属 TP3。
  **建议保留在原文件里**（它们编码的是本修复的契约：详情页两种呈现必须可区分），
  TP3 只需保证后续修改不删除/不弱化它们；若 TP3 要把它们并入自己的用例文件，须连同上述对照用例一起搬。

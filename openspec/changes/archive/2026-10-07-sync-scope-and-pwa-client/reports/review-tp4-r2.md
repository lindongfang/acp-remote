# TP4 独立复核报告（r2）

- 被检视：`feat/tp4` @ `0a19d39`（本轮基线 `b07a863`；工作包基线 `f6ea252`），worktree `D:\Project\acp-remote\.worktrees\tp4`
- 上一轮：`reports/review-tp4-r1.md`（FAIL，可返修：2 MAJOR + 4 MINOR）
- 本轮交付：`reports/deliver-tp4-r2.md`
- 检视方式：**独立只读复核 + 自行复现变异/探针**。未提交、未修改 `feat/tp4`；全部临时探针与源文件改动已还原，
  最终 `git status --porcelain` 为空（worktree 与主检出均无残留）。
- **结论：PASS —— 可带一条已知红 `R15-6` 合入，不阻塞 MU3e。0 CRITICAL、0 MAJOR、2 MINOR（均为注释/诊断噪声）。**

---

## 0. 结论摘要

| 维度 | r1 | 本轮我的独立判定 |
| --- | --- | --- |
| Write Scope | 合规 | **合规**：本轮 5 文件全在 `clients/app/e2e/`；全分支 12 路径全在 `e2e/`、**0 行删除**、零实现改动 |
| CR-1 稳定性 | MAJOR（3/4 次误红） | **已修好**：我连跑 **4 次**，红集合恒为 `{R15-6}`，CR-1 **4/4 绿** |
| 同类等待（CR-3/CR-4、R15-5/6、CR-2、R16-3） | 未点 | **已修好**：新选择器逐条核对只由真实页面发出；未改动的等待也逐条核对（§2） |
| CR-3 的伪造签名 | 未发现 | **根因成立**：我独立复算旧 `flipSignature` 的 no-op 比例 **0.247**（≈1/4）；新修法**效果正确、理由写错**（§3，发现 1） |
| M3 定性 | MAJOR（要求撤销） | **撤销成立**，且我独立复现三层变异：`tsc` 0 诊断、17/21、R15-1/2/3 因**检测到泄露**而红（§4） |
| 退出码语义 | MINOR（要求写明） | **已写明**（`deliver-tp4-r2.md` §6.2、`deliver-tp4-r1.md:273-277` 就地更正）✓ |
| R15-6 | 已知红、非阻塞 | **未被弱化**：断言零改动、4/4 次稳定红且红法是语义断言（§5.2） |
| 诊断性修复 | 建议 | **真的可定位**：快照改为抛错当时取；我注入页面级异常后明细出现 `｜页面错误：…`（§5.3） |

---

## 1. 本轮改动范围与断言完整性

`git diff b07a863..0a19d39` = **5 文件 / +119 / −15**，全部在 `clients/app/e2e/` 下：

```
clients/app/e2e/page/checks/composition-root.ts | 30 +  5 -
clients/app/e2e/page/checks/r15-directory.ts    |  8 +  2 -
clients/app/e2e/page/checks/r16-create.ts       | 16 +  4 -
clients/app/e2e/page/fake-host.ts               | 19 +  2 -
clients/app/e2e/page/support.tsx                | 46 +  2 -
```

全分支 `git diff --name-only f6ea252..0a19d39` 的 12 条路径**全部**在 `clients/app/e2e/` 下
（`grep -v '^clients/app/e2e/'` 无输出），`git diff --numstat f6ea252..0a19d39` 的删除行数为 **0**
→ **零实现改动、既有资产未被削弱**，Write Scope 合规。

**断言完整性（逐行核对 `git diff -U0`）**：`r15-directory.ts` 与 `composition-root.ts` 的改动**只有**
4 处等待选择器 + 注释；`r16-create.ts` 只换了取模型前的等待方式。**没有任何一条断言被删除、放宽或改写**
（唯一被改的字符串是 CR-4 的等待标签「未配对时目录页仍应渲染」→「未配对时真实页面渲染」）。

---

## 2. MAJOR-1（CR-1 不稳定）：独立判定 —— 已真正堵住

### 2.1 我自己的运行（完整输出、无过滤、连续 4 次）

命令：`cd .worktrees/tp4/clients/app && node e2e/run-e2e.mjs`（每次新起浏览器与 bundle，逐次红项清单来自
`e2e-evidence.json` 与原始日志）

| 运行 | 结果 | 红集合 | 退出码 |
| --- | --- | --- | --- |
| #1 | 20/21 | **{R15-6}** | 1 |
| #2 | 20/21 | **{R15-6}** | 1 |
| #3 | 20/21 | **{R15-6}** | 1 |
| #4 | 20/21 | **{R15-6}** | 1 |

**CR-1 / CR-2 / CR-3 / CR-4 在 4 次里 4 次全绿**；红集合**恒为 `{R15-6}`**。
r1 的 TP4-6（「除 CR-1 外至少还有一条偶发红」）在本轮代码状态下**未复现**：4 次运行里除 R15-6 外没有任何一条红。
（作者把它归因到 CR-3 的 1/4 no-op 与 CR-2 的抢跑重叠；前者我已独立复算证实，后者仍是
作者的 [INFERENCE]——见 §6。）

### 2.2 新等待选择器是否**只由真实页面**发出（逐条核对发射点）

`RuntimeUnavailable`（占位）只渲染 `<section data-route={route} data-runtime="unavailable" role="alert">`
（`src/components/RuntimeUnavailable.tsx:24`）——**只发 `data-route` 与 `data-runtime`**。据此逐条核对：

| 新等待选择器 | 唯一发射点 | 占位是否发出 | 判定 |
| --- | --- | --- | --- |
| `[data-directory-state="loading"]`（CR-1） | `src/components/DirectoryList.tsx:39`（`kind === "loading"` 分支） | **否** | 安全 |
| `[data-directory-list="directories"]`（CR-2 新增等待） | `src/components/DirectoryList.tsx:57`（`kind === "ready"` 分支） | **否** | 安全 |
| `[data-connection-state]`（CR-3、CR-4） | `src/components/ConnectionBadge.tsx:22`、`src/components/HostConnectionPanel.tsx:44` | **否** | 安全 |
| `[data-route="dir-detail"][data-directory-alias]`（R15-5/R15-6） | `src/components/DirectoryDetail.tsx:42`（unknown_directory）与 `:51`（ready）；`:34` 的 loading 分支**不发** | **否** | 安全 |

`[data-connection-state]` 的两处发射点都在「store 已装配」之后：
`app/index.tsx:19` 在 `store === null || state === null` 时提前 return 占位，`ConnectionBadge` 出现在
`:30`；`app/dir/[alias].tsx:23-24` 同理，徽标在 `:34`。因此该选择器**只能**在真实页面渲染后成立，
正是 CR-1 原先缺的那道闸门。

**R15-6 的等待选择器不会把红法变成超时**（这是我重点检查的一点）：该场景的缺陷态渲染的正是
`unknown_directory` 分支，而该分支**带** `data-directory-alias`（`DirectoryDetail.tsx:42`），
等待因此会成立、随后由语义断言判红——我 4 次运行的明细首行确实是
`目录存在但无会话时必须给出「还没有会话」`，**不是**「等待…超时」（见 §5.2）。

### 2.3 未改动的等待也逐条核对过（作者 §2 表格的复核）

`[data-empty-state]`（`EmptyState.tsx:23`）、`[data-content-origin]`（`DirectoryList.tsx:33`）、
`[data-host-identity="paired"]`（`HostConnectionPanel.tsx:68`）、`[data-create-sheet]`
（`CreateSessionSheet.tsx:88`）、`[data-diagnostics-drawer]`（`DiagnosticsDrawer.tsx:71`）、
`[data-directory-detail-state="unknown_directory"]`（`DirectoryDetail.tsx:43`）、
`[data-action="open-create-sheet"]`（`DirectoryDetail.tsx:57` 的 ready 分支）——
**没有一处由 `RuntimeUnavailable` 发出**。作者「同类缺陷已一并修掉」的说法核实成立。

### 2.4 一处需要写明的语义变化（不是缺陷）

CR-1 里紧随其后的显式断言（`[data-runtime="unavailable"] === null`）现在**近乎不可达**：
能通过新等待就说明真实 `DirectoryList` 已渲染。但**守门任务由等待条件承担，且比原来更严**
——`start()` 失败时页面停在占位、等待必然超时判红。**语义未削弱**（甚至因为
「绿 = 真的等到真实页面」而更强）。r1 的判断（「断言本身是对的，坏的是等待条件」）在本轮得到验证。

### 2.5 CR-2 的基线偶发超时——作者标为 [INFERENCE]，我接受该标注

作者的机制解释（CR-1 抢跑返回时真实单例的 IndexedDB/WebSocket 工作仍在飞，与 CR-2 抢通道）**没有被单变量实验证明**，
报告已如实标 `[INFERENCE]`。我这边能确证的是：**修后 4/4 次 CR-2 全绿**。
这条不构成合入阻塞，但也不应被当成「已证明」。

---

## 3. CR-3 的伪造签名：no-op 比例独立复算与**真正机理**判定

### 3.1 我按 `src/sync-client/base64url.ts` 的真实编解码独立复算

方法：用 WebCrypto 生成真实 P-256 密钥与 **4000 条真实签名**，用仓库的 `encodeBase64Url`
编码、按旧 `flipSignature`（`b07a863`：`末字符 === "A" ? "B" : "A"`）改坏、再用仓库的
`decodeBase64Url` 解码，与原字节逐字节比较：

```
真实签名样本数: 4000
旧 flip 的 no-op 次数: 988  比例: 0.247          ← 作者报 0.24325，同为 1/4 的抽样估计
末字符分布: A:988  Q:1032  g:948  w:1032         ← 末字符只可能是这 4 个
末字节低 2 位为 0 的比例（=末字符 A）: 0.247
均匀随机 64 字节上的 no-op 比例: 0.2545
```

**结论：作者的 0.24325 与我复算的 0.247 是同一个量**（理论值恰为 **1/4**：64 字节 = 512 位，
编码成 86 个字符后末字符只有高 2 位有效，其值只能是 `A/Q/g/w`；而旧 flip 只在末字符为 `A` 时
把 `A`→`B`，两者高 2 位同为 `00`，解码回来逐字节相同 → 验签通过、连接照常走到 online）。
抽样差 0.0068 属 4000 样本的正常波动（0.4σ）。**根因判定成立，我确认作者发现的是真问题。**

### 3.2 新修法的机理：**理由写错，效果正确**

我实测的两个事实：

```
新 flip 后 r 未改变次数: 0        （4000/4000 必然改变 r）
新 flip 后 r' 仍落在 [1, n) 的次数: 4000 / 4000   （即几乎总是不越界）
n = 115792089210356248762697446949407573529996955224135760342422259061068512044369
n > 2^255 ? true
```

- **作者的算据不成立**：注释写「置起 r 的最高位后 `r ≥ 2^255 > 曲线阶 n`」，
  但 P-256 的阶 `n = 0xFFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551 ≈ 2^256 − 2^224`
  **大于** `2^255`（我复算：`2^255 = 57896044618658097711785492504343953926634992332820282019728792003956564819968 < n`）。
  实测也印证：4000/4000 条样本里 `r′ = r ^ 2^255` **仍落在 `[1, n)` 内**——绝大多数情况下根本没有越界，
  因此「越界被拒」不可能是使验签失败的机理。
- **真正的机理**：`bytes[0] ^= 0x80` 必然改变 `r`（`r′ ≠ r`），于是 `(r′, s)` 不再满足该 transcript 的
  ECDSA 验签等式——验签计算的 `R′` 依赖 `r′`，要求 `x(R′) mod n == r′`，而 `r′` 已被改变，
  等式不成立。严格说是**概率 1−ε 的必然**（ε 与穷举伪造同级，可忽略），而不是「范围检查必然拒绝」。
  仅约 `2^−32` 的子集（`r ∈ [n − 2^255, 2^255)`）里 `r′ ≥ n`，那部分会先被范围检查拒掉——
  **两条路径都走 `verified === false`**：`composition.ts:243-245` 的 `.catch(() => false)` 与
  `connection.ts:406-409` 的 `#block("host_identity_changed", …)` 收敛到同一个阻断态，
  所以 CR-3 的断言不受这一小差异影响。
- **判定：效果正确（CR-3 从 1/4 概率失效变为实际必然失效），理由写错**（见发现 1）。
  「必然改变签名」这半句是对的，错的只有后半句的算据与「确定性」措辞。

### 3.3 副作用核对：无

新修法先解码再按 `encodeBase64Url` **规范重新编码**，得到的仍是 86 字符、末字符低 4 位补零的规范串，
客户端的严格解码器 `decodeBase64Url`（`base64url.ts:60-77`，只取整字节、丢弃补位）能正常解开
→ 负向样本是「**编码规范但签名无效**」，正是这条用例想要的形态。
作者在 §3.3 登记的「客户端 `decodeBase64Url` 接受非规范补位」是**既有产品行为**、
不在本轮 Write Scope，处置（只登记、不顺手改）正确。

---

## 4. M3 定性撤销：核实通过，且我独立复现了三层变异

### 4.1 报告是否真的改对了 —— 是

- `deliver-tp4-r2.md` §5 的表格已把 M1 记为「**无效变异**（未触达呈现面）」、M3 记为「有效变异，红法干净」，
  并明确「撤销 r1 §5 的定性」「R15 的负向断言判别力充分，不需要补测试」。
- `deliver-tp4-r1.md:181-212` 与 `:273-277` 已**就地**加更正块（删除线 + 「r2 更正，已撤销」），
  把原来那句「本轮遗留的判别力缺口 / 下一轮应补上双层变异」显式作废。
  → 作者「已就地撤销」的说法核实成立（更正落在 `deliver-tp4-r1.md`，不是 `review-tp4-r1.md`）。

### 4.2 我自己的三层变异复现（不采用作者给的任何数字）

按与报告相同的三层接缝注入（三层都类型合法，页面**结构完好**，不是崩溃态）：

1. `src/domain/directory.ts`：`DirectoryView` 加 `readonly path?: string | undefined;`，
   `buildDirectoryViews` 的 map 里 `path: (workspace as unknown as { path?: string }).path`；
2. `src/features/directory-model.ts`：`DirectoryRowModel` 加同名字段，`toRow` 带出 `path: directory.path`；
3. `src/components/DirectoryList.tsx`：在别名标签**旁边**加一个合法文本节点
   `<span data-directory-path-label>{directory.path ?? ""}</span>`。

结果：

```
npx tsc --noEmit                 → exit 0（0 诊断：变异体是结构完好、可正常渲染的代码）
node e2e/run-e2e.mjs             → 17/21，退出码 1
FAIL R15-1 …… 目录页渲染结果泄露了本机路径：文本里出现了 "D:\\"
FAIL R15-2 …… 同名消歧用了路径片段：文本里出现了 "D:\\"
FAIL R15-3 …… 断连后的渲染结果泄露了本机路径：文本里出现了 "D:\\"
FAIL R15-6 （已知红）
```

**红法干净**：三条红的明细都是**负向断言检测到泄露**，且随附快照显示页面正常渲染出
`nav` / `ul[data-directory-list]` / 目录行（`tsc` 零诊断 + 快照有 DOM），**不是页面崩溃、不是等待超时**
（若是崩溃/超时，消息会是「等待「目录列表渲染」超时」或「找不到元素」）。
与 r1 reviewer 的结果一致（r1 为 16/21，多出的那条就是当时 CR-1 的 flaky 红；本轮修掉后为确定的 17/21）。

**结论：M3 定性撤销成立；R15 的负向断言判别力充分，无需补测试。** 变异已还原（`git status` 为空）。

---

## 5. 其余必须核实项

### 5.1 退出码语义已写明 ✓

`deliver-tp4-r2.md` §6.2 明确写出：`e2e/run-e2e.mjs` 的 `failed.length > 0 → exitCode = 1`
**不区分「已知红」与「真回归」**，并给出维护口径（期望红集合 = `{R15-6}`；出现集合外红才是真回归；
修复后全绿、退出码 0 是成功而非门禁失败），还解释了为什么**不**把已知红清单写进 runner。
`deliver-tp4-r1.md:273-277` 也就地更正了原来「期望退出码 1」的写法。
我的 4 次运行退出码均为 1、结果均为 20/21，与该语义一致。

### 5.2 R15-6 未被弱化或删除 ✓

- **代码层**：本轮对 `r15-directory.ts` 的改动只有 2 处等待选择器 + 注释（`-U0` 逐行核对），
  R15-6 的两条断言（`[data-directory-sessions="empty"]` 必须有文本、
  `[data-directory-detail-state="unknown_directory"]` 必须缺席）**一字未动**。
- **运行层**：4/4 次稳定红，红法是语义断言而非超时；失败快照显示页面停在
  `data-directory-detail-state="unknown_directory"`、文案为「目录「alpha」不在最近一次同步的目录列表里……」，
  即产品缺陷（`src/features/directory-model.ts:189-193`，`buildDirectoryViews` 在「有目录无会话」时返回
  `kind: "empty"` 导致 `directories = []`）**被正确的用例正确报出**。
- **范围层**：该缺陷由另一路 `feat/wp7`（`1e50bde`）修复，不在 TP4 的 Write Scope，
  本轮保持红是正确的——**没有为了让 e2e 变绿而删改用例**。
- **污染层**：`clients/app/vitest.config.ts:14` 的 `include: ["src/**/*.test.ts"]` 不含 `e2e/`，
  因此 e2e 的退出码 1 不进入主套件。

### 5.3 诊断性修复是否真的让偶发红可定位 ✓（含我自己的探针）

**(a) 「抛错当时固定快照」确实生效。** 我 4 次运行里 R15-6 的失败明细**不再是**空的
`<div id="root"></div>`，而是带上了完整组件树（`main > nav > ConnectionBadge` + `section[data-route="dir-detail"]`，
其中就有 `data-directory-detail-state="unknown_directory"`）——一次就能看出「页面渲染了链接失效」。
M3 变异运行的明细同样带出了渲染出的目录行。这一条**是 r1 时无法定位偶发红的直接原因**，现在成立。

**(b) `window.__pageErrors` 通路确实生效。** 我另做一个只读探针：在 `ContentOriginNote`
（`DirectoryList.tsx:32`）的渲染路径上抛 `probe-page-error`，重跑套件，失败明细出现：

```
等待「目录列表渲染」超时（10000ms）｜页面：…<div></div>｜页面错误：Uncaught Error: probe-page-error
```

即：**页面级异常从此进入失败明细**（此前只能看到「等待 X 超时」）。探针已还原，worktree 干净。

**(c) 一处归因噪声**：该数组**跨用例累积**（同一次运行里，后续用例的明细会重复列出前面用例产生的错误，
我的探针运行里同一条错误重复 1→5 次）。这不影响断言正确性，但可能把定位引向错误方向 → 发现 2。

---

## 6. r1 六条次要项的处置复核

| ID | r1 级别 | 作者处置 | 我的判定 |
| --- | --- | --- | --- |
| TP4-1 | MAJOR | 换掉被占位命中的等待选择器（并一并修同类 4 处） | ✅ **成立**：我 4/4 次红集合恒为 `{R15-6}`；新选择器逐条核对只由真实页面发出 |
| TP4-2 | MAJOR | 就地撤销「判别力缺口」定性 + 补三层复现 | ✅ **成立**：`deliver-tp4-r1.md:181-212` 显式作废原表述；我独立复现 17/21 且红法干净（检测到 `D:\` 泄露） |
| TP4-3 | MINOR | M1 改记「无效变异（未触达呈现面）」 | ✅ 成立（按 `specs/pwa-web-client/spec.md:106`，MUST NOT 约束的是显示/编辑/推导） |
| TP4-4 | MINOR | 写明退出码语义 | ✅ 成立（§5.1） |
| TP4-5 | MINOR | M2 数字改为 17/21 | ✅ 与「CR-1 修好后 = 21 − 3(R18-3/4/5) − 1(R15-6)」自洽；我未复跑 M2（r1 已独立复现过该变异） |
| TP4-6 | MINOR | 归因到 CR-3（1/4 no-op，已修）与 CR-2（[INFERENCE]） | ⚠️ **合理但部分未证**：CR-3 的 1/4 我已独立复算证实；CR-2 的机制仍只是推断，报告已如实标注。本状态下 4 次运行**未再复现**第三条红 |

r1 关于「给 `waitFor` 加『再等一帧』」的建议，作者判定「不需要」——**我同意**：
`waitFor` 用 `setTimeout(25ms)` 宏任务轮询（`support.tsx:198-207`），本轮三条偶发红各有实因
（等待条件被占位命中 / 抢跑重叠 / 伪造签名 1/4 no-op），加「再等一帧」只会掩盖真因。

---

## 7. 分级发现清单

| ID | 级别 | 位置 | 现象 | 影响 | 复现方式 | 处置建议 |
| --- | --- | --- | --- | --- | --- | --- |
| **TP4R2-1** | MINOR（注释/理由） | `clients/app/e2e/page/fake-host.ts:106-107` | 新注释把「篡改后验签必然失败」的算据写成「`r ≥ 2^255 > 曲线阶 n`」，但 P-256 的 `n > 2^255`；实测 `r′` 在 4000/4000 样本里**仍落在 `[1, n)`**，即失败与越界无关 | 不影响行为（CR-3 依旧实际必然失败），但会把后续读者引向错误的机理，属于本项目反复出现的「理由与证据不符」 | 复算 `n` 与 `2^255`；统计 `r′ ∈ [1, n)` 的比例 | 改成：r 被改变 ⇒ `(r′, s)` 不再是该 transcript 的合法签名；并去掉「确定性」措辞 |
| **TP4R2-2** | MINOR（诊断） | `clients/app/e2e/page/support.tsx:72-77` | `pageSnapshot()` 把**全局累积**的 `window.__pageErrors` 全文附上，不区分本次用例 | 归因噪声：前序用例错误会出现在后续用例的失败明细里（探针实测 1→5 次重复），可能误导定位 | 注入一个渲染期异常后连跑，观察后续用例明细里的重复条目 | 在 `runChecks` 每条用例开始前清空该数组（或在检查开始时记录基线长度、只附增量） |

两条均为**已按「红集合 = {R15-6}」口径核对过的非阻断项**，不改变合入结论。除此之外，本轮 diff 中
**没有**发现任何「声称覆盖但实际没覆盖」或「断言不可复现」的问题。

---

## 8. 合入结论（明确）

**PASS：TP4 r2 可以合入，不阻塞 MU3e；应带一条已知红 `R15-6`。**

1. **两条 MAJOR 真的堵住了**，且我用**与作者不同的数据**独立验证：
   - CR-1 稳定性：我连跑 **4 次**，红集合**恒为 `{R15-6}`**，CR-1/CR-2/CR-3/CR-4 4/4 全绿；
     新等待选择器与其余被改的等待**逐条核对只由真实页面发出**（占位 `RuntimeUnavailable` 只发 `data-route`/`data-runtime`）。
   - CR-3 的伪造签名：旧 flip 的 no-op 比例我复算为 **0.247**（≈1/4，作者 0.24325 同量级），
     **根因成立**；新修法**效果正确**（r 必变、验签实际必然失败），但注释里的算据错误（发现 1）。
   - M3 定性：撤销成立，我独立复现三层变异 → `tsc` 0 诊断、17/21、R15-1/2/3 因**检测到 `D:\` 泄露**而红
     （非崩溃、非超时）。
2. **`R15-6` 保持红是正确的**：产品缺陷（`directory-model.ts:189-193`）、语义断言稳定报出、
   不在 vitest 主套件收集范围、由 `feat/wp7` 修复。**不构成合入阻塞。**
3. **合入前建议（非阻断）**：修 §7 的两条 MINOR（注释算据、`__pageErrors` 的按用例隔离）。

**一句话**：r1 的两条 MAJOR 都已被真正堵住（不是「换一种方式通过」）——
CR-1 的等待条件现在只能由真实页面满足，CR-3 的伪造签名不再有 1/4 概率是 no-op，
M3 的定性也与实测一致；剩下两条 MINOR 只涉及一处注释算据与诊断噪声。

---

*本报告由独立 reviewer 产出；未修改 `feat/tp4`。所有临时探针（三层 M3 变异、页面级异常注入）
均已还原，最终 `git status --porcelain` 为空。*

# TP4 测试设计报告（r2，修复轮）

- 变更：`sync-scope-and-pwa-client`（前端 Web/PWA 客户端）
- 工作包：TP4，基线 `f6ea252`，上一轮交付 `b07a863`，本轮提交 `0a19d39`（分支 `feat/tp4`）
- 写入范围：仅 `clients/app/e2e/`（本轮只改测试与测试夹具，**实现目录零改动**）
- 运行入口：`cd clients/app && node e2e/run-e2e.mjs`（真实 Chromium，退出码 0/1）
- 被检视来源：`reports/review-tp4-r1.md`（2 MAJOR + 4 MINOR）

---

## 0. 本轮结论摘要

| 项 | 上一轮 | 本轮 |
| --- | --- | --- |
| CR-1 稳定性 | 基线 4 次运行里 **3 次误红**（等待选择器被占位命中） | 修后**连跑 5 次全绿**（20/21，唯一红是已知产品缺陷 R15-6） |
| CR-3 稳定性 | 偶发红（基线 4 次里 2 次；本轮全过程 10 次里 4 次） | **根因定位并修掉**：夹具的「伪造签名」有 1/4 概率是 no-op（见 §3） |
| CR-2 稳定性 | 基线 4 次里 2 次超时 | 修后 5 次全绿（机制见 §1.3，标注为推断） |
| R16-3 | 依赖 25ms 的偶然时间 | 改为等**场景的认证结果真的落地**（§2.3） |
| M3 定性 | r1 记为「判别力缺口」 | **撤销**；三层变异能红且红法干净（§5） |
| M1 | r1 记为「未生效的变异」 | 改记为**无效变异**（未触达呈现面）（§5） |
| 退出码语义 | r1 §8 把「退出码 1」当门禁期望值 | 显式写明语义与维护含义（§6） |

**本轮提交**：`0a19d39 test(frontend): TP4 r2——修掉等待选择器竞态、伪造签名失效与失败快照不可定位`

`git diff --name-only f6ea252..HEAD`：12 条路径**全部**在 `clients/app/e2e/` 下（0 条越界），
`git status --porcelain` 为空。

---

## 1. MAJOR 1（CR-1 不稳定）：等待条件被占位页面命中

### 1.1 根因（与 reviewer 的定位一致，本轮独立复现）

`RuntimeUnavailable`（`src/components/RuntimeUnavailable.tsx:24`）渲染
`<section data-route={route} data-runtime="unavailable">`，而 `app/index.tsx:19` 在
`store === null` 时正是 `<RuntimeUnavailable route="dirs" />`。于是 CR-1 的等待条件
`[data-route="dirs"]` 在**首帧占位**时就成立，紧随其后的
`expectThat([data-runtime="unavailable"] === null)` 与 `composition.start()` 的 resolve
形成真竞态。

**本轮实测（未改任何文件，HEAD `b07a863`）**：

| 基线运行 | 结果 | 红项 |
| --- | --- | --- |
| #1 | 18/21 | **CR-1**、CR-2、R15-6 |
| #2 | 19/21 | **CR-3**、R15-6 |
| #3 | 18/21 | **CR-1**、CR-2、R15-6 |
| #4（临时加了耗时探针） | 18/21 | **CR-1**、CR-3、R15-6 |

CR-1 在 4 次里红了 3 次，失败明细为
`组合根装配完成后页面必须离开 RuntimeUnavailable｜页面：<div id="root"></div>`，
且探针显示该用例只用了 **79ms**（等待在 25ms 就返回）——与「等待抢跑占位」完全一致。

### 1.2 修法（reviewer 验证过的最小修法）

等待条件改为**只有真实 `DirectoryList` 才发**的
`[data-directory-state="loading"]`（`src/components/DirectoryList.tsx:39`，
`RuntimeUnavailable` 不发这个属性），断言一字不动。

### 1.3 修后稳定性证据（最终代码状态，连跑 5 次）

| 运行 | 结果 | 红项 |
| --- | --- | --- |
| #1 | 20/21 | R15-6（已知产品缺陷） |
| #2 | 20/21 | R15-6 |
| #3 | 20/21 | R15-6 |
| #4 | 20/21 | R15-6 |
| #5 | 20/21 | R15-6 |

**CR-1 5/5 绿**，且绿得正确（真的等到了真实页面渲染）。

> 关于 CR-2：基线里 2/4 次以「等待『连接机进入 online』超时（10000ms）」变红。
> 最可能的机制是 **CR-1 抢跑返回时真实 `_layout.tsx` 单例的 IndexedDB/WebSocket 工作仍在飞**，
> 与 CR-2 的握手抢同一条 IndexedDB 通道；CR-1 现在会等到真实页面渲染（即 `start()` 已 resolve）
> 才返回，这个重叠窗口随之关闭。**这条是 [INFERENCE]，未单独证明**：我能确证的是
> 修后 5 次运行里 CR-2 全绿，且本轮把失败快照改成「抛错当时取」（§4），
> 若它再红，明细里会直接带上页面状态与页面错误。

---

## 2. 同一缺陷类：其余被占位命中的等待（本轮一并修掉）

reviewer 只点了 CR-1，但 `[data-route=...]` 被占位命中是一**类**缺陷。逐点核对后：

| 位置 | 原等待条件 | 占位是否命中 | 修法 |
| --- | --- | --- | --- |
| CR-1 | `[data-route="dirs"]` | 是（`route="dirs"`） | `[data-directory-state="loading"]` |
| CR-3（断言） | `[data-route="dirs"]` | 是 | 先等 `[data-connection-state]`（`ConnectionBadge`），断言保留 |
| CR-4 | `[data-route="dirs"]` | 是 | 先等 `[data-connection-state]` |
| R15-5 / R15-6 | `[data-route="dir-detail"]` | 是（`route="dir-detail"`） | `[data-route="dir-detail"][data-directory-alias]`（模型已算出） |
| R16-3 | `[data-route="dir-detail"]` | 是 | 改为等**场景的认证结果落地**（见 §2.3） |
| CR-2 | 无 DOM 等待 | — | 补一条 `[data-directory-list="directories"]` 等待，再断言行数 |

未改动的等待都已确认只由真实页面发出：`[data-directory-list]`、`[data-empty-state]`、
`[data-degraded-entry]`、`[data-create-sheet]`、`[data-action="open-create-sheet"]`、
`[data-host-identity]`、`[data-directory-detail-state="unknown_directory"]`。

### 2.3 R16-3 的闸门改成「认证结果真的落地」

R16-3 的两次读数原本靠「等 `[data-route="dir-detail"]`」——而该等待在首帧占位就成立，
真正起作用的是那 25ms 的偶然等待时间。改为按场景等**该场景的不变量**：

- `scopes !== null`：等 `store.state.scopes !== null`（`auth.authenticated` 已写入）；
- `scopes === null`：服务端不会下发认证，等 `host.deviceProofs.length === 1`
  （客户端确实走完了验签并签出设备证明，即握手到了「认证本该被决定」的位置）。

（本轮中途我曾把它改成等 `[data-route="dir-detail"][data-directory-alias]`，
实测在未认证场景下永远等不到——因为该场景模型停在 `loading` 分支、不发 `data-directory-alias`。
两次运行都因此超时；这属于我自己的中间态，已改成上面的不变量等待。）

---

## 3. MAJOR（新发现）：CR-3 的「伪造签名」有 1/4 概率是 no-op

这是本轮查清 CR-3 偶发红的根因，reviewer 未发现（他们验证的是「变异生效时确实变红」）。

### 3.1 现象与证据

CR-3 的场景 `tamperHostProof: true` 期望连接进入 `identity_changed`。它偶发变红：
基线 #2、探针 #4、修后第 4 次运行（共 4/10 次）报
`等待「连接进入 identity_changed 阻断态」超时（10000ms）`。

本轮把失败快照改成「抛错当时取」后（§4），该次失败的页面快照直接给出了根因：

```
<section data-route="dirs"><p data-content-origin="live">…</p>
<ul data-directory-list="directories">…</ul>…   ← 连接是 online，目录列表已同步
<button data-connection-state="online" …>在线</button>
```

即：**被「改坏」的 `hostProof` 验签通过了**，连接照常走到 `online`。

### 3.2 根因

`e2e/page/fake-host.ts` 的 `flipSignature` 原本只翻**最后一个字符**：

```ts
return `${signature.slice(0, -1)}${signature.slice(-1) === "A" ? "B" : "A"}`;
```

64 字节签名编码成 **86 个** base64url 字符：86 × 6 = 516 位，而数据只有 512 位，
**末字符只有高 2 位有效**，低 4 位是补零。`A`（索引 0）与 `B`（索引 1）的高 2 位都是 `00`，
而编码器补零后末字符恰好是 `A`（末 2 位为 00）的概率是 **1/4**——
这时翻转后的字符串解码回来与原签名**逐字节相同**；客户端的
`decodeBase64Url`（`src/sync-client/base64url.ts:60`，只取前 8 位、丢弃补位）拿到同一条合法签名，
验签自然通过。

**独立复现（按该解码算法在 4000 条随机 64 字节签名上统计）**：

```
旧 flip 的 no-op 比例（4000 次随机签名）: 0.24325
```

与 1/4 的理论值一致，也与观测到的 4/10 次偶发红同量级。

### 3.3 修法

改为「解码 → 翻转首字节的最高位 → 重新编码」：

```ts
function flipSignature(signature: string): string {
  const bytes = decodeBase64UrlLoose(signature);
  bytes[0] = (bytes[0] ?? 0) ^ 0x80;
  return encodeBase64Url(bytes);
}
```

首字节参与数据位，必然改变签名；且置起 r 的最高位后 r ≥ 2^255 > 曲线阶 n，
ECDSA 要求 1 ≤ r < n，**验签不可能通过**（确定性失效，不再是概率事件）。
修后 CR-3 在最终代码状态下 5/5 绿。

> 顺带登记（**不在本轮写入范围**）：客户端 `decodeBase64Url` 接受非规范补位编码
> （末字符低 4 位非零也照收），这是产品侧的一处宽容解码。它不改变字节含义，
> 但正是它让「末字符翻转」这种非规范串得以通过。若将来要求 wire 编码唯一，
> 应在 `src/sync-client/base64url.ts` 侧收紧，不属 TP4。

### 3.4 关于 reviewer 建议的 `waitFor` 稳定化：本轮不需要

reviewer 的 TP4-6 建议「若是 `waitFor` 时序类问题，考虑给 `waitFor` 增补『条件成立后再等一帧』」。
查清后**不需要**：CR-1/CR-2/CR-3 的偶发红各有确切原因（等待选择器被占位命中 / 抢跑重叠 /
夹具的伪造签名有 1/4 概率无效），没有一条是「条件成立得比 DOM 更新早」——
`waitFor` 本身（25ms 宏任务轮询）没有缺陷，加「再等一帧」只会掩盖真因、把红门禁变成更慢的
flaky 门禁。因此本轮**未改** `waitFor`。

### 3.5 本轮的运行次数（供证据强度核对）

本轮共跑完整套件 **16 次**：基线（未改代码）4 次、中间态 4 次、最终代码状态 5 次、
变异态 3 次（M1/M3/M2）。最终状态按 reviewer 要求**不带输出过滤连跑 5 次**，逐次红项清单见 §1.3 与日志。

---

## 4. 诊断性修复：失败明细此前看不到任何东西

`runChecks` 原本在**捕获时**才取 `document.body.innerHTML.slice(0, 600)`：

1. 每条用例都在 `finally` 里卸载容器，等错误冒泡上来页面已被拆空，快照只剩
   `<div id="root"></div>`；
2. body 开头是占位 `<div id="root">` 与那段 bootstrap `<script>`，600 字符的窗口被它们吃光，
   真正被挂载的组件树一个字都看不到。

结果就是本轮之前所有偶发红的明细都是
`…｜页面：<div id="root"></div>`——**无法定位**（reviewer 的 TP4-6 也因此没能查清第三条红）。

改法：

- `CheckFailure` 在**构造时**固定页面快照（那时容器还在），`runChecks` 优先用它；
- 快照先摘掉 `<script>` 节点再截 600 字符，让片段落在组件树上；
- 快照附带 `window.__pageErrors`（页面级未捕获异常 / 未处理拒绝）。页面错误不会让用例
  直接变红，只会让它**超时**，此前这个数组从没进过输出。

效果即 §3.1 的页面快照——一次就定位了 CR-3 的根因。

---

## 5. 变异验证（本轮独立复现，撤销 r1 的「判别力缺口」定性）

方法：临时改坏实现 → 跑全套 → 记录红项 → `git checkout --` 还原 → 确认
`git status --porcelain` 只剩本轮 5 个 e2e 文件。

| # | 变异 | tsc | 结果 | 判定 |
| --- | --- | --- | --- | --- |
| M1 | **只**改 `src/domain/directory.ts`（`DirectoryView` 多带一个 `path` 并透传），渲染层不动 | 0 诊断 | 20/21，唯一红 R15-6 | **无效变异**（未触达呈现面） |
| M3 | M1 + `directory-model.ts` 的 `toRow` 带出 `path` + `DirectoryList.tsx` 里加一个合法文本节点 `<span data-directory-path-label>` | 0 诊断 | **17/21**：R15-1、R15-2、R15-3 全红 + R15-6 | **有效变异，红法干净** |
| M2 | `ConversationStream.tsx` 把非 `unsupported` 的原因兜底成 `"client"`，`noDedicatedView` 恒为 `true`（复现 WP7 第 1 轮 MAJOR-2） | 0 诊断 | **17/21**：R18-3、R18-4、R18-5 全红 + R15-6 | **有效变异** |

M3 的失败信息（即「红法」）：

```
FAIL  R15-1 …… 目录页渲染结果泄露了本机路径：文本里出现了 "D:\\"
FAIL  R15-2 …… 同名消歧用了路径片段：文本里出现了 "D:\\"
FAIL  R15-3 …… 断连后的渲染结果泄露了本机路径：文本里出现了 "D:\\"
```

**是「检测到泄露」，不是「页面崩溃」或「等待超时」**——页面结构完好（tsc 零诊断、正常渲染），
断言在三条独立用例、三条不同代码路径上都抓住了它。

### 结论（撤销 r1 §5 的定性）

- r1 把「M1/M3 未变红」记为「本轮遗留的判别力缺口」**与证据不符，已撤销**（r1 已就地加更正说明）。
  正确表述是：**单层变异未触达被测面**——M1 只改 domain（字段停在 `DirectoryView`，到不了
  `DirectoryRowModel`），单改组件则没有字段可渲染；只有跨层的接缝才有可观测的泄露面。
- **R15 的负向断言判别力充分，不需要补测试。**
- M1 改记为**无效变异**：按 spec（`specs/pwa-web-client/spec.md:106`），MUST NOT 约束的是
  **显示、编辑或推导**，一个没有被渲染的字段不违反任何 MUST。
- M2 的数字修正为 **17/21**（reviewer 实测 16/21 里多出的那一条是当时 CR-1 的 flaky 红；
  修掉不稳定后数字是确定性的 17/21 = 21 − 3（R18-3/4/5） − 1（R15-6））。

---

## 6. 门禁与退出码语义（reviewer 的 TP4-4）

### 6.1 本轮门禁实测

| 命令 | 结果 |
| --- | --- |
| `npx tsc --noEmit`（clients/app 主 tsconfig） | 0 诊断 |
| `npx tsc --project e2e/tsconfig.json --noEmit` | 0 诊断 |
| `npx vitest run` | 28 文件 / 359 用例全绿 |
| `node scripts/run-browser-check.mjs`（WP5b，未改动） | 16/16 |
| `node e2e/run-e2e.mjs` | **连跑 5 次：20/21、20/21、20/21、20/21、20/21，退出码均为 1** |
| `npm run check`（仓库根） | 前 9 道全绿；`check:agentic` 因环境前置缺失失败（exit=1，见 §7） |

### 6.2 退出码语义（**必须显式写明，不要再把它当成「期望值」**）

`e2e/run-e2e.mjs:301-308` 的逻辑是 `failed.length > 0` 即 `process.exitCode = 1`。
因此：

- **退出码 1 只表示「存在失败检查」，不区分「已知红」与「真回归」**；
- 当前唯一已知红是 **R15-6**（产品缺陷，见 §8），所以 1 是**现状**，不是**期望**；
- **R15-6 修好后 e2e 会变成 21/21、退出码 0。那 0 是「修复成功」，不是「门禁失败」**；
  反之若将来有人为了让 e2e 变绿而删掉 R15-6 用例，退出码同样会变 0，**退出码本身看不出来**。

**维护含义（给门禁的使用者）**：这条命令**不能用退出码单独判定**，必须看逐条清单。
判断口径应为：

- 期望红集合 = `{R15-6}`（在 `directory-model.ts:189` 修复前）；
- **真回归 = 红集合里出现期望红之外的任何一条**；
- 修复落地后期望红集合变为空集，届时全绿（退出码 0）才是正确状态。

本轮**没有**改退出码逻辑（未在实现里加特例），因为要让退出码区分二者，就得在 runner 里
维护一份「已知红清单」——那会把已知缺陷固化进门禁，且清单一旦忘记删除就会**掩盖**该缺陷的修复；
用「红集合 = 期望红」这种可读、可审查的口径更不容易被绕过。若将来确实需要机器判定，
建议的做法是让用例自带「已知缺陷」标记并在报告里单独成节，而不是把豁免写进退出码。

---

## 7. 未验证项（如实登记）

1. **`deps` / `advisories` / `secrets` 三个 CI-only job 未执行、未声称通过**（本地无法复现其环境）。
2. `npm run check` 的 `check:agentic` 在 worktree 里必然失败：它需要**仓库根**的
   `node_modules/@dongfanglin/openspec-agentic`，而 worktree 不共享 `node_modules`，
   且本工作包被明确禁止在仓库根跑 `npm ci` / `npm install`。这是环境前置缺失，不是本轮引入的回归
   （本轮只改 `clients/app/e2e/` 下的测试文件，未触及任何配置或实现）。
3. **真实配对流程、真实 `wss://` 传输、真实 expo-router 导航栈、`session.read` 分页、
   跨连接恢复游标持久化**：与 r1 §7 相同，仍未覆盖（预置「已配对」起点的取舍未变）。
4. **StrictMode 下的 `dispose()` 不可复活**：仍未验证（当前配置未启用 StrictMode）。
5. CR-2 基线偶发超时的机制（§1.3）是 [INFERENCE]：修后 5 次全绿，但未做单变量对照实验。
6. **R15-6 仍是红**：那是 `src/features/directory-model.ts` 的产品缺陷（R15 MUST 违反），
   按分工由另一路修复；本轮**未改动实现**，也未为了让 e2e 变绿而删改该用例。

---

## 8. 与 R15-6 已知红的关系（结论不变）

R15-6 的红是**被测实现的真实缺陷**（目录存在但无会话时，详情页把「还没有会话」说成
「链接已失效」），由正确的用例稳定报出（本轮 5/5 次红），且 `e2e/` 不在 `vitest` 主套件的
收集范围内（`vitest.config.ts` 只收 `src/**/*.test.ts`），不污染其它门禁。
**不构成合入阻塞**；修复落地后它应转为绿、退出码转为 0（见 §6.2）。

---

*本报告的所有变异实验均已还原，`git status --porcelain` 为空；
`git diff --name-only f6ea252..HEAD` 的 12 条路径全部在 `clients/app/e2e/` 下。*

---

## test_delivery / agentic-handoff（门禁凭据块）

```agentic-handoff
version: 1
agent_context:
  agent_id: "testing-4-r2"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "4.13"
    work_package: TP4
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "0a19d39"
    evidence_type: DELIVERY
    evidence_id: tp4-delivery-r2
    report_path: "reports/deliver-tp4-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "固定交付提交 0a19d39（完整 40 位 0a19d39b4c63b723c7e7d7edbaaf64e60cd82e83，与 verification.md Handoff Index 该行 Target Revision 单元格同值；分支 feat/tp4，基线 f6ea252）承载本轮浏览器级测试编写交付：12 个路径全部在 clients/app/e2e/ 下（run-e2e.mjs、tsconfig.json、page/{index,support,fake-host,router-stub,fixtures}.tsx、page/react-dom-client.d.ts、page/checks/{composition-root,r15-directory,r16-create,r18-degradation}.ts），本轮改 5 个文件、实现目录零改动、越界 0 条。"
    source_evidence: NOT_APPLICABLE
  - task_id: "4.13"
    work_package: TP4
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "0a19d39"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/deliver-tp4-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "PV3 的 TP4 部分在 target_revision 0a19d39 上取得零退出码：npx tsc --noEmit 与 npx tsc --project e2e/tsconfig.json --noEmit 均 0 诊断、npx vitest run 28 文件 / 359 用例全绿、node scripts/run-browser-check.mjs 16/16，均 exit 0。浏览器入口 node e2e/run-e2e.mjs 连跑 5 次均为 20/21、退出码 1，唯一红为已知产品缺陷 R15-6（其实现修复已随 MU3d 合入），退出码语义与维护口径见本报告 §6.2。日志 reports/PV3-tp4-r2.log。"
    source_evidence: NOT_APPLICABLE
checks:
  - id: PV3
    work_package: TP4
    command: "npx tsc --noEmit && npx tsc --project e2e/tsconfig.json --noEmit && npx vitest run && node scripts/run-browser-check.mjs（cwd=clients/app，本报告 §6.1 的实测行）"
    scope: "工作包 Project Verify（前端浏览器验证，TP4 部分）：两条 tsc（0 诊断）、vitest 28 文件 / 359 用例、既有浏览器断言 16/16，均 exit 0；浏览器入口 node e2e/run-e2e.mjs 连跑 5 次为 20/21、退出码 1（唯一红为已知产品缺陷 R15-6，语义见本报告 §6.2），故不计入本行的零退出码"
    environment: "Windows x64；Node v22.22.0；真实 Chromium 内核（headless=new，127.0.0.1 安全上下文）；clients/app 独立 node_modules"
    exit_code: 0
    log_path: reports/PV3-tp4-r2.log
    result: PASS
test_delivery:
  TP4:
    kind: automated
    artifacts:
      - reports/tp4/clients/app/e2e/run-e2e.mjs
    basic_checks:
      - PV3
```

<!-- WP5b 修复轮次（Attempt 2）交付报告。coder 只交付修复、判别力与检查证据；不判独立 review、合入与最终验收。 -->

# WP5b 修复轮次交付（deliver-wp5b-r2）

## Shared Report

- **task_id**: 2.6（`phase: fix`，DELIVERY）/ 3.11（CHECK：PV1 / PV3）
- **work_package**: WP5b（交付单元 MU3b）
- **role**: coder（`coder-w5b-r2`，Attempt 2）
- **phase**: fix（只修 `review-wp5b-r1` 的 F1–F7；不重写已通过检视的 R11 非导出实现、transcript 字节级编码结果、R20 的 8 MiB/30 天/LRU 策略）
- **agent_context**: `agent_id: coder-w5b-r2`，`isolation: fork_turns=none`（新实现实例接管 `.worktrees/wp5b`；未继承 `coder-w5b-r1` 的任何对话，只带已检视交付 `13e6dbb`、`review-wp5b-r1` 报告与派发说明）
- **起点（base）**: `13e6dbbedbae4e32196c994391f73acc95ba323f`（已检视交付，分支 `feat/wp5b`）
- **交付（target）**: `263d3ba8b48ae5dae8ab87b16bc4c0ea86f62314`（分支 `feat/wp5b`）
- **scope**: 只在 `.worktrees/wp5b` 内改 `clients/app/src/platform/**` 14 文件并新增 `clients/app/scripts/run-browser-check.mjs`；未触碰 `schemas/**`、`fixtures/**`、`crates/**`、`docs/**`、根 `package.json`、`clients/app/package.json`、`src/layers.test.ts`、其他 worktree、主检出、`openspec/changes/**`（登记由 main 做）
- **result**: **PASS**（PV1 / PV3 在目标提交上 exit 0）

### 交付提交

| 项 | 取值 |
| --- | --- |
| worktree | `D:\Project\acp-remote\.worktrees\wp5b` |
| 分支 | `feat/wp5b` |
| 起点（base） | `13e6dbbedbae4e32196c994391f73acc95ba323f` |
| 交付（target） | `263d3ba8b48ae5dae8ab87b16bc4c0ea86f62314` |
| 提交信息 | `fix(frontend): WP5b 第二轮——域长度按 UTF-8 字节判定、缓存写路径串行化、ensureIdentity single-flight、R20 落盘守门接线` |

`git diff --stat 13e6dbb..263d3ba`（15 files changed, **648 insertions(+), 75 deletions(-)**）：

```
 clients/app/scripts/run-browser-check.mjs          | 243 +++++++++++++++++++++
 clients/app/src/platform/index.ts                  |   2 +-
 clients/app/src/platform/lifecycle.test.ts         |  24 +-
 clients/app/src/platform/lifecycle.ts              |   2 +-
 clients/app/src/platform/lifecycle.web.ts          |  11 +-
 clients/app/src/platform/local-cache.test.ts       |  54 +++++
 clients/app/src/platform/local-cache.ts            |  30 ++-
 clients/app/src/platform/local-cache.web.ts        | 119 ++++++----
 clients/app/src/platform/secure-storage.test.ts    |  38 +++-
 clients/app/src/platform/secure-storage.ts         |   3 -
 clients/app/src/platform/secure-storage.web.ts     |  47 +++-
 clients/app/src/platform/testing/browser-checks.ts | 118 +++++++++-
 clients/app/src/platform/testing/fake-indexeddb.ts |   4 +-
 clients/app/src/platform/transcript.test.ts        |  14 ++
 clients/app/src/platform/transcript.ts             |  14 +-
```

`git diff --name-only 13e6dbb..263d3ba | grep -v '^clients/app/'` 为空：写入范围无越界。

---

## 1. F1（MINOR）—— 域长度按 UTF-8 字节判定，超限抛错而非静默截断

### 根因（成立）

`encodeTranscript` 的守卫用 `domain.length`（UTF-16 码元数）判 u16 上限，`setUint16` 写的却是
`domainBytes.length`（UTF-8 字节数）。`"\u4e2d".repeat(30000)` 仅 30000 码元（守卫放行）
却有 90000 UTF-8 字节，前缀被按模 65536 写成 24464，解码器无法还原 domain。

### 修法

先编码再判定，守卫与写入**同一基准**（UTF-8 字节数），越界抛 `TranscriptError`：

```ts
const domainBytes = new TextEncoder().encode(domain);
if (domain.length === 0 || domain.includes("\u0000") || domainBytes.length > 0xffff) {
  throw new TranscriptError("invalid_domain", "domain 为空、含 NUL 或 UTF-8 字节数超过 u16 上限");
}
```

与 Rust 侧 `crates/acpr-transcript/src/lib.rs:77` 的 `domain.len()`（字节长度）同义。
**未改动任何已对齐的编码结果**：magic/版本/前缀/字段布局逐字节不变（6 个冻结向量仍逐字节一致，见 §6 PV3 与浏览器 `AC3` 断言）。仅新增一条以多字节字符构造超长域的用例，断言抛 `TranscriptError`，并补一条 65535 字节边界用例断言前缀 == `0xffff`。

---

## 2. F2（MINOR，已复现）—— 并发缓存写入丢条目

### 根因（成立）

`WebLocalCache.#commit` 与 `get` 的过期/命中回写都是**无保护的全量读-改-写**
（`#readAll` → `evictToFit` → `#replaceAll` 先清空再重写）。两次重叠操作在读与写之间交错，
后写者以旧快照覆盖先写者已落盘的条目。

### 修法（串行化，不改测试时序）

在端口实例内加一条 **promise 链**，把所有会改持久状态的操作接入同一个临界区：

```ts
#writeChain: Promise<unknown> = Promise.resolve();

#serialize<T>(operation: () => Promise<T>): Promise<T> {
  const run = this.#writeChain.then(operation, operation);
  this.#writeChain = run.then(() => undefined, () => undefined);
  return run;
}
```

`putLocalSummary` / `putImportedMetadata` / `clear` 整体、`get`（含其回写）都经 `#serialize` 排队；
只读的 `stats` 不排队（不产生写）。前序操作失败不阻断后续。**未**删除或弱化任何竞态断言，
**未**把并发的 `Promise.all` 改成顺序调用。

---

## 3. F3（MINOR，已复现）—— 并发 `ensureIdentity` 生成双密钥

### 根因（成立）

`ensureIdentity` 是 check-then-act：`await loadIdentity()` 返回 null 才生成。两个并发调用都读到
null，各生成并写入一把密钥；先返回者的句柄与落盘身份不一致，随后会用主机不认识的密钥签名。

### 修法（single-flight）

新增 `#pendingEnsure: Promise<DeviceIdentity> | null`。`ensureIdentity` 先校验来源形状，再复用
in-flight promise；无 in-flight 时创建 `#ensureIdentityOnce(...)` 并把「检查 + 生成 + 写入」
放在其中。并发的不同来源请求在 `#pendingEnsure` 分支中同样抛 `origin_mismatch`（与串行路径同口径）。
完成后 `finally` 清空 `#pendingEnsure`，后续调用仍走完整检查路径。**未**改动 R11 的非导出生成、
`#validateRecord` 重新断言与失败关闭。

---

## 4. F4（MINOR）—— R20 的持久化闸门没有接线

### 根因（成立）

`assertPersistableForOrigin` 自述是「任何持久化 imported 内容的路径都必须经过的唯一闸门」，
但除 `index.ts` 再导出与单元测试外无任何调用者；R20 的运行期守卫实际不存在。

### 修法（选 (a)：真的接线，而不是删护栏）

在两个持久写入口调用闸门，并转发**整个入参**（而非只转发得到类型许可的字段），使「更宽对象经变量流入」在运行期被真的检查：

```ts
// putLocalSummary
assertPersistableForOrigin({ kind: "local" }, input);
// putImportedMetadata
assertPersistableForOrigin({ kind: "imported" }, input);
```

同时把守门本身加强为**运行期允许清单**：`imported` 分支除拒绝 `summary` 外，还拒绝
`metadata` 里出现 `ImportedMetadata` 允许清单之外的任何键（`ownerNodeId`/`exportId`/`sessionId`/
`originCursor`/`localSequence`/`contentDigestSha256`/`acks`）。这正是 F4 所述的
`{ ...metadata, body }` 形状——TypeScript 结构化类型不会拦它，只有运行期断言能拦。

---

## 5. 其余三项的处置

- **F5（注释指向不存在的产物）** —— 修正为指向真实存在的东西，并补上缺失的 runner：
  - 新增 `clients/app/scripts/run-browser-check.mjs`：零第三方依赖（只用 Node 内建
    `http`/`fs`/`os`/`child_process`/`fetch`/`WebSocket` 与 app 自带 `esbuild`），把
    `testing/browser-checks.ts` 打成 IIFE、静态服务、以 `--headless=new` 启动本机 Chromium
    内核（缺省发现 Edge/Chrome，可用 `BROWSER_PATH` 覆盖）、经 CDP 求值取回断言结果，
    退出码即门禁、结果 JSON 写到 `$TMPDIR/wp5b-browser/browser-evidence.json`。
  - `testing/fake-indexeddb.ts`、`secure-storage.test.ts` 的注释改指这个真实 runner。
  - **未**创建 `secure-storage.browser.test.ts`：vitest 的 `include: ["src/**/*.test.ts"]` +
    `environment: "node"` 决定了 `*.browser.test.ts` 若被收集就是**假浏览器证据**；真实浏览器
    面改为显式 runner，避免误导。
- **F6（可能的 bfcache 回归）** —— 处置：去掉 `beforeunload` 监听器，只保留 `pagehide`
  （正常卸载与 bfcache 两侧都可靠）。新增用例断言实现**不注册** `beforeunload`、仍注册 `pagehide`。
  同步删除 `ListenerBook.beforeunload`、`pagehide` 事件文案与文件头注释里的旧表述。
- **F7（未使用的导出记录键）** —— 处置：删除 `DEVICE_IDENTITY_ORIGIN_KEY`（常量、`index.ts`
  再导出、`#deleteRecords` 里对它的一行空转删除、导入项），使「声明了却没接线」不再存在。
  来源绑定仍由同一条记录的 `canonicalOrigin` 承担（删除该记录即删除绑定），行为不变。

---

## 6. 判别力证据（修复前失败 → 修复后通过）

### 6.1 修复前失败（反事实运行）

用 `git stash push` **只**暂存 8 个实现文件（保留全部新测试），在修复前的实现上运行同一测试套件，
再 `git stash pop` 复原。原始日志：`reports/PV3-wp5b-r2-counterfactual.log`。

```
Test Files  4 failed | 3 passed (7)
     Tests  8 failed | 76 passed (84)
```

8 条新用例**全部**在修复前失败：

| 用例 | 对抗的缺陷 |
| --- | --- |
| `UTF-8 字节数超过 u16 上限的 domain 被拒绝（不以码元数判定、不静默截断）` | F1 |
| `并发写入不丢条目：两条并发 put 都落盘（F2）` | F2 |
| `clear 与并发 put 竞争时 clear 胜出，存储为空（F2）` | F2 |
| `并发 ensureIdentity 收敛到同一把密钥，且与落盘记录一致（F3）` | F3 |
| `并发的不同来源请求不会静默共享同一身份（F3）` | F3 |
| `落盘守门被真的接线：imported 元数据携带正文即拒绝且未落盘（F4）` | F4 |
| `落盘守门被真的接线：本节点条目携带 imported 元数据形状即拒绝（F4）` | F4 |
| `pagehide 产生 pagehide 事件；不注册 beforeunload（bfcache 失格诱因）` | F6 |

### 6.2 修复后通过

- Node（`reports/PV3-wp5b-r2.log`）：**7 test file、84 passed / 0 failed**（起点为 77）。
- 真实 Chromium（`reports/PV3-wp5b-r2-browser.log`）：**16/16 passed**。

### 6.3 并发用例是可复现的确定性用例（不依赖真实浏览器时序）

F2/F3 在 Node 侧用本包 `testing/fake-indexeddb.ts` 的替身复现：它的 `FakeTransaction` 与
`openCursor` 都以 `queueMicrotask` 异步完成，`#commit` 的读-改-写跨多个微任务，因此
`Promise.all([...])` 的两个操作必然在读与写之间交错——这正是 review 复现该缺陷的同一手法。
修复后二者被 `#serialize` 串成临界区而复现不出去。对应浏览器侧子检查同样断言
`local=1 imported=1` 与 `clear` 后 `local=0 imported=0`。

### 6.4 本轮 Chromium 子检查清单（16 项）

保留第 1 轮全部 11 项真实浏览器子检（`exportKey(pkcs8/jwk)` 抛 `InvalidAccessError`、
真实 IndexedDB 往返后 `extractable=false`、持久化记录无 JWK `d`/pkcs8、transcript 与
device-proof 向量逐字节一致、签名 64 字节 P1363 可验证、imported 正文不入持久化等），
新增 5 项：`F1`（多字节超长域抛错）、`F2`×2（并发两个 put 都落盘 / `clear` 胜出）、
`F3`（并发 `ensureIdentity` 同一把密钥且与落盘一致）、`F4`（进口正文夹带被守门拒绝且未落盘）。

---

## 7. Checks（实际命令 / 目录 / 环境 / 退出码 / 子检查 / 日志）

| ID | 命令 | 工作目录 | 环境 | 退出码 | 日志 |
| --- | --- | --- | --- | --- | --- |
| **PV1** | `npm run check` | `.worktrees/wp5b` | 仓库根 `node_modules` 以 junction 消费；Node v22.22.0 / npm 10.9.4 | **0** | `reports/PV1-wp5b-r2.log` |
| **PV3** | `npx tsc --project tsconfig.json --noEmit && npx vitest run --config vitest.config.ts && npx expo export --platform web` | `.worktrees/wp5b/clients/app` | 独立 `clients/app/node_modules/`；Node v22.22.0 / npm 10.9.4；typescript 5.9.3 / vitest 3.2.7 / expo 54 | **0** | `reports/PV3-wp5b-r2.log` |

### PV1 子检查逐条（十道根门禁）

- `check:schemas` → `schema fixtures OK: 149 valid, 51 invalid (ajv Draft 2020-12), 41 event views bound`
- `check:commands` → `command catalog OK: 13 commands`
- `check:errors` → `error registry OK: 58 codes across 2 protocols`
- `check:features` → `feature registry OK: 13 feature ids across 2 protocols`
- `check:assets` → `contract assets OK: 17 schemas, 213 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed`
- `check:acp` → `ACP compatibility matrix OK: 25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows, schema validated by ajv)`
- `check:docs` → `doc links OK: 415 relative links, 9058 section refs across 518 markdown files`
- `check:boundaries` → `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致`
- `check:drift` → `contract drift OK: §7 的 36 条 DDL …；§5 的 15 个 trait / 96 个方法签名 …`
- `check:agentic` → `Totals: 21 passed, 0 failed (21 items)`

### PV3 子检查逐条

- `npx tsc --project tsconfig.json --noEmit` → **exit 0**（无诊断）
- `npx vitest run --config vitest.config.ts` → **exit 0**；7 test file、**84 passed / 0 failed**
  （`transcript` 8、`secure-storage` 9、`local-cache` 13、`lifecycle` 4、`layers` 9、`contract` 22、`domain` 19）
- `npx expo export --platform web` → **exit 0**；Metro 打包 642 modules，产出 `dist/index.html`

### 浏览器子检查（PV3 的决定性证据）

- `node scripts/run-browser-check.mjs`（cwd = `clients/app`）→ **exit 0**，**16/16 passed**；
  日志 `reports/PV3-wp5b-r2-browser.log`，原始结果 JSON `$TMPDIR/wp5b-browser/browser-evidence.json`。
- 该 runner 本轮才入库，使第 1 轮「注释指向不存在产物」的可复现性缺陷（F5）不再存在。

---

## 8. 未验证内容

- **未执行** E2E 与页面级浏览器用例（TP3/TP4 范围）；`expo export` 只证明构建可通过，
  **不证明**真实浏览器中的渲染、路由与运行期行为。
- **未验证** 与 Rust `acpr-transcript` 的**跨语言**对拍本身（TP3/MU3e 范围）；本包只自证与
  6 个冻结向量逐字节一致（Node + Chromium 两处）。
- **未验证** Sync Client/状态机（WP6）、页面与 feature（WP7）、配对 HTTP 端到端、imported 离线分流。
- **未判** 独立 review、合入、最终验收；`deps`/`advisories`/`secrets` 三个 CI-only job 本地无等价物，**未运行**。
- 并发证据在 Node 侧用 `testing/fake-indexeddb.ts` 替身（其事务与游标均为微任务异步，足以复现
  F2/F3 的交错），并在**真实 IndexedDB** 的 Chromium 子检查里得到同样结论；但替身**不**声称
  结构化克隆保真——该性质仍只由浏览器子检查覆盖。
- 浏览器子检查覆盖 Chrome 系（本机为 Edge）；**未**在 iOS Safari / Android Chrome 复跑。
- **未**改动 `src/layers.test.ts`；X1/X2（平台白名单缺口、WP6/WP7 组合根取用限制）属上游/计划层，未处置。

---

```agentic-handoff
version: 1
agent_context:
  agent_id: "coder-w5b-r2"
  isolation: "fork_turns=none（新实现实例接管 .worktrees/wp5b 的 feat/wp5b；未继承 coder-w5b-r1 的任何对话，只携带已检视交付 13e6dbb、review-wp5b-r1 报告与 Main 的派发说明）"
handoff_index:
  - task_id: "2.6"
    work_package: WP5b
    role: coder
    phase: fix
    round: 2
    stage: work-package
    attempt: 2
    target_revision: "263d3ba8b48ae5dae8ab87b16bc4c0ea86f62314"
    evidence_type: DELIVERY
    evidence_id: deliver-wp5b-r2
    report_path: "reports/deliver-wp5b-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 .worktrees/wp5b（feat/wp5b，base=13e6dbb）定点修复 review-wp5b-r1 的 F1–F7，固定为提交 263d3ba（15 files changed, +648/−75）。F1：encodeTranscript 先编码再以 domainBytes.length（UTF-8 字节）判 u16 上限，越界抛 TranscriptError，与 Rust lib.rs:77 的 domain.len() 同基准；已对齐的字节结果未改（6 个冻结向量仍逐字节一致）。F2：WebLocalCache 增加 #writeChain/#serialize promise 链，把 putLocalSummary/putImportedMetadata/clear 与 get 的回写串成临界区，并发 put 不再互相覆盖、clear 与并发 put 竞争时 clear 胜出。F3：ensureIdentity 增加 #pendingEnsure single-flight，并发调用收敛到同一把密钥且与落盘记录一致，并发不同来源仍抛 origin_mismatch。F4：putLocalSummary/putImportedMetadata 真的调用 assertPersistableForOrigin 并转发整个入参，守门加强为运行期允许清单（拒绝 metadata 里的额外字段，如 { ...metadata, body }）。F5：新增 clients/app/scripts/run-browser-check.mjs（零第三方依赖，esbuild 打包 + 静态服务 + Chromium --headless=new + CDP），并使两处注释改指真实产物；未创建 *.browser.test.ts（会与 vitest node 环境冲突成假证据）。F6：lifecycle.web.ts 去掉 beforeunload 监听器只留 pagehide（bfcache），新增断言不注册 beforeunload。F7：删除未使用的 DEVICE_IDENTITY_ORIGIN_KEY 及其再导出/空转删除。判别力：仅暂存实现文件的反事实运行显示 8 条新用例全部失败（4 file / 8 failed / 76 passed），修复后 7 file / 84 passed；新增用例不删竞态断言、不改为顺序调用。"
    source_evidence: "openspec/changes/sync-scope-and-pwa-client/reports/review-wp5b-r1.md"
  - task_id: "2.6"
    work_package: WP5b
    role: coder
    phase: fix
    round: 2
    stage: work-package
    attempt: 2
    target_revision: "263d3ba8b48ae5dae8ab87b16bc4c0ea86f62314"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/deliver-wp5b-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "npm run check（cwd=.worktrees/wp5b，仓库根 node_modules 以 junction 消费，Node v22.22.0 / npm 10.9.4）在 263d3ba 上 exit 0：十道合同门禁全绿（schemas 149 valid/51 invalid/41 views；commands 13；errors 58；features 13；assets 17 schemas/213 fixtures/12 transcript vectors/20 negative/2 SAS；acp 25 methods/11 updates/5 content blocks/3 tool content types/19 capabilities/8 invariants/10 test families；docs 415 links/9058 refs/518 files；boundaries 12 crates；drift 36 DDL + 15 traits/96 methods；agentic 21 passed 0 failed）。worktree 根缺 node_modules 时 check:agentic 会以缺 openspec-agentic bin 失败；已按 wp5a 既有形态补 junction，非代码问题。"
    source_evidence: reports/PV1-wp5b-r2.log
  - task_id: "3.11"
    work_package: WP5b
    role: coder
    phase: fix
    round: 2
    stage: work-package
    attempt: 2
    target_revision: "263d3ba8b48ae5dae8ab87b16bc4c0ea86f62314"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/deliver-wp5b-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 263d3ba 上的 clients/app 自身入口（cwd=.worktrees/wp5b/clients/app，独立 clients/app/node_modules，Node v22.22.0，typescript 5.9.3 / vitest 3.2.7 / expo 54）三段均 exit 0：npx tsc --project tsconfig.json --noEmit 无诊断；npx vitest run --config vitest.config.ts 为 7 test file、84 passed / 0 failed（起点 77，新增 transcript 1 / secure-storage 2 / local-cache 4 / lifecycle 1 共 8 条，其中 8/8 在仅暂存实现的反事实运行中失败）；npx expo export --platform web 由 Metro 打包 642 modules 并产出 dist/index.html。另以 node scripts/run-browser-check.mjs 在真实 Chromium 内核跑 16/16 passed（含保留的第 1 轮 11 项与新增 F1/F2×2/F3/F4 共 5 项），退出码 0。未接入根 npm run check（plan 裁决）。"
    source_evidence: reports/PV3-wp5b-r2.log
checks:
  - id: PV1
    work_package: WP5b
    command: "npm run check (cwd=.worktrees/wp5b)"
    scope: "合同门禁十道（schemas/commands/errors/features/assets/acp/docs/boundaries/drift/agentic）"
    environment: "cwd=.worktrees/wp5b；仓库根 node_modules 以 junction 消费；Node v22.22.0 / npm 10.9.4"
    exit_code: 0
    log_path: reports/PV1-wp5b-r2.log
    result: PASS
  - id: PV3
    work_package: WP5b
    command: "npx tsc --project tsconfig.json --noEmit && npx vitest run --config vitest.config.ts && npx expo export --platform web (cwd=.worktrees/wp5b/clients/app)；另有 node scripts/run-browser-check.mjs"
    scope: "前端自身入口：类型检查 + 单元/契约测试 + Web 构建（plan 裁决不接入根 npm run check）；另有真实 Chromium 的 16 项 R11/R20/F1–F4 子检查"
    environment: "cwd=.worktrees/wp5b/clients/app；独立 clients/app/node_modules；Node v22.22.0；typescript 5.9.3 / vitest 3.2.7 / expo 54；浏览器子检查用本机 Chromium 内核（Edge）+ 真实 IndexedDB/WebCrypto"
    exit_code: 0
    log_path: reports/PV3-wp5b-r2.log
    result: PASS
test_delivery:
  WP5b:
    kind: automated
    artifacts:
      - reports/PV3-wp5b-r2.log
      - reports/PV3-wp5b-r2-browser.log
      - reports/PV3-wp5b-r2-counterfactual.log
      - clients/app/src/platform/transcript.test.ts
      - clients/app/src/platform/local-cache.test.ts
      - clients/app/src/platform/secure-storage.test.ts
      - clients/app/src/platform/lifecycle.test.ts
      - clients/app/src/platform/testing/browser-checks.ts
      - clients/app/scripts/run-browser-check.mjs
    basic_checks:
      - PV1
      - PV3
```

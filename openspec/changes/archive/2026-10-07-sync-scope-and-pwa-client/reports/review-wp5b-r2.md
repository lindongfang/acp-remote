<!-- WP5b（MU3b 唯一成员）修复轮次独立复核报告（review-wp5b-r2，Round 2 / recheck）。
Reviewer 只报告，不修改代码、测试、规划文件、任务状态或 verification.md。 -->

task_id: "2.6"
role: reviewer
phase: branch
agent_context:
  agent_id: "review-wp5b-r2"
  isolation: "fork_turns=none（新建独立子 Agent，未参与 WP5b 第二轮实现，也未继承 coder-w5b-r2 的任何对话；只携带派发说明、review-wp5b-r1 报告与仓库内契约文档）"
target_revision: "263d3ba8b48ae5dae8ab87b16bc4c0ea86f62314"
base_revision: "13e6dbbedbae4e32196c994391f73acc95ba323f"
review_type: branch
round: 2
review_id: review-wp5b-r1
scope: "仅复核 13e6dbb..263d3ba 的 15 files / +648 / −75，按原问题 ID F1–F7 逐项核对，并检查是否引入回归。不重复第 1 轮已确认的 R11 不可导出、transcript 冻结向量一致、R20 imported 无持久化路径、8 MiB/30 天/LRU 生效。"
changes: "只读检视，未修改任何受版本控制的文件、测试、文档、规划文件或 verification.md；仅新增本报告。（全部独立复现均在仓库外系统临时目录 %TEMP%\\wp5b-r2-probe 内完成：用被检提交的实现打包后做黑盒执行、并抽出 13e6dbb 的旧实现做差分对照；未在 .worktrees/wp5b 内创建任何文件，git status --porcelain 为空。）"
result: PASS
evidence_paths:
  - "openspec/changes/sync-scope-and-pwa-client/reports/review-wp5b-r2.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/deliver-wp5b-r2.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/PV3-wp5b-r2-counterfactual.log"
resource_cleanup: NOT_APPLICABLE（只读检视；仓库外临时产物不在版本控制内，`git status --porcelain` 为空）

```agentic-handoff
version: 1
agent_context:
  agent_id: "review-wp5b-r2"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "2.6"
    work_package: WP5b
    role: reviewer
    phase: branch
    round: 2
    stage: work-package
    attempt: 2
    target_revision: "263d3ba8b48ae5dae8ab87b16bc4c0ea86f62314"
    evidence_type: REVIEW
    evidence_id: review-wp5b-r2
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/review-wp5b-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "新建独立子 Agent 对 263d3ba（base=13e6dbb，15 files/+648/−75，14 文件在 clients/app/src/platform/、1 文件为新增 clients/app/scripts/run-browser-check.mjs）做只读复核。以『把目标提交的实现用 esbuild 打包到仓库外临时目录后黑盒执行』的自建探针，逐项复核 F1–F7 并自造额外失败场景；另抽出 13e6dbb 的旧实现并排差分。结论：F1–F7 全部成立修复，12 个冻结向量逐字节一致（含全部 6 个 sync 向量），F2/F3 无死锁/队列泄漏、失败路径 finally 必清理，未发现 patch 引入的 CRITICAL/MAJOR；1 项 P3 新发现（runner 经未声明的传递依赖加载 esbuild）。"
    source_evidence: "openspec/changes/sync-scope-and-pwa-client/reports/review-wp5b-r1.md"
```

## Review Context

| 项 | 取值 |
| --- | --- |
| Review ID / Round | `review-wp5b-r1`（沿用原线程）/ 2（recheck） |
| Review Type / Stage | `branch` / 工作包交付前复核（MU3b 候选之前） |
| Work Package | WP5b（交付单元 MU3b 唯一成员，Order 5） |
| 检视工作区 | `D:\Project\acp-remote\.worktrees\wp5b`（只读；`git status --porcelain` 为空，HEAD = `263d3ba`） |
| Base / Target | `13e6dbbedbae4e32196c994391f73acc95ba323f` → `263d3ba8b48ae5dae8ab87b16bc4c0ea86f62314` |
| 真实 diff | `15 files changed, 648 insertions(+), 75 deletions(-)`；`git diff --name-only 13e6dbb 263d3ba \| grep -v '^clients/app/'` 为空 |
| 读取的规则 | `AGENTS.md` §5、`specs/pwa-web-client/spec.md`（R10/R11/R20）、`docs/FRONTEND_DESIGN.md` §3/§7、`docs/SYNC_PROTOCOL.md` §6.2/§6.3、`crates/acpr-transcript/src/lib.rs`、`schemas/sync/v1/pairing.schema.json`、`compatibility/transcripts/v1/transcripts.json`、`plan.md` 的 WP5b 行与 Shared File Ownership、`tasks.md` 2.6、`clients/app/{package.json,tsconfig.json,vitest.config.ts,package-lock.json}` |
| 使用的验证证据 | `reports/PV3-wp5b-r2-counterfactual.log`（只读消费）；`deliver-wp5b-r2.md`（作品，**不作为**检视证据）；仓库外临时产物 `%TEMP%\wp5b-browser\browser-evidence.json`（只读核对，mtime 12:33） |
| 限制 | 不复跑 tsc/vitest/expo export（除下文“实际检查范围”明确列出的 `run-browser-check.mjs`，该 runner 是本轮新增交付物、也是 F5 的修复对象，故亲自执行一次）；不执行跨语言对拍（TP3）；不执行 E2E（TP4） |

### 独立复现手段（本 reviewer 亲自执行，全部在仓库外临时目录）

1. **黑盒执行目标实现**：用 `clients/app/node_modules/esbuild` 把 `src/platform/transcript.ts`、`local-cache{,.web}.ts`、`secure-storage{,.web}.ts`、`testing/fake-indexeddb.ts` 打包到 `%TEMP%\wp5b-r2-probe`，以 Node 22.22.0 直接调用（不依赖 vitest）。
2. **旧实现差分对照**：用 `git show 13e6dbb:…` 抽出 F1–F4 相关的 7 个旧实现文件到同一临时目录并打包，与目标实现并排跑同一批探针。
3. **冻结向量复算**：按 `compatibility/transcripts/v1/transcripts.json` 的字段表，从 fixture `input` 重编码并比对 `transcriptBase64url` 与 `transcriptSha256Hex`（**全部 12 个**向量：sync 6 + node-link 6）。
4. **并发探针**：用本包 `installFakeIndexedDB` 构造 F2 的额外交错组合、F3 的 N=5 并发与失败路径、F4 的夹带形状。
5. **执行新 runner**：`node scripts/run-browser-check.mjs`（cwd = `clients/app`），真实 Chromium 内核（本机 Edge）。

---

## A–D 四点独立核实

### A. 判别力（counterfactual）→ **成立，且覆盖 F1–F4**

- **覆盖性核对**：`reports/PV3-wp5b-r2-counterfactual.log` 的 8 条失败逐条映射到原问题——`transcript.test.ts` 的 F1（`expected function to throw, but it didn't`）、`local-cache.test.ts` 的 F2×2（`localEntries expected 1 received 0` / `clear` 后 `expected 0 received 1`）与 F4×2（`promise resolved "undefined" instead of rejecting`）、`secure-storage.test.ts` 的 F3×2（并发公钥不等 / 不同来源并发 `expected 'fulfilled' to be 'rejected'`）、`lifecycle.test.ts` 的 F6（`expected ['pagehide','beforeunload'] to not include 'beforeunload'`）。**没有**无关断言混入；F5/F7 属文档/死代码，本就不该有运行期断言。
- **我自造的额外场景全部收敛（目标实现）**：

| 自造场景 | 目标实现（263d3ba） | 旧实现（13e6dbb，差分对照） |
| --- | --- | --- |
| F2：`put` + `get` 回写 + `put` 三路交错 | `local=2 imported=1`（全部存活） | `local=2 imported=0`（imported 丢失） |
| F2：过期 `get`（删除回写）与并发 `put` | `local=1 expired=0` | `local=0`（新 put 被覆盖） |
| F2：`clear` 先入队、`put` 后入队 | `local=1`（FIFO 确定：后入队的 put 胜出） | `local=1`（旧实现下结果恰好相同，但成因不同——旧实现是竞态） |
| F2：500 次并发 `putLocalSummary` | 队列不累积、chain 收敛、`localBytes ≤ 8 MiB` | — |
| F2：序列化区内真实失败（`openDatabase` reject）后继续 | 失败传播给该调用，**其后 `put` 与 `get` 均正常**，已落盘数据完好 | — |
| F3：N=5 并发同 origin | `distinctPubKeys=1 sameHandle=true persistedMatch=true records=1` | `distinctPubKeys=5 sameHandle=false persistedMatch=false` |
| F3：不同 origin 并发 | `fulfilled` / `rejected:origin_mismatch`，**无密钥泄漏**，落盘 origin = 首个 | `fulfilled` / `fulfilled`，**B 拿到 A origin 的密钥并写盘** |
| F4：`{...metadata, body}` | `REJECTED:content_not_allowed`，未落盘 | `ACCEPTED` |
| F1：30000 码元 / 90000 字节域 | 抛 `TranscriptError` | 静默写入前缀 `24464`（= 90000 % 65536） |
| F1：边界 65535 / 65536 字节 | 接受（前缀 `0xffff`）/ 抛错 | 同样（ASCII 侧两侧基准一致） |

**结论**：counterfactual 真覆盖 F1–F4；我自造的第三种 F2 组合、F3 失败路径、F1 边界两侧也证明修复确实收敛，而非只让既有断言变绿。

### B. F1 未破坏字节对齐 → **成立**

- 守卫改在 `transcript.ts:168-169`：先 `const domainBytes = new TextEncoder().encode(domain)`，再以 `domainBytes.length > 0xffff` 判定（与 `setUint16(5, domainBytes.length, false)` 的写入量、以及 Rust `crates/acpr-transcript/src/lib.rs:77` 的 `domain.len()` 同一基准）。
- **全部 12 个冻结向量逐字节一致**（不是 1–2 个）：sync `device-proof 303 / host-challenge 298 / pairing-proof 294 / pairing-host-proof 391 / pairing-sas 384 / pairing-status 171`，node-link `challenge 333 / node-proof 329 / pairing-owner-proof 366 / pairing-proof 274 / pairing-sas 358 / pairing-status 181`；`transcriptBase64url` 与 `transcriptSha256Hex` **两项全部相等**。
- **旧/新实现差分**：对 7 个 domain、140 个构造用例外加边界样例，目标实现与 13e6dbb 的**字节输出与抛错与否零差异**——证明 F1 只改“越界时是否抛错”，没有改动任何已对齐结果。
- **超限抛错而非半成形字节**：65536 / 65538 / 90000 字节均抛 `TranscriptError`；65535 字节（含多字节 `21845 × '中'` = 65535 字节）接受且前缀恰为 `0xffff`。区分度成立（旧实现只按 65536 **码元**判，21846 个 `'中'` = 65538 字节曾被放行）。

### C. F3 的 single-flight → **成立，未发现新问题**

- **收敛同一把密钥且与落盘一致**：N=5 并发同 origin → 1 个公钥、同一私钥句柄、`loadIdentity()` 相等、IndexedDB 仅 1 条记录。
- **失败路径清理（构造性检查）**：分别让 `crypto.subtle.generateKey`、`crypto.subtle.exportKey`、`indexedDB.open` 在**首个**调用时抛错，三种情形下后续调用**都能重新走完整路径并成功**（`err | then ok`）——证明 `#pendingEnsure` 由 `secure-storage.web.ts:272-274` 的 `finally` 必然清空，一次失败不会让后续调用永久挂起。两个并发调用同时失败后再重试也成功。
- **不同 origin 语义正确**：并发不同 origin 时，第一个 `fulfilled`，第二个抛 `origin_mismatch`（`secure-storage.web.ts:255-265`），**未把 A origin 的身份返回给 B**（探针 `bLeakedKey=false`，落盘 `canonicalOrigin` = A）；且 A 的 in-flight 失败时 B 也随之失败，不会绕过闸门拿到密钥。
- **附带核实（非本轮引入）**：串行路径下“第二次 `ensureIdentity` 传入不同 `deviceId`”本就返回首个 `deviceId`（r1 与 r2 行为相同），故这是既有身份模型特性，**不计为本轮缺陷**。

### D. F4 的闸门 → **成立，测试确实构造了危险形状**

- 两个持久写入口**真的接线**：`local-cache.web.ts:168`（local）与 `:193`（imported）均调用 `assertPersistableForOrigin`，并转发**整个入参**（而非只转发得到类型许可的字段）。
- **运行期允许清单**（`local-cache.ts:200`、`:230-236`）实际拦截：`{...PERSISTABLE_METADATA, body: "…"}` → `CachePolicyError{kind:"content_not_allowed"}` 且未落盘；`metadata=null` 亦被拒（较 r1 加强）；local 侧夹带 `metadata` 亦被拒。
- **测试真的构造了这种形状**：`local-cache.test.ts:205-213` 用对象展开加 `body` 键、再以 `as unknown as ImportedMetadata` **绕过类型检查**传入——正是“结构化类型拦不住、只有运行期断言能拦”的那一类，而非只测类型层面通不过的写法；`browser-checks.ts` 的 F4 子检同样如此。
- **残余边界（非本轮缺陷）**：允许清单校验的是**键名**，不校验**值**。`metadata.originCursor = "任意正文"` 仍会落盘——但该键本就是允许清单内的字段，语义内容无法由形状判定，属 R20 的固有余量，且 r1 的 F4 是“闸门未接线”，本修复已消除该问题。

---

## F1–F7 逐项复核

| ID | 原问题 | 复核结论 | 复核依据（文件:行 / 探针） |
| --- | --- | --- | --- |
| **F1** | domain 长度按码元判、写字节前缀，静默截断 | **FIXED** | `transcript.ts:168-169` 守卫基准改为 `domainBytes.length`；12/12 冻结向量逐字节一致；65535/65536 与 21845/21846 边界两侧行为正确；旧新差分 140 例零差异 |
| **F2** | `#commit`/`get` 无保护读-改-写，并发丢条目 | **FIXED** | `local-cache.web.ts:76-86` 加 `#writeChain`/`#serialize`，`:176`/`:201`/`:212`/`:243` 把 `putLocalSummary`/`putImportedMetadata`/`get`/`clear` 全部入队；我自造 6 种交错均收敛；500 次并发队列不累积；序列化区内失败不卡链 |
| **F3** | `ensureIdentity` check-then-act，并发生成双密钥 | **FIXED** | `secure-storage.web.ts:80` 声明 `#pendingEnsure`，`:255-265` 复用 in-flight 并校验 origin，`:272-274` `finally` 清空；N=5 收敛、不同 origin 抛 `origin_mismatch` 不泄漏、三条失败路径均正确清理 |
| **F4** | `assertPersistableForOrigin` 无调用点，运行期闸门不存在 | **FIXED** | `local-cache.web.ts:168`/`:193` 真的调用；`local-cache.ts:200`/`:230-236` 加强为键名允许清单；测试（`local-cache.test.ts:205-213`、`browser-checks.ts`）构造了 `{...metadata, body}` 形状 |
| **F5** | 注释指向不存在的 `*.browser.test.ts` 与 `scripts/run-browser-check.mjs` | **FIXED** | 新增 `clients/app/scripts/run-browser-check.mjs`（243 行）；`secure-storage.test.ts:15`、`testing/fake-indexeddb.ts:9`、`testing/browser-checks.ts:4` 改指真实 runner；全仓已无 `browser.test` 残留引用；本 reviewer 亲自执行该 runner → **16/16 passed、exit 0** |
| **F6** | 注册 `beforeunload` 使页面失去 bfcache 资格 | **FIXED** | `lifecycle.web.ts:36` 只挂 `pagehide`（`beforeunload` 已删）；`lifecycle.test.ts:114-115` 断言 `registered` 不含 `beforeunload`、含 `pagehide`；`ListenerBook.beforeunload` 已删；全仓代码无 `beforeunload` 残留 |
| **F7** | `DEVICE_IDENTITY_ORIGIN_KEY` 无处写入却在 clear 时被删 | **FIXED** | 常量、`index.ts:44` 再导出、`secure-storage.web.ts` 的 import 与 `#deleteRecords` 的空转 delete 全部移除；`#deleteRecords` 只删 `IDENTITY_RECORD_KEY`（`:134-146`）；全仓 grep 仅报告文件命中，行为不变 |

---

## 回归检查

| 检查项 | 结论 | 依据 |
| --- | --- | --- |
| transcript 字节与错误行为是否变化 | **未回归** | 目标实现与 13e6dbb 在 140 例（含边界）上基 64 输出与抛错与否零差异；12/12 冻结向量一致 |
| F2 串行化是否引入死锁 | **未引入** | `#serialize` 用 `#writeChain.then(operation, operation)`，前序 reject 亦继续；序列化区内真实失败后，后续 `put`/`get` 均正常完成 |
| F2 队列是否泄漏/无界增长 | **未引入** | 单次操作结束即从链尾脱开；500 次并发 `put` 后新操作仍秒级完成、`localBytes ≤ 8 MiB`、条目数收敛 |
| F3 失败路径 `#pendingEnsure` 清理 | **正确** | 三条失败注入后重试均成功（`finally` 生效） |
| F3 是否新引入 deviceId 不一致 | **非本轮引入** | 串行路径 r1 与 r2 行为相同（第二次不同 `deviceId` 均返回首个）；identity 单记录、首个写入者胜出为既有模型 |
| F5 注释指向是否仍失效 | **未回归** | runner 与 `browser-checks.ts` 均存在；`grep browser.test` 零命中 |
| F7 移除常量是否留悬空引用 | **未回归** | 全仓 grep 仅命中 `openspec/.../reports/*.md` |
| Node 侧全套件 | 只读消费 `PV3-wp5b-r2.log`：**7 files / 84 passed / 0 failed** | 未复跑（见限制） |
| 真实浏览器 runner | 亲自执行：**16/16 passed、exit 0** | `%TEMP%` 结果 JSON 与 `PV3-wp5b-r2-browser.log` 同批 |

---

## Findings（本轮新增，接第 1 轮编号）

| ID | Severity | 位置 | 结论 |
| --- | --- | --- | --- |
| `review-wp5b-r2-F8` | **P3 / 信息级** | `clients/app/scripts/run-browser-check.mjs:55-56`（`join(APP_ROOT, "node_modules", "esbuild", "lib", "main.js")`）与文件头 `:11` 的自述「零第三方依赖」 | runner 经**硬编码路径**加载 `clients/app/node_modules/esbuild`，而 `esbuild` **未**在 `clients/app/package.json` 声明——它当前由 `vite` 传递引入（`package-lock.json` 中唯一 `esbuild` 父节点是 `node_modules/vite`）并被提升到该路径。文件头「零第三方依赖」的表述因此不准确（是“未新增**声明**依赖”，而非“不使用第三方包”）。触发条件明确：任何使 esbuild 不再被提升到 `clients/app/node_modules/esbuild` 的安装布局变化（或被 `npm ci` 以更严格布局安装），`bundleChecks()` 会以 `ERR_MODULE_NOT_FOUND` 失败，使整个浏览器门禁不可用。当前本机与项目锁文件下**可正常运行**（本 reviewer 亲跑 16/16），故非阻断。**建议**（二选一）：(a) 在 `clients/app/package.json` 的 `devDependencies` 显式声明 `esbuild`；或 (b) 改为模块解析而非硬编码路径，例如 `createRequire(import.meta.url).resolve("esbuild/lib/main.js")`，并在解析失败时给出可操作报错。**注**：`clients/app/package.json` 属 Shared File Ownership 中 WP5a/WP6/WP7 的登记行（`plan.md` 明写 WP5b 不写本文件），故 (a) 需主 Agent 决定是否登记/由谁改；若采纳 (b) 则只在 WP5b 范围内即可完成。 |

> 说明：上表是本轮**新发现**。F5 相关另有两项**非缺陷观察**，不单列 finding：(i) `run-browser-check.mjs` 依赖 Node ≥22 的全局 `WebSocket`（项目 `package.json`/`rust-toolchain` 已钉 Node 22，成立）；(ii) 该 runner **未**接入 PV3/CI（`pv3` 仅跑 tsc/vitest/expo export；`.github/workflows/ci.yml` 亦未含它），本轮它靠作者手动执行，“可复现入口”已入库但尚无自动门禁——这与 r1-F5 的原意（让证据可复现）相符，是否接入 CI 属后续接线决定。

---

## 关于新增 `clients/app/scripts/run-browser-check.mjs` 的越界判断

- **事实**：`plan.md` 的 WP5b Write Scope 是 `clients/app/src/platform/`；该脚本落在其**外**（`clients/app/scripts/`）。它是本轮 diff 中唯一超出 `src/platform/` 的路径（`git diff --name-only … | grep -v '^clients/app/src/platform/'` 仅此一条）。
- **判断：属越界，但为 F5 修复所必需，且不构成共享文件冲突。** 理由：(1) r1-F5 的实质是“注释指向仓库中不存在的产物”，唯一能真正修复它的做法就是把该产物建出来；不建则 F5 只能靠删注释“抹平”，与本包最高风险点（R11 真实浏览器证据）的可复现性目标相悖。(2) 该文件不触碰任何 Shared File Ownership 登记行的共享文件（未改 `package.json`、`layers.test.ts`、`e2e/`、`*.test.ts`）。(3) 唯一的外部依赖 `esbuild` 是既有传递依赖，未新增声明依赖（见 F8）。
- **建议登记**（由 main 执行，措辞可直接采用）：

  > `clients/app/scripts/run-browser-check.mjs` | Writers: WP5b | Merge Owner: `coding-5b` | Merge Order: — | Re-verify After Merge: WP5b: PV3 | Region Note: WP5b 的 F5 修复产物（真实 Chromium 断言入口，零新增依赖：Node 内建 + 既有 esbuild 传递依赖）。单写者；同时建议把 WP5b 的 Write Scope 由 `clients/app/src/platform/` 扩记为 `clients/app/src/platform/` + `clients/app/scripts/run-browser-check.mjs`，并声明 `esbuild` 依赖归属由 main 决定（见 F8）。

---

## Assessment

### 本轮复核结论

**PASS**（对应 Target Revision `263d3ba8b48ae5dae8ab87b16bc4c0ea86f62314`）。**0 CRITICAL / 0 MAJOR**；F1–F7 **全部成立修复**；新增 1 项 P3（`review-wp5b-r2-F8`，非阻断）。按判定规则，无未解决的 CRITICAL/MAJOR，不要求清零。

- **F1** 守卫与写入同一基准（UTF-8 字节），12/12 冻结向量逐字节一致，旧新差分零差异，边界两侧正确抛错/接受。
- **F2** 写路径串行化真正闭合了 `commit`/`get` 回写的读-改-写竞态；我自造的 6 类交错全部收敛，且无死锁、无队列泄漏、失败不卡链。
- **F3** single-flight 收敛同一把密钥并与落盘一致；三条失败路径清理正确；不同 origin 并发仍 `origin_mismatch` 且不泄漏密钥。
- **F4** 闸门真接线并加强为运行期键名允许清单，`{...metadata, body}` 被拦且测试确实构造该形状。
- **F5** runner 入库、注释重指；本 reviewer 亲跑 **16/16 passed**。
- **F6** `beforeunload` 移除、断言补齐；**F7** 死常量与其空转删除彻底移除，无悬空引用。
- **无回归**：字节保真、R20 语义、身份模型均未受本轮改动影响。

### 待补 / 未由本 reviewer 执行的检查

| Check ID | 状态 | 说明 | 是否影响本轮判断 |
| --- | --- | --- | --- |
| `PV3`（tsc / vitest / expo export） | 只读日志（tsc exit 0；vitest 7 files / 84 passed；expo export 683 modules，exit 0） | **未复跑** | 否 |
| `PV1`（`npm run check` 十道门禁） | 只读日志（exit 0）；另本 reviewer 单独执行了 `check:docs`（415 links / 9058 refs，exit 0） | 未复跑全量 | 否 |
| 跨语言字节对拍（Rust ↔ TS） | 未执行（TP3 范围） | 只做了 12 个冻结向量的独立复算 | 否 |
| 真实浏览器 E2E / 渲染 / 路由 | 未执行（TP4 范围） | 只跑了平台层浏览器断言 runner | 否 |
| iOS Safari / Android Chrome | 未执行 | `docs/FRONTEND_DESIGN.md` §4.5 属更后阶段 | 否 |

## 实际检查范围

已检查（只读）：

- base→target 完整 diff（15 路径 / +648 / −75）与写入范围合规性（仅 `run-browser-check.mjs` 越出 `src/platform/`）。
- 目标提交 14 个平台文件与 1 个脚本逐行通读；`assertPersistableForOrigin`/`#serialize`/`#pendingEnsure` 的调用点与消费者（`index.ts` 装配、`browser-checks.ts` 使用面）逐条核对。
- 以 esbuild 打包目标实现到仓库外临时目录做黑盒执行；并以 `git show 13e6dbb` 抽出旧实现做并排差分（transcript 140 例、cache 6 类交错、identity 8 类并发/失败）。
- 按 `compatibility/transcripts/v1/transcripts.json` 复算 **全部 12 个**冻结向量（sync 6 + node-link 6）的 `transcriptBase64url` 与 `transcriptSha256Hex`。
- 亲自执行 `node clients/app/scripts/run-browser-check.mjs`（真实 Chromium，本机 Edge）→ 16/16 passed、exit 0。
- `package-lock.json` 中 `esbuild` 的依赖来源核查（唯一父节点 `vite`）；`tsconfig.json` include 与 `vitest.config.ts` include/environment 核查；CI workflow 与 PV3 是否接入 runner 的核查。
- `docs/FRONTEND_DESIGN.md` §7 允许清单、`schemas/sync/v1/pairing.schema.json` 的 `canonicalOrigin` pattern、`plan.md` 的 WP5b Write Scope 与 Shared File Ownership、`tasks.md` 2.6。

未验证内容：

- 未复跑 PV1/PV3 全量（只读消费 `PV1-wp5b-r2.log`/`PV3-wp5b-r2.log`）；未运行跨语言对拍（TP3）；未运行页面级 E2E / 渲染 / 路由（TP4）；未在 iOS Safari / Android Chrome 复跑。
- 未判 WP6/WP7 接线、配对 HTTP 端到端、imported 离线分流（属后续包）。
- `deps`/`advisories`/`secrets` 三个 CI-only job 本地无等价物，未运行。
- F2 串行化仅覆盖**同一端口实例内**的并发；跨标签页并发仍靠 IndexedDB 事务边界，属既有设计（本轮 F2 原问题即限于同实例）。
- `run-browser-check.mjs` 未接入 CI 的“是否应接入”属接线决策，未由本 reviewer 判定（见 F5 观察）。

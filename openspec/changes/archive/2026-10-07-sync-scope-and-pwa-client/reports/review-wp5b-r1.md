<!-- WP5b（MU3b 唯一成员）交付前检视报告（review-wp5b-r1，Round 1）。Reviewer 只报告，不修改代码、测试、规划文件、任务状态或 verification.md。 -->

task_id: "2.6"
role: reviewer
phase: branch
agent_context:
  agent_id: "review-wp5b-r1"
  isolation: "fork_turns=none（新建独立子 Agent，未参与 WP5b 实现，也未继承作者实例 coder-w5b-r1 的任何对话；只携带派发说明与仓库内契约文档）"
target_revision: "13e6dbbedbae4e32196c994391f73acc95ba323f"
base_revision: "a4440d683dc44ecb69d2ffeb596fbff53170272c"
scope: "仅检视 a4440d68..13e6dbb 的 14 files / +2359 / −0（clients/app/src/platform/**），按派发说明的 A–F 六点逐项核实。不判 WP5a/WP6/WP7/TP3/TP4 的实现。"
changes: "只读检视，未修改任何受版本控制的文件；仅新增本报告。（为独立复现并发行为，曾在 .worktrees/wp5b 内临时创建两枚探针测试文件并即刻删除，git status --porcelain 为空；另把 transcript.ts 打包到仓库外系统临时目录执行，未触碰仓库。）"
issues: "0 CRITICAL / 0 MAJOR；5 MINOR（F1–F5）/ 2 SUGGESTION（F6–F7）。无 patch 引入的阻断缺陷。另有 2 项跨包观察（X1–X2，归属 WP5a）。"
result: PASS
evidence_paths:
  - "openspec/changes/sync-scope-and-pwa-client/reports/review-wp5b-r1.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/deliver-wp5b-r1.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/PV3-wp5b-r1.log"
resource_cleanup: NOT_APPLICABLE（只读检视；探针文件与仓库外临时产物不在版本控制内）

```agentic-handoff
version: 1
agent_context:
  agent_id: "review-wp5b-r1"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "2.6"
    work_package: WP5b
    role: reviewer
    phase: branch
    round: 1
    stage: work-package
    target_revision: "13e6dbbedbae4e32196c994391f73acc95ba323f"
    evidence_type: REVIEW
    evidence_id: review-wp5b-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/review-wp5b-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "新建独立子 Agent 对 13e6dbb（base=a4440d6，14 files/+2359/−0，全部在 clients/app/src/platform/）做只读检视。A（R11）：generateKey(false) 后立即断言私钥 extractable===false、读取路径 #validateRecord 重新断言、私钥唯一使用点是 subtle.sign、无任何 exportKey 私钥调用、无 localStorage/明文降级路径；Node 22 实测非可导出私钥 exportKey(pkcs8/jwk) 抛 InvalidAccessException，真实 Chrome 证据由仓库外临时产物 browser-evidence.json 与 PV3 时间戳一致佐证。B（字节保真）：以独立 Python 实现与『把被检实现用 esbuild 打包到临时目录执行』两条互不相同的路径，对 device-proof 与 host-challenge 复算 base64url+SHA-256 全部一致；被检实现对 6 个 fixture 向量全部逐字节一致。C（R20）：全平台层无 localStorage/Cache Storage 写入路径，imported 正文无持久入口；8MiB/30 天/LRU 三者在纯策略与持久路径上均生效。D：页面/组件零 import 平台层、platform 层无反向 import。E/F 见报告正文。"
    source_evidence: "openspec/changes/sync-scope-and-pwa-client/plan.md（WP5b 行）；tasks.md 2.6；specs/pwa-web-client/spec.md"
```

## Review Context

| 项 | 取值 |
| --- | --- |
| Review ID / Round | `review-wp5b-r1` / 1 |
| Review Type / Stage | `branch` / 工作包交付前检视（MU3b 候选之前） |
| Work Package | WP5b（交付单元 MU3b 唯一成员，Order 5） |
| Repository / 检视工作区 | `D:\Project\acp-remote` / `D:\Project\acp-remote\.worktrees\wp5b`（只读；`git status --porcelain` 为空） |
| Base / Target | `a4440d683dc44ecb69d2ffeb596fbff53170272c` → `13e6dbbedbae4e32196c994391f73acc95ba323f`（分支 `feat/wp5b`，单提交） |
| 真实 diff | 14 files changed, 2359 insertions(+), 0 deletions(-)；`git diff --name-only a4440d6 13e6dbb \| grep -v '^clients/app/src/platform/'` 为空 |
| 读取的规则 | `AGENTS.md` §5、`specs/pwa-web-client/spec.md`（R10/R11/R20）、`docs/FRONTEND_DESIGN.md` §3/§4.5/§5/§7、`docs/SYNC_PROTOCOL.md` §6.2/§6.3、`docs/IDENTITY_AND_AUTH_CONTRACT.md` §7、`compatibility/transcripts/v1/transcripts.json`、`crates/acpr-transcript/src/lib.rs`、`crates/acpr-transcript/src/table.rs` |
| 使用的验证证据 | `reports/PV3-wp5b-r1.log`（只读消费，**未**复跑）；`reports/deliver-wp5b-r1.md`（作品，**不作为检视证据**）；仓库外临时产物 `%TEMP%\wp5b-browser\browser-evidence.json`（只读） |
| 限制 | 不复跑 tsc/vitest/expo export；不执行跨语言对拍（TP3）；不执行 E2E（TP4） |

### 独立复现手段（本 reviewer 亲自执行）

1. **独立 Python 实现**：按 `docs/SYNC_PROTOCOL.md` §6.2/§6.3 从零重写 codec，对 `fixtures/sync/v1/transcripts/device-proof.json` 与 `host-challenge.json` 的 `input` 复算 —— base64url 与 SHA-256 两项均与向量 `expected` 相等。
2. **被检实现的黑盒执行**：用 `esbuild` 把 `src/platform/transcript.ts` 打到仓库外临时目录，按 `compatibility/transcripts/v1/transcripts.json` 的字段表逐 domain 编码 —— **6 个向量全部逐字节一致**（device-proof 303、host-challenge 298、pairing-proof 294、pairing-host-proof 391、pairing-sas 384、pairing-status 171）。
3. **Node 22 密码学行为实测**：非可导出 P-256 私钥的 `exportKey("pkcs8"/"jwk")` 抛 `InvalidAccessException`；公钥 `exportKey("raw")` 得 65 字节；`sign` 得 64 字节；`typeof indexedDB === "undefined"`；`structuredClone(CryptoKey)` 保留 `extractable === false`。
4. **并发行为探针**：在 `.worktrees/wp5b` 内临时建两枚 vitest 探针（用本包的 `installFakeIndexedDB`），跑完即删。

---

## A. R11 —— 不可导出密钥是否真的不可导出 → **成立（无降级明文路径）**

| 判据 | 结论 | 依据 |
| --- | --- | --- |
| 生成即 `extractable: false` | 成立 | `secure-storage.web.ts:193` 第三实参传 `false`；`:201` 生成后**再**断言 `privateKey.extractable !== false` 即抛 `crypto_unavailable`（失败关闭） |
| 存在路径把私钥变成可导出 | **不存在** | 全文件 `privateKey` 的 11 处用法逐条核对：4 处校验、2 处赋值进身份对象/持久记录、1 处 `subtle.sign`；**无一处** `exportKey`。`exportKey` 仅 1 处（`:215`），参数是 `publicKey` |
| 重新 `importKey` 时误设 `true` | **不存在** | 本包**没有**任何 `importKey`（`testing/browser-checks.ts:159` 的 `importKey(…, true, ["verify"])` 只导入公钥，在测试面） |
| 导出公钥时连带导出私钥 | 不成立 | 生成时 `publicKey` 为 `extractable: true`（WebCrypto 规范如此，与私钥标志无关）；`#exportPublicKey` 只对公钥调 `raw`，并校验 65 字节 `0x04` 前缀 |
| 私钥 JWK/pkcs8 落 IndexedDB | **不存在** | 持久记录形状 `PersistedIdentityRecord`（`:45-53`）只有 `deviceId`/`canonicalOrigin`/`publicKeyBase64Url`/`privateKey: CryptoKey`；`put` 的是对象而非序列化文本 |
| 读取路径强制点覆盖所有返回私钥的路径 | **覆盖** | 两条出口：(a) `loadIdentity():223` → `#validateRecord():147`，`:160` 强制 `extractable !== false`；(b) `ensureIdentity():238` 的新生成分支已由 `:201` 断言，`existing` 分支来自 (a)。无第三条 |
| 私钥被用于解密/导出/日志 | 不成立 | 唯一使用点是 `subtle.sign`（`:291`）；全平台层无 `console.*`（`grep` 零命中）；错误消息不含密钥材料 |
| **降级路径（WebCrypto/IndexedDB 不可用）** | **抛错，不静默降级** | `#openDatabase():77` 在 `typeof indexedDB === "undefined"` 时 `reject(storage_unavailable)`；`#generateNonExtractableKeyPair():185` 在缺 `crypto.subtle` 时抛 `crypto_unavailable`；`:198` `generateKey` 失败亦抛。**没有**任何回退到 localStorage 或可导出密钥的分支 |

**对「结构化克隆是否可靠」的独立判断**：`CryptoKey` 在 Web Crypto / IndexedDB 规范中是**可序列化**类型，序列化时携带其 `[[extractable]]` 内部槽；因此 IndexedDB 往返后 `extractable` 保持 `false` 是规范保证的行为，而非实现巧合。Node 22 实测 `structuredClone(kp.privateKey).extractable === false` 与之一致。

**对 Chromium 断言强度的判断（关键）**：`testing/browser-checks.ts:118-134` 用 `getAll()` + `JSON.stringify` 检查「记录不含 `"d"`/`pkcs8`」。`CryptoKey` 经 `JSON.stringify` 序列化为 `{}`，故该断言**看不到 Key 对象内部的私有槽**——它证明的是「记录里没有**字符串/blob 形态**的私钥材料」，这是有价值的（任何 `exportKey` 产物都会以字符串出现），但**不能**单独证明「Key 对象本身不可导出」。真正承担 R11 的断言是同文件 `:108-115` 的**重开端口后 `rehydrated.privateKey.extractable === false`**——它走的是真实 IndexedDB 读回路径，是充分的。两者合起来证据强度足够。

## B. transcript 编码的字节级保真 → **成立（与向量逐字节一致）**

- 规范对齐：magic/codecVersion/domainLength(u16be)/domain/fieldCount(u16be)/逐字段 `tag(u16be)‖len(u32be)‖bytes`，与 `docs/SYNC_PROTOCOL.md` §6.2 的 `codec.description` 与 `crates/acpr-transcript/src/lib.rs:73-115` 同构；重复 tag 先于乱序判定（`transcript.ts:187-196` 与 Rust `encode` 判定顺序一致）；`nul-joined-utf8` 无尾随 NUL 且**校验**（非重排）升序唯一；`u64be` 用 `setBigUint64`；UUID 取 16 原始字节。
- **独立复算**：见上文「独立复现手段」1/2。两条互不相同的路径都得到「与向量逐字节一致」，且被检实现对 **6 个** fixture 向量全部通过——**不是只对一个向量调参**。向量与实现一致，不存在「改向量迎合实现」。
- 与 Rust 侧语义一致，且**不是凑字节**：两者的结构、判定顺序、错误分类（`duplicate_tag`/`field_order`/`too_long`）逐条对应；本 reviewer 的 Python 复算独立于两侧。
- 一处**非缺陷**不一致（登记不计发现）：`encodeUuid16:107` 用 `replaceAll("-", "")` 去掉**任意位置**的连字符，故 `"0-00102030...`（33 字符、连字符位置错误）与无连字符的 32 位 hex 都会被接受，比注释声称的「canonical 带连字符」宽。但因产出的 16 字节对**任何合法 UUID 文本**完全相同，**不产生错误字节**，且规范 §6.2 只要求「UUID 解码为 16 原始字节」，故不构成正确性缺陷。

## C. R20 —— imported 资源不落盘 → **成立（含 1 项运行时守门缺口 F4）**

- **持久化路径上只有元数据**：`LocalCachePort` 只有 `putLocalSummary`/`putImportedMetadata`/`get`/`clear`/`stats`；`ImportedMetadata`（`local-cache.ts:83-92`）字段为 owner/export/session 标识、cursor、`localSequence`、`contentDigestSha256`、`acks`，**无正文字段**。全平台层 `grep` 无 `localStorage`/`sessionStorage`/`caches.`/`CacheStorage`（唯一命中是 `secure-storage.ts:38` 的注释）。imported 正文的唯一载体是 `createVolatileImportedContent()` 的内存 `Map`（`local-cache.web.ts:36-54`），无持久实现。
- **固定上限 + TTL + LRU 真实实现且可触发**：常量 8 MiB / 30 天 / 2 MiB / 7 天（`local-cache.ts:31/34/42/45`）与 `docs/FRONTEND_DESIGN.md:245` 一致；`evictToFit`（`:154-189`）先丢过期再按 `lastUsedAtMs` 升序淘汰；持久路径 `#commit:116-133` 对 local 与 imported **分别**配额收敛，`get:176-204` 命中时刷新 `lastUsedAtMs`、过期即从存储删除。三个淘汰规则**都**在真实路径上生效（`local-cache.test.ts` 的 5 项持久/策略用例覆盖，PV3 中 9 passed）。
- **缺口（F4）**：`assertPersistableForOrigin`（`local-cache.ts:205`）被注释声明为「R20 的**唯一**落盘守门函数」，但它**从未被任何持久化路径调用**（全仓只有 `index.ts:38` 的重导出与测试调用）。当前 imported 正文进不去持久层靠的是 `#commit` 只在 `putImportedMetadata` 的 `ImportedMetadata` 形状上构造条目——这是 TS 的**结构化**类型，而结构化类型允许**更宽**的对象经变量赋值通过，因此该守门并非冗余，而是**未接线**。

## D. 平台层边界与端口 → **成立（另见 X2 跨包风险）**

- **页面/组件不直接触碰平台能力**：`clients/app/app/**` 与 `src/components/` 对 `src/platform/` 的 import 数为 **0**（`grep` 零命中）；`src/platform/` 下 14 个文件的 import 全部指向 `./` 同级或 `../protocol`（仅测试用 `../protocol/paths`），**无反向 import**。
- **对外只暴露端口**：`platform/index.ts` 导出 `DeviceIdentityPort`/`LocalCachePort`/`VolatileImportedContent`/`LifecyclePort` 四个接口 + `openPlatform(): PlatformPorts` 单一装配点；平台实现在 `.web.ts` 内、页面拿不到 `indexedDB`/`crypto.subtle`。
- **`layers.ts` 分层定义正确**：`PLATFORM_LAYER` 不在 `LAYER_ORDER` 序号链上，`layers.test.ts:178` 对任何「import 目标为 `src/platform/`」记违规，`:170` 跳过 `from === PLATFORM_LAYER`（平台层自身不受方向检）。本包新增文件**没有**违反这两条。
- 边界纪律的额外核对：`secure-storage.ts`/`local-cache.ts`/`lifecycle.ts`（端口声明文件）确实零平台 API，平台 API 只出现在三个 `.web.ts` 与 `testing/`；全仓无自有 `.native.ts`（`find` 结果 0），与 `tasks.md` 2.6 的 v1 只交付 Web 裁定一致。

## E. 作者自报的 `PLATFORM_API_SPECIFIERS` 白名单缺口 → **缺口确实存在，但归属 WP5a，不阻断本包**

- **缺口是否为真**：**为真**。`layers.test.ts:139-142 isPlatformApiSpecifier` 的输入是 `importedSpecifiers()` 抽出的 **import 说明符**；页面直接写 `indexedDB.open(...)` / `crypto.subtle.generateKey(...)` / `document.visibilityState` / `navigator.storage` 时**没有任何说明符可匹配**，该禁令对这些写法是空断言。本 reviewer 复核了该函数覆盖的 12 个通道与 `node:*` 前缀：只拦「包/子路径」形态。
- **严重度**：**MINOR / 信息级**。R10 在该情形下有**两半**：结构上（层目录 + 端口）合规，机器断言上缺一半。但**实现本身合规**——本包全部平台访问都收敛在 `src/platform/`，`app/`+`components/` 零命中。因此它是对 spec 的**强制机制不完整**，不是 spec 的实质违反，不阻断本包。
- **归属**：**WP5a**。`src/layers.test.ts` 是 WP5a 的交付面（`verification.md` 的 `2.5-fix` 行登记了「平台白名单查表与 `isPlatformApiSpecifier`」），作者未改动它有据；本 reviewer 亦确认该文件不在本 diff 内（`git diff --name-only` 为空），故**不列为本包 finding**（X1）。
- **附带发现（X2，跨包风险，非本包缺陷）**：同一条规则会在下一个包炸掉——`layers.test.ts:178` 把**任何**层 import `src/platform/` 都记为违规（含 `app`），而 WP6/WP7 必须创建组合根来调用 `openPlatform()`。也就是说，**当前 `layers.test.ts` 使 WP6/WP7 的必需接线在 PV3 上不可通过**，除非为组合根加豁免（例如只允许 `app/` 下一处具名入口 import `src/platform`）。此为**计划层/上游包的缺口**，WP5a 的 `review-wp5a-r1` 已登记「仓库内不存在被指名的组合根，按有意取舍处理，建议登记」，建议主 Agent 在 MU3c/MU3d 派发前把它显式化。

## F. 测试环境限制对 R11 证据强度的影响 → **强度足够，但可复现性有缺陷（F5）**

- **限制为真**：Node 22.22.0 实测 `typeof indexedDB === "undefined"`；`testing/fake-indexeddb.ts` 自述「值按引用保存」，因此它**不可能**发现结构化克隆不再保留 `extractable: false` 的行为——替身对「clone 保真」这一关键性质是**盲区**。
- **关键断言是否仍有足够证据**：**有**。R11 的实质判据是「真实 IndexedDB 往返后句柄仍不可导出」与「不可导出句柄的 `exportKey` 抛错」，两者都**只**由 Chromium 子检承担，且：(a) 该行为有规范保证（见 A 节）；(b) 仓库外临时产物 `%TEMP%\wp5b-browser\browser-evidence.json` 含 11 条断言的原始结果（全 `passed: true`，含 `exportKey(pkcs8)` 拒绝类型 `InvalidAccessError`、往返后 `extractable=false`、记录数 1），其 mtime `11:32` 与 `PV3-wp5b-r1.log` 的 `11:32:56` 同批，与作者自报的 11/11 吻合；(c) 本 reviewer 在 Node 侧独立复核了同一密码学前置（非可导出私钥的 `exportKey` 抛 `InvalidAccessException`）。因此**替身可能掩盖的行为，恰由真实浏览器证据覆盖**，R11 结论不受该限制动摇。
- **可复现性缺陷（F5）**：注释指向的两处产物**在仓库中不存在**——`secure-storage.test.ts:15` 与 `testing/fake-indexeddb.ts:9` 都写「由 `src/platform/secure-storage.browser.test.ts`（Chromium）核对」，但该文件在**所有分支**上都不存在；`testing/browser-checks.ts:4` 写「由 `scripts/run-browser-check.mjs` 注入页面」，而 `clients/app/scripts/` 与 `scripts/run-browser-check.mjs` 均不存在。即：**声称常驻的那半证据缺失，Chromium 子检的执行脚手架不在仓库内**，产物只存在于会被清理的系统临时目录。这使本包最高风险点的最强证据**不可复现**（本 reviewer 只能消费临时产物）。

---

## Findings

| ID | Severity | 位置 | 触发条件 / 证据 | 预期 / 实际 | 影响 | 建议 |
| --- | --- | --- | --- | --- | --- | --- |
| `review-wp5b-r1-F1` | MINOR | `clients/app/src/platform/transcript.ts:164`（守卫）与 `:180`（`setUint16(5, domainBytes.length, false)`） | 传入 UTF-8 字节长度 > 65535 但 `String.length`（UTF-16 code unit 数）≤ 65535 的 `domain`（如 `"\u4e2d".repeat(30000)`） | 守卫用 `domain.length` 判上限（应为 `domainBytes.length`，与 Rust `lib.rs:77` 的 `domain.len()` 即字节长度同义）；`setUint16` 随后把超界值**静默截断**。实测：code units 30000 通过守卫，UTF-8 90000 字节，写入长度前缀 `24464`（`90000 % 65536`），`TextDecoder` 无法还原 domain 文本 | 产出的 transcript 头部与真实 domain 长度不符，跨语言对拍/解码必然失败，且**不抛错**（违反「非法时抛 `TranscriptError`，不产出半成品字节」的自述契约） | 把守卫改为对 `domainBytes.length` 判定，先编码再检查：`if (domain.length === 0 \|\| domain.includes("\u0000") \|\| domainBytes.length > 0xffff)`。当前所有调用方都用协议常量 domain（< 64 字节），**不可达**，故非阻断 |
| `review-wp5b-r1-F2` | MINOR | `clients/app/src/platform/local-cache.web.ts:116-133`（`#commit` 的读-改-写 + `#replaceAll`）与 `:176-204`（`get` 内的两次 `#replaceAll`） | 同一页面并发发起两个缓存操作（`await Promise.all([...])`）。本 reviewer 用本包的 `installFakeIndexedDB` 建探针实测 | 预期：每次 `put` 都在前一次落盘结果之上累加。实际：`Promise.all([putLocalSummary("A"), putImportedMetadata("B")])` 后 `stats` 只剩 **1** 条（另一条被覆盖丢失）；`Promise.all([putLocalSummary("A"), clear()])` 后 `clear()` **未清干净**（`localEntries` 仍为 1） | 无并发保护的全量读改写会让并发写入互相覆盖；`clear()` 是 R20 场景 4「清除数据」的落点，被并发 `put` 抢跑时可能残留条目。`docs/FRONTEND_DESIGN.md:214`/`SYNC_PROTOCOL.md:611` 明确多标签页要协调连接，WP6/WP7 接线后同页并发调用是现实场景 | 在端口实例内以 promise 链（或 IndexedDB 事务边界内的按 key 读改写 + 单一配额扫描）串行化写路径 |
| `review-wp5b-r1-F3` | MINOR | `clients/app/src/platform/secure-storage.web.ts:238-278`（`ensureIdentity` 的 check-then-act） | 并发两次 `ensureIdentity`（同页双击配对、或两个标签页同时配对）。探针实测 | 预期：文档自述「已存在身份且来源相同则返回既有身份（**幂等**）」。实际：两次调用都读到 `loadIdentity() === null`，各自 `generateKey` 并 `put`；返回的两个句柄公钥**不同**，且**先返回者的句柄与持久化结果不一致**（探针输出 `sameInMemory:false, persistedMatchesA:false, persistedMatchesB:true`） | 某个调用方会持有一把「不是落盘身份」的私钥句柄去签 device proof，导致认证失败/返工配对；同一 `deviceId` 下出现两把密钥的材料流转 | 用 in-flight promise（`#pending`）合并并发 `ensureIdentity`，或在写入前以 IndexedDB 事务内的存在性检查（`add` 语义）保证单次生成 |
| `review-wp5b-r1-F4` | MINOR | `clients/app/src/platform/local-cache.ts:205-224`（`assertPersistableForOrigin`） | 任何「imported 正文进持久层」的调用路径 | 文档自述：「R20 的**唯一**落盘守门函数……任何想把 imported 正文写进持久层的调用路径都必须先过这里」。实际：该函数**从未**被 `local-cache.web.ts` 的 `putLocalSummary`/`putImportedMetadata`/`#commit` 调用；全仓引用只有 `index.ts:38` 的重导出与 `local-cache.test.ts` | R20 的运行时守门实际不存在；当前 imported 正文进不去持久层**只**靠 `ImportedMetadata` 的 TS 结构化类型——而结构化类型允许经变量传入更宽的对象（多余字段不被检查），因此一个 `{...metadata, body}` 形状可以合法流入 `putImportedMetadata` 并被 `#commit` 落盘。是可触发前的**护栏缺口** | 在 `putImportedMetadata` 落盘前调用 `assertPersistableForOrigin({ kind: "imported" }, { metadata })`（并在 `putLocalSummary` 调 `{ kind: "local" }, { summary }`），或删除该函数与相应文档表述，不与实现两说 |
| `review-wp5b-r1-F5` | MINOR | `clients/app/src/platform/testing/fake-indexeddb.ts:8-9`、`clients/app/src/platform/secure-storage.test.ts:15`、`clients/app/src/platform/testing/browser-checks.ts:4` | 按注释去找被指名的产物 | 预期：存在 `src/platform/secure-storage.browser.test.ts` 与 `scripts/run-browser-check.mjs`。实际：两者在**所有分支**上都不存在（`git branch` 遍历 + `git log --all` 零命中）；`clients/app/scripts/` 目录不存在 | 本包最高风险点（R11 不可导出 + 真实结构化克隆）的**最强证据不可复现**：Chromium 结果只以系统临时目录里的 `browser-evidence.json` 形式存在，脚手架、入口、断言源码均未入库；注释指向不存在的文件会误导后续维护者认为「有常驻浏览器用例」 | 把 `browser-checks.ts` 的 runner（esbuild 打包 + 静态服务 + 断言入口）与一个真实的 `*.browser.test.ts`（或 `e2e/`，注意 `plan.md` 把 `e2e/` 归 TP4）落进仓库并接入 PV3，使 11 条断言可复跑 |
| `review-wp5b-r1-F6` | SUGGESTION | `clients/app/src/platform/lifecycle.web.ts:32-34`（`window.addEventListener("beforeunload", onPageHide)`） | 任一次 `subscribe()`（订阅期间监听器常驻） | 注释自述「`pagehide` 是 bfcache 场景下可靠的一侧；`beforeunload` 只作兜底」。实际：注册 `beforeunload` 监听器恰是 bfcache 的失格诱因（[web.dev bfcache](https://web.dev/articles/bfcache)：桌面 Chrome 与 Firefox 会因 `unload` 监听器使页面失格；Firefox 对 `beforeunload` 同样如此），而本实现只在退订时移除该监听器 | 可能使页面被排除出 bfcache，前进/后退时改为重新加载并重跑设备签名握手，抵消 `pagehide` 选择的初衷；影响限于性能/体验，与 R11/R20 语义无关 | 去掉 `beforeunload`，只保留 `pagehide`（bfcache 与正常卸载两侧都可靠）；若须兼容极旧浏览器，再按能力探测挂载 |
| `review-wp5b-r1-F7` | SUGGESTION | `clients/app/src/platform/secure-storage.ts:43`（`DEVICE_IDENTITY_ORIGIN_KEY`），清理点 `secure-storage.web.ts:132` | 静态阅读 | 预期：该常量是「来源变化必须重新配对」的持久记录键。实际：`#deleteRecords` 删除它，但**没有任何写入**该 key 的代码（来源绑定存在同一条 `PersistedIdentityRecord.canonicalOrigin` 里），`DEVICE_IDENTITY_STORE` 亦只作为 object store 名使用 | 导出面多出一个永远为空的记录键，`clearIdentity` 的删除动作是空转；对行为无影响（不是缺陷） | 删除该常量与其删除调用，或把它作为独立记录真正写入 `ensureIdentity`，二者取一以消除「声明了却没接线」的歧义 |

### 跨包观察（不计为本包 finding，归属 WP5a）

| ID | 位置 | 结论 |
| --- | --- | --- |
| `X1` | `clients/app/src/layers.test.ts:139-142`（`isPlatformApiSpecifier`），白名单表 `:57-81` | 作者自报的缺口**成立**（详见 §E）：白名单只匹配 import 说明符，页面直接调用浏览器全局（`indexedDB.open`/`crypto.subtle`/`navigator.storage`）不被机检。**实现本身合规**，故为「强制机制不完整」而非 spec 实质违反，MINOR/信息级；归属 WP5a（`verification.md` 的 `2.5-fix` 行已登记该文件的交付） |
| `X2` | `clients/app/src/layers.test.ts:170-181` | 同一条规则**禁止任何层**（含 `app`）import `src/platform/`，而 WP6/WP7 必须创建组合根调用 `openPlatform()`。若不先给组合根加豁免，WP6/WP7 的必需接线在 PV3 上必然失败。属计划层/上游缺口，建议在 MU3c/MU3d 派发前显式化（`review-wp5a-r1` 已登记「仓库内不存在被指名的组合根，按有意取舍处理」） |

---

## Assessment

### 本轮检视结论

**PASS**（对应 Target Revision `13e6dbbedbae4e32196c994391f73acc95ba323f`）。**0 CRITICAL / 0 MAJOR**；5 MINOR（F1–F5）/ 2 SUGGESTION（F6–F7）。**无未解决的 CRITICAL/MAJOR**，按判定规则不要求清零。

- **A（R11 不可导出）成立**：生成即 `extractable:false` 且二次断言；读取路径 `#validateRecord` 覆盖全部私钥出口；私钥唯一使用点是 `subtle.sign`；无 `importKey`、无 `exportKey("pkcs8"/"jwk", privateKey)`、无 `localStorage` 降级；能力缺失一律抛分类错误（`crypto_unavailable`/`storage_unavailable`），**不存在静默退化为可导出或明文**。chromium 断言中「JSON 无 `d`/pkcs8」一项强度有限（`CryptoKey` 序列化为 `{}`），但真正承担 R11 的是「真实 IndexedDB 往返后仍 `extractable===false`」——该断言充分且与 WebCrypto/IndexedDB 规范行为一致。
- **B（字节级保真）成立**：以独立 Python 实现与被检实现的临时目录黑盒执行两条互不相同的路径，对 device-proof/host-challenge 复算一致；被检实现对 **6 个** fixture 向量逐字节一致；与 Rust `acpr-transcript` 语义逐条对应，非凑字节。唯一偏差（F1）是 domain 长度守卫取 code unit 而非 UTF-8 字节，当前调用方不可达。
- **C（R20 imported 不落盘）成立**：持久层类型上无正文字段、无 `localStorage`/Cache Storage 路径、imported 正文只在内存 `Map`；8 MiB/30 天/TTL/LRU 三者在真实持久路径上均生效且有测试触发。护栏缺口 F4（守门函数未接线）为潜在风险，当前 API 形状下不可触发。
- **D（边界与端口）成立**：页面/组件零 import 平台层，平台层零反向 import；对外只暴露四个端口与单一装配点 `openPlatform()`；`layers.ts` 把 `platform` 单列为跨切层并禁止被内层依赖。
- **E（白名单缺口）成立但归 WP5a**：缺口真实（机检看不到浏览器全局），实现合规，MINOR；并发现 X2——该测试当前使 WP6/WP7 的必需接线不可通过，需在后续包派发前显式化。
- **F（测试环境限制）可控**：Node 侧替身对结构化克隆是盲区，但该盲区恰由 Chromium 子检覆盖，且 R11 行为有规范保证、临时产物与 PV3 同批可佐证；缺陷在于该子检**不可复现**（F5：注释指向的 `secure-storage.browser.test.ts` 与 `scripts/run-browser-check.mjs` 在仓库中不存在）。

### 待补 / 未由本 reviewer 执行的检查

| Check ID | 状态 | 说明 | 是否影响本轮判断 |
| --- | --- | --- | --- |
| `PV3`（tsc / vitest / expo export） | 主 Agent 已执行，本 reviewer 只读日志（三段 exit 0，77 passed，684 modules，bundle 内 node 内建 0） | **未复跑** | 否 |
| 跨语言字节对拍（Rust `acpr-transcript` ↔ TS） | 未执行（TP3 范围） | 本 reviewer 只做了独立第三方（Python）复算与被检实现的向量比对 | 否 |
| 真实浏览器 E2E / 渲染 / 路由 | 未执行（TP4 范围） | `expo export` 只证明构建可通过 | 否 |
| Chromium 11 项子检的执行本身 | 未复跑 | 只在仓库外临时产物 `%TEMP%\wp5b-browser\browser-evidence.json` 上只读核对（11/11 passed，mtime 与 PV3 同批）；**脚手架不在仓库内**（F5） | 否 |

## 实际检查范围

已检查（只读）：

- base→target 完整 diff（14 路径 / +2359 / −0）与 `git diff --name-only` 范围合规性。
- 14 个新增文件逐行通读；全平台层 `privateKey`/`exportKey`/`indexedDB`/`localStorage`/`console` 的逐点审计；`grep` 全 `src/` 的页面层平台 import 数。
- `fixtures/sync/v1/transcripts/{device-proof,host-challenge,...}.json` 与 `compatibility/transcripts/v1/transcripts.json` 的字段表；对 6 个向量做独立字节复算（Python + 被检实现黑盒）。
- 与 `crates/acpr-transcript/src/lib.rs`/`table.rs` 的结构/判定顺序对照。
- Node 22.22.0 的实测：`indexedDB` 缺失、非可导出私钥 `exportKey` 抛错、`structuredClone(CryptoKey)` 保留 `extractable=false`。
- 并发探针（2 项，用本包 `installFakeIndexedDB`，跑完即删；`git status --porcelain` 为空）。
- 真实 Chromium 子检的原始结果 JSON（仓库外临时目录）；`git branch` 全分支遍历 + `git log --all` 搜索 `browser.test`/`browser-check`。
- `docs/FRONTEND_DESIGN.md` §3/§4.5/§5/§7、`docs/SYNC_PROTOCOL.md` §6.1–§6.3、`docs/IDENTITY_AND_AUTH_CONTRACT.md` §7、`specs/pwa-web-client/spec.md` 的 R10/R11/R20，`plan.md` 的 WP5b 行与 Write Scope，`tasks.md` 2.6，`verification.md` 的相关登记行。

未验证内容：

- 未复跑 PV3（只读消费 `reports/PV3-wp5b-r1.log`）；未运行跨语言对拍（TP3）；未运行 E2E / 真实浏览器渲染与路由（TP4）；未在 iOS Safari / Android Chrome 复跑（`docs/FRONTEND_DESIGN.md` §4.5 的跨浏览器验收属更后阶段）。
- 未判 WP6/WP7 的接线、配对 HTTP 端到端、imported 离线分流的端到端行为（均属后续包）。
- `deps`/`advisories`/`secrets` 三个 CI-only job 本地无等价物，未运行。
- Chromium 子检的执行过程未复现，仅核对其临时产物（F5）。

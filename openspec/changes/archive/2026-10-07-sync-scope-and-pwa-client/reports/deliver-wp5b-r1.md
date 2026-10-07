<!-- WP5b（MU3b）交付报告。coder 只交付实现、可核对证据与工作包 Project Verify；不判独立 review、合入与最终验收。 -->

# WP5b 交付（deliver-wp5b-r1）

## Shared Report

- **task_id**: 2.6（实现）/ 3.11（PV3）
- **work_package**: WP5b（交付单元 MU3b，独立单元，Order 5）
- **role**: coder（`coder-w5b-r1`，Attempt 1）
- **phase**: implement
- **agent_context**: `agent_id: coder-w5b-r1`，`isolation: fork_turns=none`（新实现实例接管 `.worktrees/wp5b`）
- **起点提交**: `a4440d683dc44ecb69d2ffeb596fbff53170272c`（`refs/heads/main` / `feat/wp5b` 的基点）
- **交付提交**: `13e6dbbedbae4e32196c994391f73acc95ba323f`（分支 `feat/wp5b`）
- **scope**: 只在 `.worktrees/wp5b` 内新建 `clients/app/src/platform/`（14 个文件）；未触碰 `schemas/**`、`fixtures/**`、`crates/**`、`docs/**`、`clients/app/package.json`、`clients/app/scripts/`、`src/layers.test.ts`、其他 worktree、主检出。报告与日志写入权威 planning root（worktree 内无 `openspec/changes/`）
- **result**: **PASS**（PV3 三段在目标提交上 exit 0）

### 交付提交

| 项 | 取值 |
| --- | --- |
| worktree | `D:\Project\acp-remote\.worktrees\wp5b` |
| 分支 | `feat/wp5b` |
| 起点（base） | `a4440d683dc44ecb69d2ffeb596fbff53170272c` |
| 交付（target） | `13e6dbbedbae4e32196c994391f73acc95ba323f` |
| 提交信息 | `feat(frontend): WP5b——platform 层：不可导出设备身份、transcript 编码、缓存与生命周期端口` |

`git diff --stat a4440d6..13e6dbb`（14 files changed, **2359 insertions(+)**, 0 deletions）：

```
 clients/app/src/platform/index.ts                  |  74 +++++
 clients/app/src/platform/lifecycle.test.ts         | 114 ++++++++
 clients/app/src/platform/lifecycle.ts              |  43 +++
 clients/app/src/platform/lifecycle.web.ts          |  57 ++++
 clients/app/src/platform/local-cache.test.ts       | 174 ++++++++++++
 clients/app/src/platform/local-cache.ts            | 235 ++++++++++++++++
 clients/app/src/platform/local-cache.web.ts        | 240 ++++++++++++++++
 clients/app/src/platform/secure-storage.test.ts    | 140 ++++++++++
 clients/app/src/platform/secure-storage.ts         | 113 ++++++++++
 clients/app/src/platform/secure-storage.web.ts     | 311 +++++++++++++++++++++
 clients/app/src/platform/testing/browser-checks.ts | 207 ++++++++++++++
 clients/app/src/platform/testing/fake-indexeddb.ts | 162 +++++++++++
 clients/app/src/platform/transcript.test.ts        | 234 ++++++++++++++++
 clients/app/src/platform/transcript.ts             | 255 +++++++++++++++++
```

`git diff --name-only a4440d6..13e6dbb | grep -v '^clients/app/src/platform/'` 为空：写入范围无越界。

---

## 1. R11 —— 设备身份由不可导出密钥承载

### 实现

`secure-storage.web.ts`（`openDeviceIdentityStore()`）：

1. `crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, false, ["sign", "verify"])`
   —— `extractable: false` 在**生成时**固定；生成后立即断言 `keyPair.privateKey.extractable === false`，环境若返回可导出私钥即失败关闭。
2. 持久化只把 **`CryptoKey` 对象本身**与公开材料（`deviceId`、`canonicalOrigin`、65 字节 SEC1 公钥的 base64url）交给 IndexedDB。没有 PKCS8/JWK/hex/base64 形式的私钥写入路径。
3. `loadIdentity()` 读取路径**重新断言** `privateKey.extractable === false`、`type === "private"`、`algorithm.name === "ECDSA"`；任一不符抛 `corrupt_entry`（fail closed）。即使有人手工往 IndexedDB 塞一个可导出私钥，也不会被当成设备身份。
4. `signDeviceProof()` 是私钥的**唯一**使用点，返回 64 字节 P1363；端口不提供导出方法（与 `docs/IDENTITY_AND_AUTH_CONTRACT.md` §7「私钥不出端口」同口径）。
5. 来源绑定：`ensureIdentity` 在已存在身份且 `canonicalOrigin` 不同时抛 `origin_mismatch`（R11 场景 3）；`clearIdentity()` 后 `loadIdentity()` 返回 `null`、`signDeviceProof` 抛 `identity_missing`（R11 场景 4 / R20 场景 4）。

### 可核对证据（两种环境）

**（a）真实 Chromium（最强证据）** —— 真实 WebCrypto + 真实 IndexedDB 结构化克隆：

| 断言 | 结果 |
| --- | --- |
| 私钥句柄 `extractable === false` | PASS（`extractable=false`） |
| `exportKey("pkcs8", privateKey)` 抛错 | PASS（`InvalidAccessError`） |
| `exportKey("jwk", privateKey)` 抛错 | PASS |
| 公钥是 65 字节 SEC1 未压缩点（`0x04` 前缀） | PASS |
| **真实 IndexedDB 往返后 `extractable` 仍为 `false`** | PASS（`extractable=false`） |
| 落盘记录不含私钥字节（无 JWK `d` / `pkcs8`） | PASS（记录数 1） |
| 签名为 64 字节 P1363 | PASS |
| 签名可被设备公钥验证 | PASS（`verify=true`） |

**（b）Node 侧常驻测试**（`secure-storage.test.ts`，7 项，含 IndexedDB 替身）：同样的不可导出断言，外加「被篡改为可导出私钥的记录被拒绝」（`kind: "corrupt_entry"`）与来源变化拒绝。

> 边界如实登记：Node 22.22.0 **没有**内置 `indexedDB`，Node 侧用的是最小替身（`testing/fake-indexeddb.ts`），它按引用保存值、**不**声称结构化克隆保真；浏览器语义由上面 (a) 的 Chromium 结果承担。

---

## 2. 按 transcript 规范实现编码 —— 字节级保真

### 实现与依据

`transcript.ts` 实现 `acpr-transcript-v1`：

```
magic(4 ASCII "ACPR") || codecVersion(u8=1) || domainLength(u16be) || domain(UTF-8)
  || fieldCount(u16be) || 逐字段 { fieldTag(u16be) || byteLength(u32be) || rawBytes }
```

字段表**不复制**：`transcript.test.ts` 从仓库根读 `compatibility/transcripts/v1/transcripts.json`，按 `domain` 查表后逐字段编码。规则逐条对齐 `docs/SYNC_PROTOCOL.md` §6.2/§6.3 与 Rust 侧 `crates/acpr-transcript`：tag 严格递增且唯一（重复先于乱序判定）、字符串不带 NUL、UUID 取 16 原始字节、`u16be`/`u64be` 大端、公钥/nonce 用原始字节、`negotiatedFeatures` 排序后单 `0x00` 连接无尾随 NUL。

### 与固定向量的比对（可核对）

- **读取方式**：`transcript.test.ts` 经 `src/protocol/paths.ts` 的 `repoRoot` 解析仓库根，`readFileSync(join(repoRoot, "fixtures","sync","v1","transcripts","device-proof.json"))`；**不复制**到 `clients/app/` 之下。
- **编码条目**：`domain = "acp-remote/device-proof/v1"`，字段 tag `1,2,3,6,9,10,11,12`（8 个字段），输入取该 fixture 的 `input`。
- **比对结果（Node）**：
  - `encodeBase64Url(encoded)` **等于** `expected.transcriptBase64url`
  - `SHA-256(encoded)` **等于** `expected.transcriptSha256Hex`（`20328088…03faff`）
  - 结构头逐字段核对：magic/版本/domain 长度/字段数/tag 递增/长度前缀/无尾随字节
- **比对结果（浏览器）**：同一实现在 Chromium 内对同一输入编码，`encodeBase64Url` 与 `expected.transcriptBase64url` 逐字节一致（长度 303）。**字节与向量完全一致，没有改向量迎合实现。**
- **避免只对一个向量调参**：额外用 `host-challenge.json` 的向量对同一实现做了逐字节核对（同字段集、不同 domain），同样一致。

---

## 3. R20 平台侧

### 实现

- `local-cache.ts`：固定常量 `SUMMARY_CACHE_MAX_BYTES = 8 MiB`、`SUMMARY_CACHE_TTL_MS = 30 天`、`IMPORTED_METADATA_MAX_BYTES = 2 MiB`、`IMPORTED_METADATA_TTL_MS = 7 天`；纯函数 `evictToFit()`（先丢过期、再按 `lastUsedAtMs` 由旧到新 LRU 淘汰到配额内）；`assertPersistableForOrigin()` 是 imported 正文落盘的**唯一**守门；`CacheEntry` 用顶层判别式 `kind: "local" | "imported"`，imported 分支在**类型上**没有 `summary` 字段——「imported 正文落盘」不可构造。
- `local-cache.web.ts`：IndexedDB 持久层只接受 `putLocalSummary`（摘要）与 `putImportedMetadata`（无正文元数据）；imported 正文只进 `createVolatileImportedContent()` 的内存 `Map`，刷新即失效、重连后必须重新获取。
- `lifecycle.web.ts`：`visibilitychange`/`pagehide`（+ `beforeunload` 兜底）收敛为 `LifecyclePort`；`requestPersistentStorage()` 调 `navigator.storage.persist()`，缺失/拒绝时返回 `false` 不抛错。

### 证据

Node 测试（`local-cache.test.ts` 9 项 + `lifecycle.test.ts` 4 项）：LRU 淘汰顺序、TTL 过期、持久层配额收敛、imported 元数据形状允许/正文拒绝、内存载体清除后消失、可见性映射与退订、无 DOM 时不崩溃。Chromium（真实 IndexedDB）：imported 正文 sentinel **未出现在持久层**；`clear()` 后内存载体为空。

---

## 4. 平台端口清单（WP6/WP7 的消费面）

`platform/index.ts` 的 `openPlatform(): PlatformPorts` 是唯一装配点：

| 端口 | 实现 | 供 WP6/WP7 使用 |
| --- | --- | --- |
| `DeviceIdentityPort`（load/ensure/sign/clear） | `secure-storage.web.ts` | 逐连接签名握手、配对页 |
| `LocalCachePort`（putLocalSummary/putImportedMetadata/get/clear/stats） | `local-cache.web.ts` | 快照摘要缓存、no-content-cache 分流 |
| `VolatileImportedContent`（put/get/drop/clear/keys） | `local-cache.web.ts` | imported 正文内存暂存 |
| `LifecyclePort`（subscribe/currentVisibility/requestPersistentStorage） | `lifecycle.web.ts` | 状态机的可见性/卸载接线 |

**留给 WP6/WP7 的边界**：WP6 负责把 `openPlatform()` 的端口注入 Sync Client 与状态机（去重、快照暂存原子替换、no-content-cache 分流、`revoked` 清理）；WP7 负责页面消费端口与视图模型。平台层**不**实现连接/命令状态机，也**不**实现任何页面。

**v1 只交付 Web（记录该裁定，而非靠「没人做」）**：本包**不写任何 `.native.ts`**。`tasks.md` 2.6 与 `docs/FRONTEND_DESIGN.md` §8.2 已裁定 v1 只交付 Web/PWA，原生端（Keychain/Keystore、原生 SQLite、相机、前后台生命周期）属后续变更与独立验收。端口接口按平台中立形状声明，正是为了让将来那份实现无需改动上层。

---

## 5. 顺带核实：`PLATFORM_API_SPECIFIERS` 白名单覆盖

`src/layers.test.ts`（WP5a 交付面，**未自行修改**）的白名单按 **import 说明符**匹配：`ws`、`socket.io-client`、`idb`、`localforage`、`indexeddb`、`local-cache`、`local-storage`、`expo-secure-store`、`secure-store`、`secure-storage`、`expo-sqlite`、`sqlite`、`sqlite3` + `node:*` 前缀。

**核对结论 —— 存在一处覆盖缺口，如实登记**：本包实际使用的能力是**全局对象/浏览器 API**（`indexedDB`、`crypto.subtle`/`CryptoKey`、`document.visibilityState`、`window` 事件、`navigator.storage`、`btoa`/`atob`），而不是可 import 的包名。白名单能拦住 `import "idb"`、`import "indexeddb"` 这类**包**写法，但**拦不住**页面直接写 `indexedDB.open(...)` 或 `crypto.subtle.generateKey(...)`——它们没有 import 说明符可匹配。本包**没有**违反该约束（所有平台访问都收敛在 `src/platform/`），但 WP5a 的禁令对「直接调用浏览器全局」这一类写法是**空断言**。修法（不在本包写入范围内，留给 WP5a/后续 WP 决定）：在 `layers.test.ts` 增加对 `app`/`components` 源文件的**标识符级**扫描（如 `indexedDB`、`crypto.subtle`、`localStorage`、`document.addEventListener`），与现有说明符扫描并列。

---

## 6. Checks（实际命令 / 目录 / 环境 / 退出码 / 子检查 / 日志）

| ID | 命令 | 工作目录 | 环境 | 退出码 | 日志 |
| --- | --- | --- | --- | --- | --- |
| **PV3** | `npx tsc --project tsconfig.json --noEmit && npx vitest run --config vitest.config.ts && npx expo export --platform web` | `.worktrees/wp5b/clients/app` | 独立 `clients/app/node_modules/`（`npm ci`，856 包）；Node v22.22.0 / npm 10.9.4；typescript 5.9.3 / vitest 3.2.7 / expo 54 | **0** | `reports/PV3-wp5b-r1.log` |

### PV3 子检查逐条

- `npx tsc --project tsconfig.json --noEmit` → **exit 0**（无诊断）
- `npx vitest run --config vitest.config.ts` → **exit 0**；7 个 test file、**77 passed / 0 failed**（`src/platform/*` 新增 27 项：transcript 7、secure-storage 7、local-cache 9、lifecycle 4；既有 `layers.test.ts` 9、`protocol/contract.test.ts` 22、`domain/domain.test.ts` 19 不变）
- `npx expo export --platform web` → **exit 0**；Metro 打包 684 modules，产出 `dist/index.html` + 1.03 MB bundle；bundle 扫描 `fileURLToPath`/`readFileSync` = **0**（平台层未把 `node:*` 带入 web bundle；尚未被路由引用，故也不引入新的运行期面）

### 浏览器子检查（R11 的决定性证据）

- 用 esbuild 把 `src/platform/testing/browser-checks.ts` 打成自包含 IIFE bundle（产物与入口均写在系统临时目录，**不进入仓库**），经 `http://127.0.0.1:4173` 在真实 Chromium 中运行 `window.runPlatformBrowserChecks()`。
- **11/11 全部 PASS**（清单见 §1(a)/§2/§3）；原始结果 JSON 存于系统临时目录 `wp5b-browser/browser-evidence.json`。

---

## 7. 未验证内容

- **未执行** E2E 与页面级浏览器用例（TP3/TP4 范围）；`expo export` 只证明构建可通过，**不证明**真实浏览器中的渲染、路由与运行期行为。
- **未验证** 与 Rust `acpr-transcript` 的**跨语言**对拍本身——按 plan，那由 **TP3（MU3e）** 执行；本包只交实现并自证与固定向量逐字节一致（Node + Chromium 两处）。
- **未验证** Sync Client/状态机/命令终态/事件去重（WP6）、页面与 feature 交互（WP7）、配对 HTTP 端到端流程（`PairingClaimRequest.proof` 的 HMAC 与 `clientNonce` 生成属 WP6/WP7 接线）。
- **未验证** imported 会话的端到端离线行为（来源离线标记、`resource.remote_unavailable` 处理）——平台层只提供端口与内存载体，分流逻辑归 WP6。
- **未判** 独立 review、合入、最终验收；`deps`/`advisories`/`secrets` 三个 CI-only job 本地无等价物，**未运行**。
- Node 侧 IndexedDB 是**最小替身**、非 IndexedDB 完整实现；真实结构化克隆语义只由 Chromium 子检查覆盖（已如实登记）。
- 浏览器子检查覆盖 Chrome 系；**未**在 iOS Safari / Android Chrome 上复跑（`docs/FRONTEND_DESIGN.md` §4.5 的跨浏览器验收属更后阶段）。
- `PLATFORM_API_SPECIFIERS` 对「直接调用浏览器全局」的覆盖缺口已登记（§5），本包**未**修改该文件。

---

```agentic-handoff
version: 1
agent_context:
  agent_id: "coder-w5b-r1"
  isolation: "fork_turns=none（新实现实例接管 .worktrees/wp5b 的 feat/wp5b；未继承任何既有对话，只携带起点提交 a4440d6、主 spec/plan/tasks 与派发说明）"
handoff_index:
  - task_id: "2.6"
    work_package: WP5b
    role: coder
    phase: implement
    round: 1
    stage: work-package
    attempt: 1
    target_revision: "13e6dbbedbae4e32196c994391f73acc95ba323f"
    evidence_type: DELIVERY
    evidence_id: deliver-wp5b-r1
    report_path: "reports/deliver-wp5b-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 .worktrees/wp5b（feat/wp5b，base=a4440d6）新建 clients/app/src/platform/ 共 14 文件（+2359/-0，全部在写入范围内）。R11：secure-storage.web.ts 用 WebCrypto 生成 extractable:false 的 P-256 密钥对并持久化到 IndexedDB 的结构化克隆存储，只落 CryptoKey 对象与公开材料；loadIdentity 重新断言 extractable===false，可导出即 corrupt_entry 失败关闭；signDeviceProof 是私钥唯一使用点，端口不导出私钥。可核对证据：真实 Chromium 中 exportKey(pkcs8/jwk) 抛 InvalidAccessError、真实 IndexedDB 往返后 extractable 仍为 false、落盘记录不含 JWK d/pkcs8，共 11/11 PASS；Node 侧另有余 7 项常驻测试（含篡改为可导出私钥被拒绝）。R11/AC3：transcript.ts 按 compatibility/transcripts/v1/transcripts.json 的表实现 acpr-transcript-v1，对 fixtures/sync/v1/transcripts/device-proof.json 的 input 编码后 base64url 与 SHA-256 与向量逐字节一致（Node 与 Chromium 两处），并用 host-challenge.json 另做一向量避免只对单个向量调参。R20：local-cache.ts 固定 8 MiB/30 天/TTL+LRU 纯策略与 imported 正文落盘守门，local-cache.web.ts 只持久化无正文元数据、imported 正文仅内存；lifecycle.web.ts 提供 visibility/pagehide 与 navigator.storage.persist 端口。v1 只交付 Web，不写 .native.ts。已登记 PLATFORM_API_SPECIFIERS 对浏览器全局调用（indexedDB/crypto.subtle）的覆盖缺口（未修该文件，属 WP5a 面）。"
    source_evidence: "openspec/changes/sync-scope-and-pwa-client/plan.md（WP5b 行）；tasks.md 2.6"
  - task_id: "3.11"
    work_package: WP5b
    role: coder
    phase: implement
    round: 1
    stage: work-package
    attempt: 1
    target_revision: "13e6dbbedbae4e32196c994391f73acc95ba323f"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/deliver-wp5b-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 13e6dbb 上的 clients/app 自身入口（cwd=.worktrees/wp5b/clients/app，独立 clients/app/node_modules（npm ci 856 包），Node v22.22.0 / npm 10.9.4，typescript 5.9.3 / vitest 3.2.7 / expo 54）三段均 exit 0：npx tsc --project tsconfig.json --noEmit 无诊断；npx vitest run --config vitest.config.ts 为 7 个 test file、77 passed / 0 failed（新增 src/platform/* 27 项：transcript 7、secure-storage 7、local-cache 9、lifecycle 4；既有 layers 9 / contract 22 / domain 19 不变）；npx expo export --platform web 由 Metro 打包 684 modules 并产出 dist/index.html 与 1.03 MB bundle，bundle 中 fileURLToPath/readFileSync 计数为 0。前端不接入根 npm run check（plan 裁决）。"
    source_evidence: reports/PV3-wp5b-r1.log
checks:
  - id: PV3
    work_package: WP5b
    command: "npx tsc --project tsconfig.json --noEmit && npx vitest run --config vitest.config.ts && npx expo export --platform web (cwd=.worktrees/wp5b/clients/app)"
    scope: "前端自身入口：类型检查 + 单元/契约测试 + Web 构建（plan 裁决不接入根 npm run check）；另有真实 Chromium 的 R11/R20 子检查 11/11 PASS"
    environment: "cwd=.worktrees/wp5b/clients/app；独立 clients/app/node_modules（npm ci）；Node v22.22.0 / npm 10.9.4；typescript 5.9.3 / vitest 3.2.7 / expo 54；浏览器子检查用真实 Chromium + 真实 IndexedDB/WebCrypto"
    exit_code: 0
    log_path: reports/PV3-wp5b-r1.log
    result: PASS
test_delivery:
  WP5b:
    kind: automated
    artifacts:
      - reports/PV3-wp5b-r1.log
    basic_checks:
      - PV3
```

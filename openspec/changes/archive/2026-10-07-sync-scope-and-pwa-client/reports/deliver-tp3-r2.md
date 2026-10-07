# TP3 交付报告（r2）

- **分支**：`feat/tp3`
- **基线**：`f6ea252`（MU3d）
- **r1 提交**：`16ef844`（`test(frontend): TP3 固定向量跨语言对拍与契约/状态机覆盖`）
- **r2 提交**：见文末
- **被检视对象**：`reports/review-tp3-r1.md`（独立只读检视，PASS 带 2 MAJOR / 3 MINOR / 2 SUGGESTION）
- **Write Scope**：`clients/app/**/*.test.ts`（已核实，见 §6）
- **验证日志**：`reports/PV3-tp3-r2.log`

---

## 1. 结论

review 的 2 条 MAJOR **全部修复**，并各自给出「改坏实现 → 变红」的实际数据（§3、§4）。
5 条次要项逐条处置：**3 条修**（MINOR-1、MINOR-2、SUGGESTION-2）、**2 条接受**（MINOR-3、SUGGESTION-1，理由见 §5）。

两条 MAJOR 的性质都是「声称覆盖得比实际更多」——r1 报告与用例注释写了「断言了目标状态」「imported 即使在线也不可标记」，实际断言的不是那件事。r2 的处置是把断言补成注释所声称的那件事，并把两处缺口从「未登记的已知缺口」变成「有判别力的用例」。

| finding | 级别 | 处置 | 「修复前」实测 | 「修复后」实测 |
| --- | --- | --- | --- | --- |
| MAJOR-1 R12 目标态零断言 | MAJOR | **修**：补手写目标态表并逐条断言 | 变异 M1/M2 下全仓 451 条全绿 | M1 → 1 红（7 条差异）；M2 → 1 红（5 条差异） |
| MAJOR-2 R20 断言孤儿函数 | MAJOR | **修**：新增经 `ClientStore` 的生产路径用例 | 变异 M3 下全仓 451 条全绿 | M3 → 2 红（新文件 2/2） |
| MINOR-1 变异 #9/#10 归因写错 | MINOR | **修**：补「去重键不是序号」用例 + 订正归因 | #9 下新增 r14 文件 7/7 全绿 | M4 → 新用例 1 红（其余 7 绿） |
| MINOR-2 `sortedFeatures` 只断言输入自洽 | MINOR | **修**：补「乱序 features 喂进握手仍验签通过」 | 去掉生产排序后全仓 451 条全绿 | M5 / M6 → 各 1 红 |
| MINOR-3 worktree 内 `npm run check` exit 1 | MINOR | **接受**（环境限制，非覆盖缺口） | — | — |
| SUGGESTION-1 三条未验证项定性 | SUGGESTION | **接受**（复核后同意「不阻塞」） | — | — |
| SUGGESTION-2 `withheld` 参数从未被使用 | SUGGESTION | **修**：漏投场景改用同一模型，删掉重复实现 | — | M7 → 3 红（判别力保留） |

---

## 2. 用例数变化

| | 测试文件 | 用例数 |
| --- | --- | --- |
| r1（`16ef844`） | 36 | 451 |
| r2（本提交） | **37** | **455** |
| 净增 | +1（`features/r20-input-mark-production-path.test.ts`） | +4（新文件 2 + r11 1 + r14 1） |

无删除、无跳过标记、未弱化任何既有断言；`r20-no-content-cache.test.ts` 的既有两条 `mayMarkInputAsSent` 断言**原样保留**，只补了一段注明「它不覆盖生产路径」的注释。

---

## 3. MAJOR-1：R12 的目标态列（手写，不从实现反推）

### 3.1 改了什么

`clients/app/src/state/r12-connection-transitions.test.ts`：

- 新增**手写目标态表**（与 `EXPECTED_SOURCES` 并列的第二列）：
  - `EXPECTED_TARGET`：8 个参数无关事件的字面量表（含 reviewer 点名的两条：`disconnect_requested: "disconnected"`、`identity_lost: "unpaired"`）；
  - `EXPECTED_CONNECT_TARGET` / `EXPECTED_CLOSED_TARGET` / `TARGET_BY_CAUSE`：三个参数相关事件（`connect_started` 是否来自退避、`connection_closed` 是否可重试、`blocked` 的阻断原因）的分支表；
  - `expectedTargetOf(event)` 把四张表按参数展开成「一条事件 → 期望目标态」。
- `:240` 的用例从「每条合法组合不抛错」改为「每条合法组合不抛错**且落到 `expectedTargetOf(event)`**」，差异逐条收集进 `wrongTarget` 后断言为空。
- 文件头「## 判别力」段改写为与实现一致的措辞（原措辞「每条合法组合都同时断言目标状态与阻断载荷」在 r1 时并不成立）。

**期望值的独立性**：本文件**不 import** `ALLOWED_SOURCES`（只 import 公开的 `allowedSourceStates()`），目标态表是字面量常量，`TARGET_BY_CAUSE` 与既有 `CAUSE_BY_STATE` 是两张方向相反的手写表。因此把实现的迁移分支改掉不会连带把期望值改绿。

### 3.2 「改坏目标态 → 变红」的实际数据（日志 §M1 / §M2）

| 变异 | 修改 | r12 文件 | 全仓 |
| --- | --- | --- | --- |
| **M1** | `connection-machine.ts` `disconnect_requested` 目标 `disconnected` → `unpaired` | **1 failed / 17 passed** | 1 failed / 450 passed (451) |
| **M2** | `connection-machine.ts` `identity_lost` 目标 `unpaired` → `disconnected` | **1 failed / 17 passed** | 2 failed / 449 passed (451) |

失败信息逐条列出差异，例如 M1：

```
+ "unpaired --disconnect_requested--> 期望 disconnected 实际 unpaired"
+ "disconnected --disconnect_requested--> 期望 disconnected 实际 unpaired"
+ "connecting --disconnect_requested--> 期望 disconnected 实际 unpaired"
…（共 7 条，覆盖 7 个合法来源）
```

M2：

```
+ "disconnected --identity_lost--> 期望 unpaired 实际 disconnected"
+ "online --identity_lost--> 期望 unpaired 实际 disconnected"
+ "reconnecting --identity_lost--> 期望 unpaired 实际 disconnected"
+ "revoked --identity_lost--> 期望 unpaired 实际 disconnected"
+ "identity_changed --identity_lost--> 期望 unpaired 实际 disconnected"
```

对照 review 的实测：M1 在 r1 时**全仓 451 条全绿**、M2 在 r1 时**新增 r12 文件 18/18 全绿**。两条现在都变红，且红法干净（失败信息是「期望 X 实际 Y」，不是崩溃或超时）。

还原后：`git checkout --` → `git status --porcelain` 为空，r12 文件 18/18 全绿。

---

## 4. MAJOR-2：R20 改走 `ClientStore` 生产路径

### 4.1 改了什么

- 新增 `clients/app/src/features/r20-input-mark-production-path.test.ts`（2 条用例）：
  - 用 `storeWith()` 喂入快照，再走**完整连接路径**（`pairSucceeded()` → `authenticating` → `replaying` → `online`）把连接推到 `online`，并**先断言** `store.state.connection.state === "online"`——否则「不可标记」会因连接未在线而平凡成立；
  - 断言 `store.conversationPage(REMOTE).capabilities.canSend === false`（来源节点在线与离线各一条）；
  - 对照：同一连接状态下本节点来源（`local`）必须 `canSend === true`，排除「组件永远不出可发送」的平凡通过。
- `clients/app/src/platform/r20-no-content-cache.test.ts`：既有两条 `mayMarkInputAsSent` 断言保留，补注释说明该导出全仓**只有定义与 re-export、无生产调用方**，因此不覆盖生产路径，并把读者指向新文件。

### 4.2 「改松生产判据 → 变红」的实际数据（日志 §M3）

变异：`features/conversation-model.ts:147`

```diff
-  const sendable = input.connectionOnline && open && input.origin === "local";
+  const sendable = input.connectionOnline && open && (input.origin === "local" || input.origin.kind === "remote");
```

（即 review 用的那一处变异：生产路径上 imported 会话被允许标记已发送。）

| | 结果 |
| --- | --- |
| 新文件 `src/features/r20-input-mark-production-path.test.ts` | **2 failed / 0 passed** |
| 全仓 | **2 failed / 453 passed (455)** |
| r1 时全仓（review 实测） | 451 passed（无一条红） |

失败信息：

```
- Expected
+ Received
- false
+ true
 ❯ src/features/r20-input-mark-production-path.test.ts:90
```

即：新用例真的走到了 `conversationCapabilities` 的 `sendable` 判据。

---

## 5. 次要项逐条处置

### MINOR-1（**修**）：变异 #9 / #10 的捕获者归因写错

- 归因订正（r1 报告 §2 表格第 9、10 行）：

  | # | 变异 | r1 报告写的捕获者 | 实际捕获者 |
  | --- | --- | --- | --- |
  | 9 | `dedupe.ts` 去重键 `eventId` → `globalSequence` | 「r14 的三条 eventId 去重用例」 | 既有 `src/sync-client/dedupe.test.ts`；r1 新增的 `r14-replay-recovery.test.ts` 当时 7/7 全绿 |
  | 10 | `snapshot.ts` `begin()` 不再 `discard("superseded_by_new_snapshot")` | 「由 #13 一并覆盖」 | 既有 `src/sync-client/snapshot.test.ts`；r1 新增的 `r14-rebuild-snapshot.test.ts` 当时 3/3 全绿 |

- 归因虽写错，但 reviewer 认定的「无覆盖缺口」只对**一半**成立：#9b（换 `messageId` 仍判重复）已被 r1 新增用例抓住，而「去重键不是**序号**」这一半当时只有既有 `dedupe.test.ts` 守着。为把这一半也收进本测试包，在 `r14-replay-recovery.test.ts` 补一条：

  > 同序号但 `eventId` 不同判 `late`（补齐呈现）而非 `duplicate`（去重键不是序号）

- 判别力实测（日志 §M4）：变异 #9 下，新文件 **1 failed / 7 passed**，红的正是新补的这条（`等待条件未成立：检出丢失区间`）；其余 7 条仍绿——说明这条是唯一抓住该变异的用例，补得有必要。

### MINOR-2（**修**）：`sortedFeatures` 的断言是输入数据自洽性

- reviewer 判定「可不动」，理由是生产侧由 `encodeNulJoinedUtf8` 的 `field_order` 硬校验兜底。**但「有硬校验」与「生产在编码前排序」是两件事**：删掉生产里的 `sortedFeatures` 只会让乱序输入抛错，而不会产出错字节，所以 r1 的 451 条全绿。
- r2 补一条**经握手生产路径**的用例（`r11-fixed-vectors.test.ts`）：把 host-challenge 向量里已升序的 `negotiatedFeatures` **反转**成乱序，原样放进 `auth.server_challenge.selectedFeatures`（wire 解码器不校验顺序，已核实 `wire.ts:590-600`），再用**向量自带**的 `p1363Signature` 验签——它签的是升序 features 的 transcript，因此乱序输入必须经过「编码前排序」才能验签通过。
- 判别力实测（日志 §M5 / §M6）：
  - M5：去掉 `composition.ts` 的 `sortedFeatures`（`verifyHostChallenge` 侧）→ **1 failed / 10 passed**，`TranscriptError: nul-joined-utf8 字段必须按 UTF-8 字节升序、无重复`（`field_order`）；
  - M6：去掉 `connection.ts` 的 `sortedFeatures`（device-proof 侧）→ **1 failed / 10 passed**，同一异常。
  - 两条覆盖两个不同的生产编码点，任一处回归都会红。
- 原有那条 `expect(sortedFeatures(features)).toEqual([...features])` 保留，但注释改为如实说明「它断言的是向量自带的 features 已有序（输入数据的性质），生产排序由新用例覆盖」。

### MINOR-3（**接受**）：worktree 内 `npm run check` exit 1

- 复核：前 9 道门禁全绿，第 10 道 `check:agentic` 输出「缺少 `node_modules/@dongfanglin/openspec-agentic/bin/openspec-agentic.mjs`，请先在仓库根运行 `npm ci`」退出 1（日志 §4）。worktree 根确无 `node_modules`，而共同约束明确禁止在仓库根跑 `npm ci`/`npm install`。
- **接受，不改**：本包只新增/修改 `clients/app/**/*.test.ts`，十道根门禁均不检视该路径；`git diff --name-only f6ea252..HEAD` 的 9 条路径全部是 `.test.ts`。这是环境限制，不是覆盖缺口。

### SUGGESTION-1（**接受**）：三条未验证项的定性

复核后同意 reviewer 的逐条判定，全部**不阻塞**：真实浏览器内的私钥结构化克隆保真属浏览器内核语义（与 TP4 边界重叠，本包未新增浏览器断言，浏览器侧证据仍 16/16 且属 TP4）；`harness.ts` 的 `waitUntil` 只轮转微任务是测试基础设施的既有局限（本包三处自带宏任务等待器，不受影响）；`composition.ts` 固定 `resumeCursor: null` 不在 R14 的 MUST 文本内。

reviewer 另指出「唯一需要补充的未验证项」是 MAJOR-1 与 MAJOR-2 揭示的两处缺口——这两处在 r2 已**从缺口变为有用例守住**，不再需要登记为未验证项。

### SUGGESTION-2（**修**）：`attachReplayServer` 的 `withheld` 参数从未被使用

- r1 里 `withheld` / `withheldUsed` 逻辑齐备，但两处调用都传 `{}`，真正测「服务端漏投」的是另写的一份 `socket.send` 覆写——读者会以为两个场景共用同一模型。
- r2 把漏投场景改为 `attachReplayServer(context, { withheld: new Set(["7"]) })`，删掉那份重复实现；`withheld` 现在**真的被使用**（服务端明知 7 可见却在首轮重放不投），两个场景确实落在同一条 §9.3 模型上，`subscribeCount()` 取代了局部计数器。
- 判别力保留实测（日志 §M7）：把 `dedupe.ts` 的 `late` 判成 `duplicate`（即「低于水位却未呈现的事件被按重复丢弃」）→ 新文件 **3 failed / 5 passed**，其中包含这条改造后的漏投用例（`等待条件未成立：检出丢失区间`）。改造没有削弱它。

---

## 6. 变异验证表（r2 新增 7 项）

方法：临时改坏实现 → 跑对应文件（M3 另跑全量对照）→ 记录红/绿 → **立即 `git checkout --` 还原**。日志 `PV3-tp3-r2.log` 逐项附 `git diff` 与原始输出。还原后 `git status --porcelain` 为空，`git diff` 对实现文件为 0 行。

| # | 变异内容 | 目标文件 | 实测 | 捕获者 |
| --- | --- | --- | --- | --- |
| M1 | `disconnect_requested` 目标 `disconnected` → `unpaired` | `src/state/connection-machine.ts` | 🔴 r12 1 红 / 17 绿（全仓 1 红 / 450 绿） | r12「每条合法组合…落到手写期望表的目标态」 |
| M2 | `identity_lost` 目标 `unpaired` → `disconnected` | `src/state/connection-machine.ts` | 🔴 r12 1 红 / 17 绿（全仓 2 红 / 449 绿） | 同上 |
| M3 | `sendable` 放宽为「local 或 remote」 | `src/features/conversation-model.ts` | 🔴 新文件 2 红 / 0 绿（全仓 2 红 / 453 绿） | `r20-input-mark-production-path` 两条 |
| M4 | 去重键 `eventId` → `globalSequence` | `src/sync-client/dedupe.ts` | 🔴 r14 1 红 / 7 绿 | 新补的「同序号不同 eventId 判 late」 |
| M5 | 去掉 `verifyHostChallenge` 编码前的 `sortedFeatures` | `src/composition.ts` | 🔴 r11 1 红 / 10 绿（`field_order`） | 新补的「features 乱序…仍验签通过」 |
| M6 | 去掉 device-proof 编码前的 `sortedFeatures` | `src/sync-client/connection.ts` | 🔴 r11 1 红 / 10 绿（`field_order`） | 同上 |
| M7 | `late` 判成 `duplicate` | `src/sync-client/dedupe.ts` | 🔴 r14 3 红 / 5 绿 | S2 改造后的漏投用例 + 缺口上限用例 + M4 的新用例 |

**r1 变异表的归因订正**：第 9、10 行的捕获者改为既有文件（见 MINOR-1）。

---

## 7. Write Scope 与交付物核实

```
$ git diff --name-only f6ea252..HEAD
clients/app/src/features/r20-input-mark-production-path.test.ts   ← r2 新增
clients/app/src/platform/r20-no-content-cache.test.ts             ← r2 修改（仅补注释）
clients/app/src/r11-fixed-vectors.test.ts                         ← r2 修改
clients/app/src/r14-rebuild-snapshot.test.ts                      ← r1
clients/app/src/r17-r19-store-contract.test.ts                    ← r1
clients/app/src/state/r12-connection-transitions.test.ts          ← r2 修改
clients/app/src/state/r13-command-idempotency.test.ts             ← r1
clients/app/src/sync-client/r13-dispatch-idempotency.test.ts      ← r1
clients/app/src/sync-client/r14-replay-recovery.test.ts           ← r2 修改
```

- **9/9 路径以 `.test.ts` 结尾**；未改动任何实现文件、`app/**`、`scripts/**`、`package.json`、`vitest.config.ts`。
- `git status --porcelain` **为空**（提交后复核）。
- 变异验证期间的实现文件改动**全部已还原**（`git diff` 对 `connection-machine.ts`、`conversation-model.ts`、`composition.ts`、`connection.ts`、`dedupe.ts` 均为 0 行）。

---

## 8. 门禁退出码

| 门禁 | 命令 | 退出码 | 结果 |
| --- | --- | --- | --- |
| 类型检查 | `npx tsc --noEmit`（`clients/app`） | **0** | 无诊断 |
| 单元测试 | `npx vitest run`（`clients/app`） | **0** | 37 files / **455 passed** |
| 浏览器断言 | `node scripts/run-browser-check.mjs`（未改动该脚本） | **0** | **16/16 passed** |
| 仓库门禁 | `npm run check`（worktree 根） | **1** | 前 9 道绿，第 10 道 `check:agentic` 因缺根 `node_modules` 未运行（见 §5 MINOR-3） |

---

## 9. 未验证项

1. **`npm run check` 的第十道门禁未执行**——环境限制（worktree 根无 `node_modules`，且约束禁止在仓库根 `npm ci`），非本包引入。需在具备根 `node_modules` 的环境复跑。**未以任何方式绕过。**
2. **真实浏览器内的私钥结构化克隆保真**未由本包验证——本包未新增浏览器断言，浏览器侧证据仍是既有的 16 条（属 TP4 范围）。
3. **`crates/**` Rust 侧未重新编译或跑测试**——本包只消费其冻结向量，未改动它们。
4. **真实 WebSocket / 真实浏览器内核中的 WebCrypto 行为**仍只由既有 16 条浏览器断言覆盖；本包全部对拍在 Node 22 的 WebCrypto 上完成（与浏览器同源实现，但不是同一进程）。
5. **CI-only job `deps` / `advisories` / `secrets` 未执行，不声称通过。**

---

## test_delivery / agentic-handoff（门禁凭据块）

```agentic-handoff
version: 1
agent_context:
  agent_id: "testing-3-r2"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "4.10"
    work_package: TP3
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "6e83070"
    evidence_type: DELIVERY
    evidence_id: tp3-delivery-r2
    report_path: "reports/deliver-tp3-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "固定交付提交 6e83070（完整 40 位 6e8307096629dcfd3fa63ac20559e25b0a3e7a7a，与 verification.md Handoff Index 该行 Target Revision 单元格同值；分支 feat/tp3，基线 f6ea252）承载本轮全部测试编写交付：9 个路径 / +3542 行，全部为 clients/app 下的 .test.ts（新增 clients/app/src/features/r20-input-mark-production-path.test.ts，修改 clients/app/src/{r11-fixed-vectors,r14-rebuild-snapshot,r17-r19-store-contract}.test.ts 与 clients/app/src/state/{r12-connection-transitions,r13-command-idempotency}.test.ts、clients/app/src/sync-client/{r13-dispatch-idempotency,r14-replay-recovery}.test.ts、clients/app/src/platform/r20-no-content-cache.test.ts），零实现文件改动；用例 451 到 455（37 个文件），PV3 已在同一提交上 exit 0。"
    source_evidence: NOT_APPLICABLE
  - task_id: "4.10"
    work_package: TP3
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "6e83070"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/deliver-tp3-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "PV3 在 target_revision 6e83070 上实测 exit 0：clients/app 内 npx tsc --noEmit 无诊断、npx vitest run 37 文件 / 455 用例全绿（含本轮净增 4 条）、node scripts/run-browser-check.mjs 浏览器断言 16/16 通过；命令以「clients/app 自身入口」记于本报告 §8。日志 reports/PV3-tp3-r2.log。"
    source_evidence: NOT_APPLICABLE
checks:
  - id: PV3
    work_package: TP3
    command: "npx tsc --noEmit && npx vitest run && node scripts/run-browser-check.mjs（cwd=clients/app，本报告 §8 的实测行）"
    scope: "工作包 Project Verify（前端契约与状态机）：类型检查、vitest 全量 37 文件 / 455 用例（含本包 9 个测试文件）与既有的浏览器断言脚本 16/16"
    environment: "Windows x64；Node v22.22.0；clients/app 独立 node_modules；浏览器断言经 node scripts/run-browser-check.mjs 在 Node 22 WebCrypto 上执行（未改动该脚本）"
    exit_code: 0
    log_path: reports/PV3-tp3-r2.log
    result: PASS
test_delivery:
  TP3:
    kind: automated
    artifacts:
      - reports/tp3/clients/app/src/features/r20-input-mark-production-path.test.ts
    basic_checks:
      - PV3
```

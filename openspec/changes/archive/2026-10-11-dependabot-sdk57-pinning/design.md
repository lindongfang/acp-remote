<!-- 记录技术方案：当前事实、取舍与理由、风险与缓解。动机与范围见 proposal.md。 -->

# Design

## Context

动机与范围见 proposal.md。本节只记录决定方案形状的**实测事实**与约束。

### 1. SDK 57 是稳定渠道，SDK 58 不是；`clients/app` 已与 SDK 57 逐项一致

SDK 57 / 58 各自的期望版本取自两个 SDK 版本自带的 `bundledNativeModules.json`
（分别从 `expo@57.0.27`、`expo@58.0.7` 的 npm tarball 内取出，不是推断）：

| 包 | `clients/app` 现状 | SDK 57 期望 | SDK 58 期望 |
| --- | --- | --- | --- |
| `expo` | `57.0.27` | `~57.0.0`（`latest` = 57.0.27） | `~58.0.7`（`next` 渠道） |
| `expo-router` | `57.0.25` | `~57.0.25` | `~58.0.17` |
| `expo-constants` | `57.0.21` | `~57.0.21` | `~58.0.10` |
| `expo-status-bar` | `57.0.1` | `~57.0.1` | `~58.0.3` |
| `react` / `react-dom` | `19.2.3` | `19.2.3` | `19.3.0` |
| `react-native` | `0.86.3` | `0.86.3` | `0.88.0-rc.4` |
| `react-native-web` | `~0.21.0`（解析到 0.21.3） | `~0.21.0` | `~0.21.4` |

三条支持「现在」「统一维持 57」的观测：

- SDK 57 线上这四个 expo 家族包的**最新版就是仓库当前装的版本**（`expo` 57.0.27、
  `expo-router` 57.0.25、`expo-constants` 57.0.21、`expo-status-bar` 57.0.1），
  SDK 57 线内没有待升级项；`expo` / `expo-router` / `expo-constants` / `expo-status-bar` /
  `expo-linking` 在 57 线上**都只有 `57.0.x`**（无 57.1+），所以关掉 major 后剩下的只会是补丁。
- SDK 58 未进入稳定渠道：`expo@latest` 仍是 57.0.27，58 挂 `next`；`expo@58.0.7` 自身依赖
  `@expo/metro@~58.0.0-rc.0`；SDK 58 认定的配套 `react-native` 是 `0.88.0-rc.4`。
- `expo-router@58.0.0` 的对等依赖要求 `expo-constants@^58.0.0`、`@expo/log-box@^58.0.0`、
  `@expo/metro-runtime@^58.0.0`、`expo-linking@^58.0.0`——缺的是**整代**，不是一个包。

### 2. `#76` 的失败位置与形状（决定了「改哪里」）

`前端检查（clients/app）` 的第一步 `npm ci` 报 `ERESOLVE`：`expo-constants@57.0.21` 同时被
根项目与 `expo@57.0.27` 要求（`~57.0.21`），而 `expo-router@58.0.0` 要求 `^58.0.0`。
失败发生在 8 秒内，`tsc` 与 458 个单测未执行。PR 分支的锁文件里，`expo-router` 位于顶层
`58.0.0`，`node_modules/expo-constants` 是 `57.0.21`，Dependabot 只把 `58.0.9` 嵌到了
`node_modules/expo-linking/node_modules/expo-constants`——即它自己写出的锁文件也不自洽。

### 3. 配置层的现状与约束

- `.github/dependabot.yml` 的 `/clients/app` 条目：`ignore` 目前只有一条
  （`react-native` + `version-update:semver-minor`），`groups` 只有 `clients-app-minor-patch`。
- **范围号不是边界**：提交 `b34ad17` 中 dependabot[bot] 把 `"typescript": "~6.0.3"` 改写成
  `"~7.0.2"`。默认 `versioning-strategy: auto` 允许它改写 manifest 的范围本身。
- **`ignore` 的 `update-types` 只作用于版本更新**，不看安全更新——本仓库上一变更的 D2 已确认该
  语义并按此给 `react-native` 设过规则，且实测 RN 的安全更新仍会照常开 PR。
- **`ignore` 的取值域写错不会报错，只会静默失效**（该文件自己的注释已警告这一点）。
- **本地没有任何门禁读取 `.github/**`**：`npm run check` 的十道脚本不读它。因此本变更的正确性
  无法由现有门禁证明，只能靠 YAML 解析断言 + 人工核对，加上一次 Dependabot 实际运行的观测。

## Goals / Non-Goals

**Goals:**

- 让「跨 SDK 的单个 expo 家族包升级」不再被生成成 PR——这是 #76 这类红的唯一来源。
- 让「能过 CI 但不在 SDK 57 期望集合内」的 `react` / `react-dom` minor 提议也不再生成，
  使 `clients/app` 的依赖集合在 SDK 57 期间稳定等于官方期望集合。
- 让该条目的注释只包含可核对的事实：读者应能从注释直接得到「为什么必须整族同批升」，
  而不是一条未被实测支持的结论。

**Non-Goals:**

- 不引入任何新的本地或 CI 判定面（不新增 job、不改 `npm run check`、不改脚本）。
- 不新增锁文件完整性断言：`npm ci` 的同步断言已经承担该判定（它就是
  `Missing: react-native-worklets@0.10.4 from lock file` 的报告者），上一变更的「本次不包含」
  已按同一理由定过这个取舍。
- 不改变 `clients/app` 现在**装**什么：`package.json` 与锁文件不动。
- 不处理 Dependabot 锁文件生成器缺陷（见 Risks 的最后一条）。

## Decisions

### D1：用 `ignore` + `version-update:semver-major` 表达「锁在一个 SDK 上」

expo 家族包的**主版本号即 SDK 号**（57/58/59…），所以对整族关闭 major 版本更新，
语义上精确等于「前端锁在某个 SDK」。补丁仍会提议（SDK 57 的维护性发布），安全更新不受影响。

**替代方案 A：`groups.exclude-patterns`**——只改分组归属，仍会单独成一个 PR。
上一变更已否决过同一思路（RN 单独升仍会稳定失败，等于每周产生一个注定红灯的 PR）。
**替代方案 B：靠 `"~57.0.0"` 范围号挡住**——实测无效（见 Context 第 3 节的 `b34ad17`）。
**替代方案 C：删掉 `/clients/app` 条目、彻底关闭版本更新**——过粗：连 SDK 57 的补丁也收不到，
而 `react` / `expo` 之外的工具链（`vite` / `vitest` / `esbuild` / `typescript`）本应继续收更新。

### D2：显式列出包名，不用通配符

四条 expo 规则与两条 react 规则都逐条写明 `dependency-name`，不使用 `expo*` 这类通配。
理由：该文件自己的注释已警告「取值域写错不会报错、只会静默失效」，逐条列出才能被 tasks 的
YAML 解析断言逐项比对；通配会把「匹配到什么」变成不可核对的行为。

### D3：`react` / `react-dom` 也关闭 `version-update:semver-minor`（用户在 A/B 之间选定 A）

`react@19.3.0` 是 minor，能过全部门禁（上一变更实测：RN 不动时 `npm ci` 通过、458 测试全过、
`expo export` 成功），但 SDK 57 的期望版本是 `19.2.3`，不在集合内。不挡它就会每周产生一个
「能合、但不被 `expo install --check` 认可」的 PR，把判断变成重复的人工劳动。

**代价（已知并接受）**：SDK 57 期间不再收到这两个包的 minor；补丁与安全更新不受影响
（`update-types` 只作用于版本更新）。SDK 58 稳定后整批升级时一并解除。

**不选 B（只挡跨 SDK）的理由**：proposal 的目标是「统一维持在 57」，而 19.3.0 正是那种
「能过门禁但不在期望集合内」的中间态；留着它，这条边界就永远靠人守。

### D4：注释按可核对的事实重写

被替换的旧注释（「实测它们的升级不影响判定面」）有两处不成立：其引用的实测表不包含
`expo-router`（对 `react` 的结论被外推），且同一变更新增的 `前端检查（clients/app）`
正是把这类失败变成红灯的判定者。新注释写三件可核对的事：

1. expo 家族的 major 号即 SDK 号，跨 SDK 必须整族同批升（附 `expo-router@58` 需要整套
   SDK 58 对等依赖这一事实）；
2. 范围号挡不住 Dependabot（引 `b34ad17` 的实测）；
3. **本清单是清单本身**：新增 expo 家族的**直接依赖**（写进 `clients/app/package.json` 的那些）
   必须同步补一条 major 规则，否则跨 SDK 提议会从新包重新出现。

### D5：只改一个文件

不碰 `clients/app` 的 manifest 与锁文件。依赖集合现在已与 SDK 57 期望一致，本变更只改变
「未来提议什么」。这样 PR 的改动面最小、可逐行核对，也不会引入任何需要重新生成锁文件的步骤。

## Risks / Trade-offs

- **`ignore` 的生效范围无法本地或 CI 验证** → 上游运行时行为，本地无法执行。缓解：把可验证的
  部分（规则存在、取值逐字正确、既有规则未被误改）做成 YAML 解析断言；运行时效果由下一次
  Dependabot 运行观测（proposal 的成功判据第 1 条）。
- **`update-types` 语义静默失效**（写错不报错） → 由上述断言逐条比对取值，而不是只检查键存在。
- **expo 家族清单可能不完整** → 未来若把 `expo-linking`、`@expo/metro-runtime` 等提为直接依赖，
  跨 SDK 提议会从新包重新出现。缓解：D4 第 3 点把这条要求写进注释本体，使它随文件一起被审阅。
- **冻结 `react` / `react-dom` 的 minor 会推迟拿到新特性** → 已知代价，D3 明确接受；
  SDK 58 稳定后整批升级时解除。
- **同类红灯仍会由 Dependabot 的锁文件生成器缺陷产生**（`Missing: react-native-worklets@0.10.4
  from lock file`，已命中 #68 / #69 / #74）→ 与本策略无关，本变更不处理：它源自 Dependabot 重写
  `clients/app/package-lock.json` 时丢失一条嵌套条目，修法是重生成锁文件（#69 的做法）。
  本变更只消除「跨 SDK 错配」这一类红。
- **本变更不改变任何已合入内容的行为**，因此不存在运行时回退风险。

## Migration Plan

- **落地**：只改 `.github/dependabot.yml` 一个文件，随常规 PR 流程合入（`docs/WORKFLOW.md` §6.2）。
  无数据迁移、无 schema 变更、无依赖变更。
- **回滚**：删除新增的 6 条 `ignore` 规则（或只删其中一类）。回滚是纯配置操作，下一次
  Dependabot 运行即回到改动前行为。
- **运营跟进（仓库外）**：关闭 #76（内容与「统一维持 57」相反）；关闭 #74（按 D3 的选择，
  `react` 19.3.0 不采纳）。两者都不产生仓库改动。
- **观测**：合入后的下一次 Dependabot 运行若不再出现 expo 家族 major 提议与 `react` minor 提议，
  即确认规则生效；若仍出现，按 D2 的理由逐条核对 `dependency-name` 拼写与 `update-types` 取值。

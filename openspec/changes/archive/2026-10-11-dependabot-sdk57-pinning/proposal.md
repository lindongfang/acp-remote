<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

`.github/dependabot.yml` 的 `/clients/app` 条目对 Expo 家族的**跨 SDK 大版本升级没有设防**，
Dependabot 因此持续生成注定无法通过门禁的 PR。最新一例是 #76：单独把 `expo-router` 从
`57.0.25` 升到 `58.0.0`。

**这类 PR 在门禁的第一步就失败，且失败原因不出现在 PR 描述里。** `expo-router@58.0.0` 的
对等依赖要求 `expo-constants@^58.0.0`，而仓库顶层与 `expo@57.0.27` 都要求
`expo-constants@~57.0.21`：一个位置只能有一份，没有可满足的组合。`npm ci` 报 `ERESOLVE`
并以退出码 1 结束，`前端检查（clients/app）` 在 8 秒内判红——类型检查与 458 个单测根本没机会跑，
所以症状（8 秒失败）与原因（依赖错配）之间的因果只能靠人读日志还原。

**现在的位置是清楚的：SDK 57 是稳定渠道，SDK 58 不是。**

- `expo@latest` = `57.0.27`、`expo-router@latest` = `57.0.25`；SDK 58 只挂在 `next`
  标签上（`expo@next` = `58.0.7`），且 `expo@58.0.7` 自身依赖 `@expo/metro@~58.0.0-rc.0`。
- SDK 58 的 `bundledNativeModules.json` 把配套的 `react-native` 定为 `0.88.0-rc.4`——
  官方认定的配套 RN 还是候选版。
- `clients/app` 当前依赖**逐项等于** SDK 57 的期望集合（实测，见 design.md 的 Context 表），
  SDK 57 线内也没有任何待升级项。也就是说「统一维持 57」现在**零待办**，缺的只是「让 Dependabot
  不再提议跨 SDK 的单包升级」这一道边界。

**并且当前配置阻止不了它。** `package.json` 里的 `~57.0.0` 不是边界：本仓库已有先例
（提交 `b34ad17`，dependabot[bot] 把 `"typescript": "~6.0.3"` 改写成 `"~7.0.2"`），默认的
`versioning-strategy: auto` 允许它改写 manifest 的范围本身。只有 `ignore` 挡得住。

## What Changes

**一、`/clients/app` 条目新增 `ignore`：对 expo 家族关闭大版本更新**

对 `expo`、`expo-router`、`expo-constants`、`expo-status-bar` 四者关闭
`update-types: ["version-update:semver-major"]`。

这一族包的**主版本号就是 SDK 号**（57 = SDK 57，58 = SDK 58），所以这条规则的效果正是
「把前端锁在一个 SDK 上」。日常补丁仍会照常提议；安全更新不受影响（`update-types`
只作用于版本更新，不看安全更新——本仓库上一变更的 D2 已确认该语义，RN 那条规则即按此成立）。

**二、同一 `ignore` 再新增两条：把 `react` / `react-dom` 冻在 SDK 57 的期望版本**

SDK 57 期望的是 `19.2.3`，`19.3.0` 虽能过全部门禁（上一变更已实测：RN 不动、react 升 19.3.0 时
`npm ci` 通过、458 测试全过、`expo export` 成功），但它不在官方期望集合内，`expo install --check`
会提示。按「统一维持 57」的目标，这种「能合但不被官方认可」的中间态不应每周出现一次再靠人判断，
因此对这两者关闭 `version-update:semver-minor`。安全补丁同样不受影响。

**三、重写该条目里关于 expo 家族的注释**

现有注释写着「`react` / `expo` / `expo-router` 刻意不挡：实测它们的升级不影响判定面」，
这句话有两处问题：

1. 它引用的「实测」并不包含 `expo-router`——上一变更的 Context 实测表只有基线、RN、react、
   `vite`+`vitest` 四行，对 `react` 的结论被外推到了 `expo-router`。
2. 即使成立，判据也已在同一变更里被推翻：正是那个变更新增了 `前端检查（clients/app）`，
   而它的 `npm ci` 就是这次失败的判定者。

新注释改为可核对的机制说明：expo 家族的 major 号即 SDK 号，整族必须同批升级；跨 SDK 的单包
升级必然在 `npm ci` 的对等依赖校验阶段失败，`ignore` 是唯一的配置级防线（范围号挡不住）。

**本次不包含**

- 升级到 Expo SDK 58（`expo ~58` / RN 0.88 / `expo-router` 58 一整套）。SDK 58 当前在 `next`
  渠道，配套 RN 仍是 `0.88.0-rc.4`；该动作应在 RN 0.88 出正式版后按 #63 的先例做一次人工整批变更。
- 改动 `clients/app/package.json` 与 `package-lock.json`。现有依赖已与 SDK 57 期望集合一致，
  本变更只改变「未来提议什么」，不改变「现在装什么」。
- 修改 `npm run check` 的判定面、新增 CI job、或修改分支保护的必需检查。
- 新增锁文件完整性断言。同一取舍在上一变更已定过：`npm ci` 的同步断言已经承担该判定
  （它正是 `Missing: react-native-worklets@0.10.4 from lock file` 的报告者），本变更不重复
  引入一层断言。
- 用 `@dependabot ignore ...` PR 评论作为主要手段。配置文件的 `ignore` 是唯一出处，评论只影响
  单个 release（Dependabot 自己在 #58 下回复过「不会再就**这个 release** 提醒，但下个版本会再来」）。
- 关闭 #76 / #74 等 PR。那是仓库外的运营动作，见 design.md 的 Migration Plan。

## 成功判据

- Dependabot 下一次运行后，不再生成 expo 家族的 major 更新提议（含 `expo-router` 57→58 这类），
  也不再生成 `react` / `react-dom` 的 minor 提议。
- `/clients/app` 条目的 `ignore` 规则经 YAML 解析断言：四条 expo 规则为
  `version-update:semver-major`，两条 react 规则为 `version-update:semver-minor`；
  `react-native` 的既有规则逐字未变；其余三个 `updates` 条目逐字未变。
- `npm run check` 在仓库根全绿（本变更不改判定面，用于证明没有误伤）。
- 分支保护必需检查集合仍为六个，不变。

## 硬约束

- 只改 `.github/dependabot.yml` 一个文件；不改 `clients/app` 的 manifest 与锁文件。
- 不引入 `--legacy-peer-deps` / `--force` 之类的放宽——那会让门禁对依赖错配失明。
- `ignore` 的 `update-types` 取值域逐字为 `version-update:semver-major` /
  `version-update:semver-minor`；写错不会报错、只会静默失效（该风险由 tasks 的 YAML 解析断言覆盖）。

## Capabilities

本变更为**依赖维护策略与 CI 配置修正**：产品行为、协议 wire、端口合同、存储 DDL 均不变，
`openspec/specs/` 下既有能力规范的每一条 Requirement/Scenario 语义保持原样，
`clients/app` 的可观察行为契约（已冻结在 `pwa-web-client` 规范里）不被触碰——本变更只改变
「哪些依赖升级会被自动提议」，不改变任何已实现的客户端行为。

按变更流程 schema 的规则，在本变更的 `.openspec.yaml` 中设置 `skip_specs: true`，不生成增量规范文件。

### New Capabilities

无（无规范层面的行为变化，见上方说明）。

### Modified Capabilities

无（同上；既有规范的验证语义不因此改变）。

## Impact

- **配置**：`.github/dependabot.yml`（`/clients/app` 条目的 `ignore` 新增 6 条规则，并重写该条目
  的一段注释）。**不改变 job 集合与判定面。**
- **依赖 / 代码 / 合同资产**：无。`clients/app/package.json`、`package-lock.json`、
  `Cargo.lock`、`schemas/`、`fixtures/`、`compatibility/` 均不变。
- **权威文档**：本变更不改变产品、协议、安全或模块边界，`AGENTS.md` §9 的
  「变更类型 → 权威文档」映射中没有需要更新的条目。`README.md` 的「合同检查」与「仓库当前状态」
  描述的是 job 清单与已落地范围，本变更不改这两者。
- **运营跟进（仓库外，不属于本变更的代码改动）**：关闭 #76；按本变更的选择关闭 #74；
  在 `docs/WORKFLOW.md` §6.2 的既有 PR 流程下这些是常规动作。
- **风险**：`ignore` 的生效范围依赖 Dependabot 的运行时行为，本地与 CI 都无法执行验证；
  可验证的部分只有「规则写对且取值正确」，见 design.md 的 Risks。

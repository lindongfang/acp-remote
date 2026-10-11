<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

`clients/app` 的依赖自动升级目前**无法通过 CI**，而且失败方式会误导排查。

PR #68（Dependabot `clients-app-minor-patch` 分组）同时触发两个独立缺陷：

1. **锁文件不自洽**：`npm ci` 报 `Missing: react-native-worklets@0.10.4 from lock file`。
   `expo-modules-core@57.0.21` 有一条可选对等依赖要求 worklets `^0.7.4 || … || ^0.10.0`，
   而根上装的是 `0.13.0`（`react-native-reanimated@4.7.1` 要求 `0.13.x`），npm 必须在 expo 子树里
   再放一份 `0.10.4`。`main` 的锁文件有这条，PR 分支的锁文件没有。
2. **React Native 与 Expo SDK 错配**：PR 把 `react-native` 从 `0.86.3` 升到 `0.87.1`，
   但 `expo` 仍是 `57.0.27`。SDK 57 的 `@expo/metro-config@57.0.13` 硬编码
   `require(join(rnHostPath, 'rn-get-polyfills'))`，而 RN 0.87.1 删掉了 `exports` 里的 `"./*"` 通配
   与 `rn-get-polyfills.js` 文件，打包期报 `ERR_PACKAGE_PATH_NOT_EXPORTED`。
   `tsc` 与 458 个单测全过，**只有 `expo export` 会炸**。

两个缺陷互相遮蔽：`npm ci` 先失败，打包根本没机会跑到，所以问题 2 一直没暴露。

根因是**分组的粒度与上游发布节奏不符**：Expo SDK 与 React Native / React 是成套发布的，
RN 的 minor（`0.86` → `0.87`）实际对应 SDK 的 major（`57` → `58`）。Dependabot 只看 semver 数字，
把 RN 的 minor 当普通小版本单独升，就会装出一个能过类型检查、能过单测、但打不出包的组合。

同时，`ci.yml` 里那段说明的归因也不准确：它把「锁文件缺嵌套依赖」记在 #69 名下，而 #68 同时命中
该症状——`gh pr diff 68` 中 `react-native-worklets` 出现 2 次（都是删除该嵌套条目的行），#68 的
头部锁文件（`1ba6d08`）里 `node_modules/expo/node_modules/react-native-worklets` 为 0 条。
需要说明的是：#69 的**原始** head（`7c8a632`）同样缺这条嵌套条目、`npm ci` 一样失败，它是补齐
之后才变绿的。所以真正的问题不是「归错了 PR」，而是**漏记 #68 也命中锁文件缺陷**，并把 #68
描述成只有 RN/Expo 一个症状。这行注释会让下一次排查走向错误方向。

## What Changes

**一、`clients/app` 依赖升级收窄到安全集合**

把 PR #68 想升的六个包拆开：只保留 `vite`、`vitest`（开发工具，与 SDK 无耦合），
`react` / `react-dom` / `react-native` / `@types/react` 的升级从本次移除。
`react-native` 的升级必须与 Expo SDK 的 major 升级**同一批**进行，不能由 minor/patch 分组单独提出。

**二、`.github/dependabot.yml` 阻止同类分组再次提出 SDK 错配**

在 `/clients/app` 条目的 `groups:` 之前增加 `ignore`，对 `react-native` 关闭 minor 版本更新
（`update-types: ["version-update:semver-minor"]`）。`update-types` 只作用于版本更新，
安全更新不受影响，仍会照常创建。

**三、按实测重写 `ci.yml` 注释的两个症状归属**

把锁文件缺陷写成「**#68 与 #69 共有**的 Dependabot 锁生成器问题」，把
`ERR_PACKAGE_PATH_NOT_EXPORTED` 写成「#68（RN 0.87.1）独有」。原注释把前者记在 #69 名下并不算错，
真正的问题是**漏记 #68 也命中它**、并把 #68 描述成只有一个症状。

**本次不包含**：

- 升级到 Expo SDK 58（`expo ~58`、RN 0.88、`expo-router` 等一整套）。SDK 58 当前是 `next` 标签，
  不是稳定版；这是独立的人工变更，不属于本变更。
- 修改 `npm run check` 的判定面、新增 CI job 或修改分支保护的必需检查。
- 修改任何 crate、协议 wire、端口合同、存储 DDL。
- 引入锁文件重生成工具或 CI 断言（`npm ci` 已经承担该断言，本变更只是让锁文件重新自洽）。

## Capabilities

<!-- 先检查现有规范，区分新增能力与已有能力的需求变化。
 仅当规范层面的行为不变时（如不改变行为的纯重构、工具或文档变更），
 在该变更的 .openspec.yaml 中设置 skip_specs: true，说明理由并引用既有行为契约。
 此时两类能力清单均无变更，删除占位项；否则至少声明一项能力并生成对应增量规范。
 不得为通过校验而编造需求。 -->

本变更为**依赖维护与 CI 配置修正**：产品行为、协议 wire、端口合同、存储 DDL 均不变，
`openspec/specs/` 下 24 个既有能力规范的每一条 Requirement/Scenario 语义保持原样。
按变更流程 schema 的规则，在该变更的 `.openspec.yaml` 中设置 `skip_specs: true`，
不生成增量规范文件。

`clients/app` 的**可观察行为契约**（七层依赖方向、设备身份、连接状态机、命令终态、
目录与对话页呈现、no-content-cache 离线边界）已经冻结在 `pwa-web-client` 规范里，本变更不触碰它们；
`vite` / `vitest` 是构建与测试工具，不进入任何规范的行为面。

### New Capabilities

无（产品行为不变，见上方 skip_specs 说明）。

### Modified Capabilities

无（同上；既有规范的验证语义不因此改变）。

## Impact

- **依赖**：`clients/app/package.json` 的 `vite`（`^8.2.2` → `^8.3.2`）与 `vitest`（`^5.0.0` → `^5.0.3`）；
  `clients/app/package-lock.json` 随之重生成。**不新增任何依赖**。
  实测重生成会把 `vite` 解析到 `8.3.4`（`^8.3.2` 范围内的正常漂移），不是错误。
- **CI 配置**：`.github/dependabot.yml`（`/clients/app` 条目新增 `ignore`）、
  `.github/workflows/ci.yml`（注释归因修正）。两者都**不改变 job 集合与判定面**。
- **代码/合同资产**：无。`npm run check` 的十道脚本判定面不变。
- **权威文档**：本变更不改变任何产品、协议、安全或模块边界，因此 `AGENTS.md` §9 的
  「变更类型 → 权威文档」映射中**没有**需要更新的条目。`README.md` 的「合同检查」与
  「仓库当前状态」描述的是 job 清单与已落地范围，本变更不改这两者。
- **风险**：`ignore` 的生效范围依赖 Dependabot 的行为，本地无法执行验证（见 design.md 的 Risks）。

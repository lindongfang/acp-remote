# Design

## Context

`clients/app` 是独立的 npm 工程（自带 `package.json` / `package-lock.json`），CI 由
`前端检查（clients/app）` 一个 job 覆盖，跑两件事：`npm ci`（顺带断言锁文件与 `package.json` 同步）、
`npm run check`（typecheck + vitest + `expo export --platform web`）。

本设计基于以下**实测**结论（不是推断），它们决定了改动范围：

| 实验 | 组合 | 结果 |
|---|---|---|
| 基线（`main`） | RN `0.86.3` + react `19.2.3` + vite `8.2.2` + vitest `5.0.0` | `npm ci` OK，37 文件 / 458 测试全过，`expo export` OK |
| PR #68 原样 | RN `0.87.1` + react `19.3.0` + expo `57.0.27` | `npm ci` 失败（缺 worklets 嵌套条目）；补齐锁文件后 `expo export` 报 `ERR_PACKAGE_PATH_NOT_EXPORTED` |
| 只升 vite + vitest | RN `0.86.3` + react `19.2.3` + vite `8.3.4` + vitest `5.0.3` | `npm ci` OK，458 测试全过，`expo export` OK |
| react 升到 19.3.0，RN 不动 | RN `0.86.3` + react `19.3.0` | `npm ci` OK，458 测试全过，`expo export` OK |
| 只升 RN | RN `0.87.1` + react `19.2.3` | `npm ci` OK，**`expo export` 报 `ERR_PACKAGE_PATH_NOT_EXPORTED`** |

结论：**唯一硬卡点是 `react-native`**。`react` / `react-dom` / `@types/react` 的升级在本项目的
CI 判定面上不产生失败（虽然 `expo install --check` 会提示它们不是 SDK 57 的期望版本）。

另外两条事实：

- `expo-modules-core@57.0.21` 的可选对等依赖要求 worklets `^0.7.4 || … || ^0.10.0`，而根上是 `0.13.0`
  （由 `react-native-reanimated@4.7.1` 的 `0.13.x` 要求决定），因此 expo 子树里必须有一份 `0.10.4`。
  该条目在 `main` 锁文件里存在，在 PR 分支锁文件里缺失。
- `npm run check` 的十道脚本**不读取** `.github/**`：`.github/dependabot.yml` 与
  `.github/workflows/ci.yml` 在本地没有等价门禁，它们的正确性只能靠 YAML 解析与人工核对。

## Goals / Non-Goals

**Goals:**

- 让 `clients/app` 的依赖升级能通过六个必需检查，且升级内容只含与 Expo SDK 无耦合的包。
- 让 Dependabot 不再把 `react-native` 塞进 minor/patch 分组，从源头消除同类 SDK 错配。
- 修正 `ci.yml` 里对失败症状的错误归因，使下一次排查不走向错误方向。

**Non-Goals:**

- 升级到 Expo SDK 58（RN 0.88 / `expo ~58` / `expo-router` 等整套）。SDK 58 当前是 `next` 标签。
- 改变 `npm run check` 的判定面、job 集合或分支保护的必需检查。
- 为 `.github/**` 新增本地门禁（本变更不引入新的校验脚本）。
- 关闭或改写 PR #68 的历史（它由人在 GitHub 上处置，见 Migration Plan）。

## Decisions

### D1：只保留 `vite` / `vitest`，其余四个包的升级移出本变更

**选择**：`clients/app/package.json` 只把 `vite` 从 `^8.2.2` 提到 `^8.3.2`、`vitest` 从 `^5.0.0`
提到 `^5.0.3`；`react` / `react-dom` / `react-native` / `@types/react` 保持 `main` 原值。

**理由**：实测（Context 表）表明这四个包的升级对 CI 判定面没有影响，唯一失败来自 `react-native`。
但把它们**留到与 SDK major 同批**处理，可以避免在 SDK 57 期间维护一个 `expo install --check`
不认可的版本组合，也让「SDK 升级」保持为一个可审计的整批动作。

**替代方案**：全部保留、只修锁文件。被否决——RN 0.87.1 会稳定失败，PR 永远不能合。

### D2：`ignore` 只针对 `react-native`，且只挡 minor 版本更新

```yaml
    ignore:
      - dependency-name: "react-native"
        update-types: ["version-update:semver-minor"]
```

**理由**：

- `react-native` 是 `0.x`，按 SemVer 它**所有**的 `0.86 → 0.87` 类升级都是 minor。挡住 minor
  等于让 Dependabot 不再单独提 RN 的版本升级——这正是我们要的，因为 RN 的升级必须与 Expo SDK 的
  major 同批进行。
- `update-types` **只作用于版本更新**（官方文档明写 "only affects *version* updates, not *security*
  updates"）。因此 RN 的安全更新仍会照常创建 PR，不会因为这条 `ignore` 而被漏掉。
- 只挡 `react-native`：实测 `react` 19.3.0 与 RN 0.86.3 共存时全部检查通过，没有理由连它一起挡。
  过度拦截会无谓地停掉 react 系的更新。

**替代方案 A**：`groups.exclude-patterns: ["react-native"]`。它让 RN 单独成一个 PR 而不混进分组。
被否决——RN 单独升仍会稳定失败，等于每周产生一个必定红灯的 PR，噪音比 `ignore` 更大。

**替代方案 B**：把 `expo` / `expo-router` 也加进 `ignore`。被否决——本文件的
`groups.update-types` 是 `["minor", "patch"]`，跨 SDK 的 `expo` 升级（57 → 58）是 **major**，
本来就不进分组、会单独成 PR 供审阅；给它加 `ignore` 反而会连它的安全更新一起挡掉。

**不能**用「`~57.0.x` 已把可升范围锁在 SDK 57 的 patch 内」当理由：该文件未设
`versioning-strategy`（默认 `auto`），Dependabot 会改写 manifest 的范围本身——本仓库 `b34ad17`
就是 dependabot[bot] 把 `"typescript": "~6.0.3"` 改成 `"~7.0.2"` 的。

### D3：锁文件在 `clients/app` 内随 `package.json` 一起重生成

**选择**：由同一个工作包同时改 `clients/app/package.json` 与 `clients/app/package-lock.json`，
锁文件以 `main` 版本为基线重跑 `npm install --package-lock-only` 生成（**不是**在 PR #68 的锁文件上打补丁）。

**理由**：`npm ci` 的同步断言要求两者一致，拆成两个工作包会让中间状态必然失败。以 `main` 为基线
可以保留那条必需的 worklets 嵌套条目（实测重生成后它仍在）。

**已知现象**：重生成会把 `vite` 解析到 `8.3.4`（而非 PR 想要的 `8.3.2`）。这是 `^8.3.2` 范围内的
正常漂移，不是错误；记录在此以免复核时被当成缺陷。

### D4：`ci.yml` 的注释按实测重写两个症状的归属

原注释把 `ERR_PACKAGE_PATH_NOT_EXPORTED` 记给 #68、把「锁文件缺嵌套依赖」记给 #69。实测：
#68 **同时**命中两者（其 diff 中 `react-native-worklets` 出现 2 次、均为删除该嵌套条目的行；
其头部锁文件里该条目 0 条），#69 的**原始** head 也命中锁文件缺陷（同样 0 条、`npm ci` 同样失败），
补齐后才变绿。因此正确的写法是：锁文件缺陷是 **#68 与 #69 共有**的 Dependabot 锁生成器问题，
RN 0.87.1 的 `exports` 破坏才是 #68 独有；原注释的问题不是「归错了 PR」，而是**漏记 #68 也命中
锁文件缺陷**。只改注释文本，不动 job 定义、steps、`working-directory` 或缓存键。

### D5：不新增本地门禁来校验 `.github/**`

**理由**：`npm run check` 的十道脚本都不读 `.github/**`，本变更不扩大这个判定面。对
`dependabot.yml` 的验证退化为「YAML 能解析 + 关键条目存在」的主检出检查（见 plan 的 Project Verify），
对 `ci.yml` 的验证是「YAML 能解析且 job 集合未变」。这与仓库现状一致：这两个文件的语义由 GitHub
侧执行，本地无法等价复现。

## Risks / Trade-offs

| 风险 | 影响 | 缓解 |
|---|---|---|
| `ignore` 的实际生效范围无法在本地验证 | Dependabot 是 GitHub 侧服务，本地没有等价物；配错会静默无效 | 严格按官方文档语义编写（`update-types` 只影响版本更新）；合入后观察下一次 Dependabot 运行是否仍提 RN，必要时用 `@dependabot unignore` 回退 |
| 重生成锁文件可能引入额外漂移 | 除 vite/vitest 外的包版本可能随 `^`/`~` 范围上移 | 合入前用 `npm ci` + `npm run check` 断言可安装且检查全绿；把解析结果与预期版本一并记录在 PR 描述里 |
| 隔离工作区可能无法访问 npm registry | 工作包无法生成锁文件，交付不完整 | 兜底：由主 Agent 在主检出执行重生成，并把该步登记为 Project Verify 的一部分（主检出已有验证过的产物） |
| PR #68 保持打开会继续产生红灯 | 分支保护要求全部必需检查，PR 无法合并 | 本变更合入后关闭 #68；若 Dependabot 重算后只剩安全集合，则等价于本变更的 A 部分 |

## Migration Plan

**回滚**：本变更只改两个配置文件与一个 npm 工程的依赖声明，回滚即 `git revert` 对应提交。
没有数据迁移、没有 schema 变更、没有已发布产物受影响。

**部署**：无部署动作。`.github/dependabot.yml` 与 `.github/workflows/ci.yml` 在推送到 `main`
后由 GitHub 直接生效；`clients/app` 的依赖变更在下一次 CI 运行即被验证。

**PR #68 的处置（本变更之外、由人在 GitHub 上执行）**：

1. 本变更合入 `main`。
2. 关闭 PR #68（它携带的 RN 三件套升级不在本变更范围内）。
3. 若 Dependabot 重算后新开的 PR 只含 `vite` / `vitest`（等价于本变更的 A 部分），直接关闭它，
   避免与本变更重复；若它含其它包，按常规流程审阅。

## Open Questions

- Dependabot 对 `ignore` 的重算行为只能在其下一次运行时观察，本变更合入前无法确认。
  这不阻塞实现：配置语义有官方文档依据，且回退手段（删除 `ignore` 或 `@dependabot unignore`）成本极低。

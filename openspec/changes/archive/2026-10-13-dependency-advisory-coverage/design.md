<!-- 方案与取舍；动机与范围见 proposal.md，不在此重复。 -->

# Design

## Context

`scripts/check-dependency-advisories.mjs` 是 CI `deps` job 的 npm 侧判定入口
（`cargo-deny` 负责 Rust 侧）。它把 `npm audit` 的 `cwd` 固定为仓库根，因此只审根
`package-lock.json`。判定语义（按 advisory ID 放行、`critical` 一律失败、豁免须逐条写明理由与到期条件）
已经写好且稳定，本变更**不改变这套语义**，只改变它**作用于哪些锁文件**。

两个事实约束方案：

1. `clients/app` 是**独立 npm 工程**（自带 `package.json` 与 `package-lock.json`），根目录的
   `npm ci` 与 `npm run check` 都不覆盖它。这是仓库已登记的结构（README「仓库当前状态」）。
2. `npm audit` **只读锁文件、不需要 `node_modules`**（实测：在只放 `package.json` +
   `package-lock.json` 的干净目录里可解析出完整 JSON）。因此扩范围**不需要**让 `deps` job 引入 `npm ci`。

`deps` 已是必需的六个状态检查之一（ruleset `main-protection`），所以扩它的判定范围
**无需改动分支保护**。

## Goals / Non-Goals

**Goals:**

- 让 `clients/app` 的依赖 advisory 从「无任何机器判定」变为「由必需检查判定」。
- 把本次对 4 条告警的评估结论**固化进代码**（理由 + 到期条件），使其可持续复核而非依赖记忆。
- 保持 `deps` job 的步骤与失败模式不变（仍是单条 `node scripts/...` 命令）。

**Non-Goals:**

- 不改动任何依赖版本（见 proposal 非目标）。
- 不改写 `npm audit` 的判定阈值或引入新的 advisory 数据源。
- 不把 Rust 侧（`deny.toml`）或密钥扫描（`.gitleaks.toml`）的判定纳入本脚本。
- 不为「好看的输出」重构脚本的既有结构（本次只做必要的范围与登记改造）。

## Decisions

### 决策 1：4 条告警的判定

逐条评估「是否真漏洞 / 是否进产物 / 本项目是否可达 / 上游有无修复」四问，证据如下。

| 包 | GHSA | 严重度 | 进 PWA 产物 | 上游修复版 |
|---|---|---|---|---|
| `decode-uri-component` | `GHSA-vcc3-ghjq-m6fr` | moderate | **是** | `0.5.0` |
| `braces` | `GHSA-vfj7-8cjw-p6xm` | high | 否（构建期） | **无** |
| `node-forge` | `GHSA-86w9-cpqp-85rv` | high | 否（构建期） | **无** |
| `uuid` | `GHSA-w5hq-g745-h8pq` | moderate | 否（构建期） | `11.1.1` |

**四条都是已登记的真实 advisory**，不是扫描器误报。但「是漏洞」与「本项目可达」是两件事：

- **进不进产物**：对 `clients/app` 已导出的 `dist/_expo/static/js/web/entry-*.js` 做字符级核对。
  `decode-uri-component` 独有的 BOM 替换表特征串（`'%FE%FF'`）**命中**；而 `node-forge`、
  `micromatch`、`generateUuid` 均 **0 命中**。产物里的 8 处 `uuid` 经上下文核对全部是
  **Expo 自身的** `uuidv4`/`uuidv5` helper 与 `crypto.randomUUID`，不是 `uuid@7` 这个包。
- **`braces`**（嵌套 glob 模式导致栈耗尽 DoS）：调用方是 `metro-file-map`，模式来自 Expo 自身配置，
  不来自外部输入 → 本项目不可达。
- **`node-forge`**（RSA PKCS#1 v1.5 签名校验放宽）：位于 `@expo/code-signing-certificates`，
  属 Expo Updates 的代码签名路径；本项目只做 `expo export --platform web`，不产出 Updates 签名包 → 不可达。
- **`uuid`**（v3/v5/v6 传入 `buf` 时缺边界检查）：`xcode` 只调用 `uuid.v4()` 且**不传 `buf`**，
  不触碰 v3/v5/v6 → 漏洞前提不成立。
- **`decode-uri-component`**（畸形百分号编码导致指数级解码 DoS）：在产物里，
  `expo-router` 的 `parseQueryParams` → `query-string.parse` → decode 这条路径可达，
  触发方式是打开带畸形 query 的链接 → **判定为真实可达问题**。

替代方案：把 4 条一律当「不可达」登记。**否决**——`decode-uri-component` 有产物级证据表明可达，
把它与「不可达」混为一谈会让豁免清单失去可信度。

### 决策 2：`decode-uri-component` 本次不修，登记为待办 + 到期条件

三条修复路径已实测，两条被否掉：

| 路径 | 实测结果 |
|---|---|
| `overrides` 强压 `decode-uri-component@0.5.0` | **运行时崩溃**：Node CJS 与 esbuild 浏览器打包**都是** `TypeError: decodeComponent is not a function` |
| 强压 `query-string@9.5.1`（已依赖 `^0.5.0`） | **接口变形**：`expo-router` 用 `__importStar(require(...))` 取到 `stringify === undefined` |
| 升级到 Expo SDK 58（`expo-router@58` 移除 `query-string`） | 有效：`query-string` 与 `decode-uri-component` **双双消失** |

前两条的死因是同一条逻辑，无法绕开：

```text
被通报范围： decode-uri-component  <= 0.4.2
唯一修复版： 0.5.0
0.2.2        = CJS  → 兼容 query-string@7 的 require()，但它是漏洞版
0.4.1 / 0.5.0 = ESM → 与 query-string@7 的 CJS require() 不兼容
```

即「安全」与「可 require」在该包上互斥；要同时满足必须换掉消费者，而换消费者就等于升 `expo-router` 到 58。

而 SDK 58 的代价经实测确认：

- `expo@58.0.7` 的 dist-tag 是 **`next`**，不是 `latest`（`latest` 仍是 `57.0.27`）→ **尚未 GA**；
- 它期望 `react-native = 0.88.0-rc.4`（**release candidate**；RN 稳定版仍停在 `0.87.1`）；
- `react` / `react-dom` 需 `19.3.0`（当前 `19.2.8`）；
- `expo-router@58` 新增若干必需 peer（`expo-linking`、`react-native-screens`、
  `react-native-safe-area-context`、`@expo/metro-runtime` 等），解析时出现 `ERESOLVE` 覆盖警告；
- 且实测 SDK 58 树里 **`braces` / `node-forge` / `uuid` 原样保留**（上游无修复），
  即付出跨 SDK major 的代价**也只清掉 4 条里的一条**。

**结论：本次不修，登记豁免并附到期条件**「Expo SDK 58 进入 npm `latest` 时升级并删除本条」。
权衡理由：用「pre-GA SDK + RC 版 RN + 更多 peer」的构建风险去换一条 medium、需用户主动打开
恶意链接的 DoS，收益低于代价；而且这类升级正是 `.github/dependabot.yml` 有意挡住的窗口。

替代方案一：现在就升 SDK 58。**否决**（代价如上）。
替代方案二：先不扩门禁、让它红着直到修好。**否决**——那会把「必需检查永久红灯」变成常态，
而 README/ADR-0008 已判定「靠管理员 bypass 变成习惯是比这条 DoS 更大的风险」。

### 决策 3：豁免登记增加「适用工程」维度

`braces` 的 `GHSA-vfj7-8cjw-p6xm` **同时出现在两个工程**的锁文件里（根：经
`@fission-ai/openspec` → `fast-glob` → `micromatch`；`clients/app`：经 `expo` → `@expo/metro`
→ `metro-file-map` → `micromatch`）。现有 `ALLOWED` 只按 advisory ID 索引，扩范围后会出现
「一个工程的豁免顺带放行另一个工程的同号 advisory」——虽然本例中两者确实都该放行，
但**这种隐式放行是错的抽象**：同号 advisory 在不同工程里可达性可以完全不同。

因此登记项新增一个「适用工程」字段，判定时要求 **(advisory ID, 工程)** 双双匹配。
替代方案：维持 ID 单键、把 `braces` 的豁免理由写成「两个工程通用」。**否决**——它无法表达
「同一 advisory 只在 A 工程豁免」这类必然出现的情况（例如本变更的另外 3 条就只适用于 `clients/app`）。

### 决策 4：扩范围的实现形状

脚本改为「按工程逐一 audit + 合并判定」：每个工程各自跑 `npm audit --json`，各自解析根因
advisory，再与「该工程的豁免集合」比对；任一工程有未登记项或任何 `critical` 即整体失败。
失败输出须**标明工程路径**，否则排查时无法区分「根的问题」与「前端的问题」。

替代方案：一次性把两份锁文件合并审计。**否决**——`npm audit` 按 `cwd` 读取当地锁文件，
不存在合并入口；且按工程输出才能保留定位能力。

## Risks / Trade-offs

- **[豁免会掩盖真实的新洞]** → 豁免按 (ID, 工程) 精确匹配，且新增任意未登记 advisory 立即失败；
  脚本已有「豁免未命中即提示复核」的输出，保持不变。
- **[`decode-uri-component` 长期挂着被遗忘]** → 豁免条目带 `expires` 字段，写明
  「SDK 58 进入 `latest` 时升级并删除」，并在 `README` 与 ADR-0008 各留一处指回。
- **[本地无法验证判定]** → 与 `deps` job 其余判定同理（README 已声明依赖/公告/密钥三项只在 CI 运行，
  本地无等价物）。本地可验证的部分是**脚本自身的合并逻辑与 (ID, 工程) 匹配**，用构造的
  `npm audit --json` 输入做单元级核对；真实 advisory 数据面只能在 CI 观察。
- **[扩范围后 `deps` 可能先红]** → 这是预期的一次性状态：变更落地时 4 条豁免已同时登记，
  因此合入后应为绿；若合入后仍红，说明实测与 CI 的 advisory 数据面存在差异，须按实际输出修正登记项。

## Migration Plan

本变更**不涉及数据迁移**；它改变一个 CI 判定的覆盖范围，因此给出落地与回滚：

1. **落地**：脚本改造 + 4 条豁免登记 + 文档同步必须在**同一批**合入——只改脚本会让 `deps`
   立刻红（4 条未登记），只改登记则无效果。
2. **验证**：合入后观察 `deps` job 的输出，确认两个工程都被审计且 4 条豁免命中
   （脚本会逐条打印「放行 <ID>（<包>）：<理由>」与到期条件）。
3. **回滚**：把脚本的工程列表还原为单工程即可恢复原判定面；登记项可留（未命中时只会提示复核，
   不会失败）。回滚不影响产品代码与任何其它 job。

## Open Questions

- 无。本次需要定夺的技术选择（是否现在升 SDK 58、豁免粒度、实现形状）都已在上方决策中判定；
  剩余未知项（上游何时发布修复）属于可安全延后的外部节奏，已通过 `expires` 条件表达。

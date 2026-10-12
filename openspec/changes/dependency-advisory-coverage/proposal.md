<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

仓库的依赖 advisory 门禁**只审根 npm 工程，完全看不见 `clients/app`**，因此前端依赖的漏洞告警
没有任何必需检查能拦住。

现状：`scripts/check-dependency-advisories.mjs` 把 `npm audit` 的工作目录写死为仓库根
（`cwd: ROOT`），而 `clients/app` 是一个**独立的 npm 工程**（自带 `package.json` 与
`package-lock.json`），根目录的 `npm ci` 与 `npm run check` 都不覆盖它。结果是 GitHub 上
4 条 open 告警**全部**挂在 `clients/app/package-lock.json`，其中 2 条 high，而 `deps`
这个必需检查对它们**完全没有判定**——它们会一直挂着，直到有人偶然点开 security 页。

这个洞与前端工程同时存在，且已经造成过实盘教训的反面样本：`frontend` job 的注释记录了
「`clients/app` 的缺口实盘暴露过两次，两次在原来的 job 集合下都是全绿」（#68、#69）。
本变更补的是同类缺口在**安全判定**上的那一面。

为什么是现在：4 条告警已实测评估完毕（判定与证据见 design.md 决策 1），
其中 3 条上游**没有修复版本**、1 条修复代价高于收益。结论只有在门禁能看见它们之后
才能被固化并持续复核，否则这次评估只存在于对话里。

## What Changes

- **把 advisory 门禁的判定范围扩到 `clients/app`**：`scripts/check-dependency-advisories.mjs`
  改为对仓库根与 `clients/app` **两个** npm 工程各跑一次 `npm audit`，并合并判定。
- **豁免登记增加「适用工程」维度**：现有 `ALLOWED` 条目按 advisory ID 索引，而 `braces` 的
  `GHSA-vfj7-8cjw-p6xm` **同时出现在两个工程**里。登记项必须显式声明它覆盖哪个工程，
  避免一个工程的豁免静默放行另一个工程的同号 advisory。
- **登记本次评估的 4 条豁免**（理由、适用范围与到期条件逐条写明，见 tasks）：
  - `braces`（`GHSA-vfj7-8cjw-p6xm`，high）：两个工程都命中，上游无修复；
  - `node-forge`（`GHSA-86w9-cpqp-85rv`，high）：上游无修复，构建期工具链；
  - `uuid`（`GHSA-w5hq-g745-h8pq`，moderate）：漏洞前提在本仓库不成立；
  - `decode-uri-component`（`GHSA-vcc3-ghjq-m6fr`，moderate）：**已评估为真实问题但本次不修**，
    附带明确到期条件（Expo SDK 58 进入 `latest` 时升级并删除本条）。
- **同步权威文档**：`README.md` 的「合同检查」与 `docs/adr/0008-ci-supply-chain-tooling.md`
  的残余风险/记录节，说明该 job 现在覆盖两个 npm 工程以及 4 条豁免的性质差异。

**本次不包含**（非目标，明确排除以免范围蔓延）：

- **不升级任何依赖版本**，特别是**不升级到 Expo SDK 58**。`decode-uri-component` 的修复需要
  SDK 58，而它当前是 npm 的 `next` 标签（`latest` 仍是 57.0.27）、期望 `react-native 0.88.0-rc.4`
  （release candidate）。用 pre-GA SDK + RC 版 RN 换一条 medium DoS 不划算，故延后并登记到期条件。
- **不改 `braces` / `node-forge` / `uuid` 的判定结论**（保持「上游无修复 + 本项目不可达」）。
- **不新增 CI job、不改分支保护的必需检查**：`依赖许可证与来源`（`deps`）本来就是必需检查，
  扩它的判定范围即可；且实测 `npm audit` 只读锁文件、不需要 `node_modules`，`deps` job 无需加 `npm ci`。
- **不改任何 crate、协议 wire、端口合同、存储 DDL、前端产品行为**。
- **不改 `deny.toml`、`.gitleaks.toml`**（Rust 与密钥扫描的判定面不变）。

## Capabilities

<!-- 先检查现有规范，区分新增能力与已有能力的需求变化。
 仅当规范层面的行为不变时（如不改变行为的纯重构、工具或文档变更），
 在该变更的 .openspec.yaml 中设置 skip_specs: true，说明理由并引用既有行为契约。
 此时两类能力清单均无变更，删除占位项；否则至少声明一项能力并生成对应增量规范。
 不得为通过校验而编造需求。 -->

本变更为**依赖安全门禁与 CI 配置修正**：产品行为、协议 wire、端口合同、存储 DDL 与前端呈现
均不变，`openspec/specs/` 下既有能力规范的每一条 Requirement/Scenario 语义保持原样
（已用 `openspec list --specs` 盘点 24 个能力，并逐个检索确认**没有任何规范**覆盖 CI 门禁、
依赖 advisory 或供应链判定；`scope-expansion` 命中的是「授权 scope 展开」，与门禁无关）。
按变更流程 schema 的规则，在该变更的 `.openspec.yaml` 中设置 `skip_specs: true`，
不生成增量规范文件。同类先例：`dependabot-sdk-alignment`、`dependabot-sdk57-pinning`、
`storage-ddl-constraint-coverage`、`test-temp-dir-cleanup` 均以该标记处理工具链变更。

`clients/app` 的**可观察行为契约**（七层依赖方向、设备身份、连接状态机、命令终态、
目录与对话页呈现、no-content-cache 离线边界）已冻结在 `pwa-web-client` 规范里，本变更不触碰它们；
`node-forge` / `braces` / `uuid` / `decode-uri-component` 是 Expo 工具链的传递依赖，
本变更既不改变任何依赖版本，也不改变任何规范的验证语义。

### New Capabilities

无（`skip_specs: true`；本变更不改变任何规范层面的可观察行为）。

### Modified Capabilities

无（`skip_specs: true`；同上）。

## Impact

- **判定脚本**：`scripts/check-dependency-advisories.mjs`——判定范围从 1 个 npm 工程扩到 2 个；
  `ALLOWED` 条目结构新增「适用工程」字段；新增 3 条豁免（`braces` 复用，因同号 advisory 已在清单中）。
  该脚本**不在 `npm run check` 的链上**（与 `deny.toml`/`.gitleaks.toml` 同理，见 README「合同检查」），
  只在 CI 的 `deps` job 运行，本地无等价物。
- **CI 配置**：`.github/workflows/ci.yml` 的 `deps` job —— 仅更新那段注释使其与新判定范围一致，
  **步骤命令本身不变**（仍是 `node scripts/check-dependency-advisories.mjs`）。
- **权威文档**：`README.md`「合同检查」的 `deps` 描述；`docs/adr/0008-ci-supply-chain-tooling.md`
  的残余风险条目（新增本次范围扩展与豁免性质差异）。依 `AGENTS.md` §9，这两处是该类变更的映射目标；
  **不涉及**产品、协议、安全模型、模块边界或存储，故不触发其它权威文档。
- **依赖**：**零依赖变化**。`package.json` 与两份 `package-lock.json` 均不改动。
- **风险**：豁免范围依赖上游发布节奏（SDK 58 何时进 `latest`、`braces`/`node-forge` 何时修复）；
  本地无法执行该脚本的判定（与 `deps` job 其余判定同理，见 design.md 的 Risks）。

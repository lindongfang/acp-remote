# Tasks

## 1. 扩审计范围并登记豁免（WP1）

- [x] 1.1 在 `scripts/check-dependency-advisories.mjs` 中把审计目标从单一仓库根改为**工程列表**
      （仓库根 + `clients/app`），每个工程各跑一次 `npm audit --json`；完成判据：脚本内出现
      `clients/app` 这一项，且本地 `node scripts/check-dependency-advisories.mjs` 的输出显示
      两个工程都被审计（对照本变更前只输出根工程的判定）。
- [x] 1.2 给 `ALLOWED` 条目新增「适用工程」字段，判定改为按 **(advisory ID, 工程)** 双匹配；
      完成判据：构造一组「A 工程登记、B 工程同号 advisory 未登记」的输入能被判为失败——
      用一份手工构造的 `npm audit --json` 夹具（或临时改登记项）实跑一次，确认未被静默放行，
      并把该次实跑的命令与结果写进工作包报告。
- [x] 1.3 登记 4 条豁免，逐条写明理由、适用范围与**到期条件**：`braces`（`GHSA-vfj7-8cjw-p6xm`，
      覆盖两个工程，上游无修复）、`node-forge`（`GHSA-86w9-cpqp-85rv`，仅 `clients/app`，上游无修复）、
      `uuid`（`GHSA-w5hq-g745-h8pq`，仅 `clients/app`，漏洞前提在本仓库不成立）、
      `decode-uri-component`（`GHSA-vcc3-ghjq-m6fr`，仅 `clients/app`，**真实可达但本次不修**，
      到期条件 = Expo SDK 58 进入 npm `latest` 时升级并删除本条）；
      完成判据：`node scripts/check-dependency-advisories.mjs` 退出码 0，且输出逐条打印这 4 条的理由与到期条件。
- [x] 1.4 让失败输出**标明工程路径**（否则无法区分「根的问题」与「前端的问题」）；
      完成判据：人为制造一条未登记 advisory（或临时移除一条登记）触发失败，确认错误信息里出现
      出问题的工程路径，随后还原。
- [x] 1.5 保留既有判定语义不变：只按 advisory ID 放行、`critical` 一律失败、豁免未命中时提示复核；
      完成判据：diff 中这三条行为未被削弱，且 1.3 的实跑输出仍包含「豁免未命中即提示」那段逻辑。

## 2. 同步权威文档（WP2）

- [x] 2.1 更新 `README.md` 的「合同检查」中对 `deps` job 的描述，说明该 npm 侧判定现覆盖
      **两个** npm 工程（仓库根与 `clients/app`）；完成判据：`node -e` 断言 README 的该处
      含 `clients/app`，且**保留**「`deps`/`advisories`/`secrets` 本地没有等价物」的既有声明
      （该事实未变，不得删改）。
- [x] 2.2 在 `docs/adr/0008-ci-supply-chain-tooling.md` 的残余风险/记录节登记本次范围扩展，
      并区分两类豁免的**性质差异**（`braces`/`node-forge`/`uuid` 为「不可达」；
      `decode-uri-component` 为「真实可达但延后」）；完成判据：ADR 中出现该区分，
      且 `npm run check` 的 `check:doc-links` 通过（相对链接未断）。
- [x] 2.3 在两处各留一处指回 `decode-uri-component` 的到期条件；完成判据：README 与 ADR 中
      均能检索到该 advisory 号或「SDK 58 进入 latest」这一触发条件。

## 3. 同步 CI 注释（WP3）

- [x] 3.1 更新 `.github/workflows/ci.yml` 中 `deps` job 的那段注释，使其与新的判定范围一致；
      完成判据：diff 显示**只改注释文本**，`jobs` 名集合仍为六个
      （`advisories,checks,commits,deps,frontend,secrets`），且 `deps` 的判定命令仍是
      `node scripts/check-dependency-advisories.mjs`（用 PV5 的内联断言实跑确认）。

## 4. 集成与验收（Workflow follow-up 前的收口）

- [x] 4.1 三个工作包同批合入后，在主检出执行 PV1（`npm run check`）与 PV2
      （`node scripts/check-dependency-advisories.mjs`）；完成判据：两者退出码均为 0，
      且 PV2 输出同时覆盖两个工程与 4 条豁免。
- [x] 4.2 在主检出执行 PV3 与 PV5 的内联断言；完成判据：脚本工程列表确实含 `clients/app`、
      CI 的 job 集合与判定命令未被误改（两条均打印 `OK`）。
- [ ] 4.3 记录本次合入后 CI `deps` job 的真实结果；完成判据：`gh pr checks` 显示 `依赖许可证与来源`
      为绿，或如实记录与本地实测的差异（不得把未观察到的结果写成已通过）。

## Workflow follow-up

- 合入后按 `docs/WORKFLOW.md` §6.2 走远端落地（分支 → PR → 六个必需检查 → squash 合并）。
- 归档前确认 `npm run check` 全绿，再由后续请求启动 `/opsx-archive` 或等价归档流程。

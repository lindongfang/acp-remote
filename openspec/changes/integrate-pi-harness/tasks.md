# Tasks

本轮仅修订规划；下列均为后续实施任务，未勾选不代表规划产物缺失。开始前须取得新的实施授权、完成独立方案/用例检视并满足 `WP1` 门禁。任何任务均不在仓库绑定或锁定本机 `pi` 版本。用例定义见 `tests.md`，分工与目录见 `plan.md`。

## 1. 环境摸底、隔离探针与实施门禁

- [ ] 1.1 WP1 在不修改本机安装、不绑定 `pi` 版本的前提下，只读定位本机 `pi` 入口，并在 `testing/probes/pi-harness/discovery/` 完成候选外部 ACP 服务端审查与隔离协议探针（仅按需锁定探针适配器依赖，绝不安装 `pi` 本体），实证核查 `clientCapabilities: {}` 与反向 `-32601` 下的握手/工具/委托表现；完成判据为 `testing/runs/integrate-pi-harness/discovery/` 脱敏报告齐备，PI-001 取证部分有依据，不兼容项明确阻断。
- [ ] 1.2 WP1 确定 `.tmp/` 隔离工作区、配置/会话根、认证边界、Windows 绝对路径无 Shell 原生 PE 启动参数合同及 Job 进程树清理验证方案；完成判据为 `proposal.md`/`design.md` 实施输入更新完毕、参数合同可执行且本机用户数据零改动，实际清理效果留待 WP4 验证。
- [ ] 1.3 WP1 由独立 reviewer 复核实证后的方案、`tester` 用例与源码边界一致性并将门禁检视记录写入 `testing/runs/integrate-pi-harness/discovery/`（若需调整 `tests.md` 预期则先按 `plan.md` 补充 `tester` 写范围并复核）；完成判据为 `openspec validate integrate-pi-harness --strict` 通过、PI-001 映射与实施门无未闭环阻断。

## 2. 无版本绑定配置与节点合同

- [ ] 2.1 WP2 在 `testing/probes/pi-harness/profiles/` 与 `testing/probes/pi-harness/verify.ps1` 交付 Windows 绝对路径原生 PE 可执行启动的无密钥 `format: 1` 配置模板（不含 `pi` 版本绑定、拒绝相对路径及 `.ps1` 直接作为 `executable`）及节点侧验证脚本，并编写 `docs/implementation/pi-harness.md`；完成判据为 PI-003–006 底层与节点开发断言及 `npm run check:rust` 通过，模板不含密钥或用户私有路径。
- [ ] 2.2 WP2 覆盖底层配置（含 `mcp_servers`、`extra` 保真）、结构内容保真、无客户端委托时的标准拒绝、原生恢复、来源隔离及取消与 Job 关闭分离的节点合同并同步更新 `docs/implementation/pi-harness.md`；完成判据为 PI-007–009、PI-011 节点开发范围通过（若含新增产品断言则附 Red Check 证据），未引入厂商运行时分支。

## 3. 通用 Web 范围与限制展示

- [ ] 3.1 WP3 在 `clients/web/tests/pi-harness.test.ts` 编写通用表单已暴露字段（含行级 `trim` 参数边界）、单入口登记、能力/委托错误及历史不完整提示的专项测试，并编写 `docs/implementation/pi-harness-web.md` 如实说明当前表单未暴露 `mcpServers`/`extra`；完成判据为 PI-004–009 页面断言及 `npm --prefix clients/web run check`、`test`、`build` 实际通过，未私改通用生产源码。
- [ ] 3.2 WP3 覆盖真实权限请求有效性、无审批事件时的能力边界说明、页面断线及取消与实例释放分开展示的 Web 测试并更新 `docs/implementation/pi-harness-web.md`；完成判据为 PI-010–011 页面断言通过，文档与真实平台能力一致。

## 4. 独立验收与真实本机闭环

- [ ] 4.1 WP4 独立 tester 在 `testing/probes/pi-harness/acceptance/` 实现专项验收入口 `run.ps1` 与协议/故障/委托替身夹具并核对 `tests.md` 预期；完成判据为 PI-003–012 每例可执行、不硬编码校验特定 `pi` 版本且具备隔离与异常清理保护，缺环境记 `not-run` 不转为 LX。
- [ ] 4.2 WP4 对 WP2/WP3 集成后的冻结快照执行 PI-001–012，输出逐 ID 报告至 `testing/runs/integrate-pi-harness/tester/`；完成判据为开发/模拟/真实范围分列、问题闭环，`npm run check` 与 `npm run build:rust` 退出码及非零断言数可追溯。
- [ ] 4.3 WP4 在隔离测试根连接本机已安装 `pi` 与外部 ACP 服务端执行 PI-013 真实 Windows/Web 闭环、Job 完整进程树收敛及用户本机数据保护检查；完成判据为只读记录当次观测到的双组件版本、全部必需真实分支有据、可选不支持或未暴露项有实证说明，用户本机 `pi` 安装与真实会话零改动。

## 5. 最终集成、长期规格与套件同步及独立检视

- [ ] 5.1 WP5 固定最终文件清单及摘要，由独立 reviewer 与 tester 核对同一冻结快照并复查全部问题；完成判据为 `testing/runs/integrate-pi-harness/reviewer/` 与 `review.md` 独立结论无阻断，完整 PI-001–013 状态可判定。
- [ ] 5.2 WP5 同步 `openspec/specs/pi-harness-compatibility/spec.md`、`testing/suites/pi-harness-compatibility/tests.md` 与 `testing/suites/traceability/integrate-pi-harness.md`（长期套件仅指向 `openspec/specs/` 长期规范，禁止跨目录补位），并在保护其他在途修改的前提下按串行顺序同步 `README.md`、`docs/HARNESS_CONTROL_PLANE_REDESIGN.md`、`docs/REBUILD_CAPABILITY_MAP.md`；完成判据为 `openspec validate --all --strict`、`git diff --check`、长期规格与套件 ID/场景精确映射及链接核验全部通过。
- [ ] 5.3 WP5 将冻结版本、当次观测组件信息、逐例结果、未执行项、无版本绑定升级复验边界及独立复核结论回填 `plan.md`；完成判据为不依赖外部私有日志即可判断交付范围，且与 `proposal.md` SC2–SC4 逐项对应。

## Workflow follow-up

- 全部 tracked 任务、长期规格与长期套件同步及独立检视闭环后，按项目归档流程执行 `openspec archive` 并复核移动映射、文件摘要保真和跨文档链接；本轮不归档。
- 提交、推送和 PR 按后续用户授权执行，本轮不提交。

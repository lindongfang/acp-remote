# Pi 接入配置与 ACP 兼容性验收

## Why

用户希望将本机已安装的 Pi 纳入当前接入计划，明确当前阶段聚焦支持 Codex 与 Pi（暂缓 OMP 适配），同时明确项目不绑定 Pi 的版本。复用通用 ACP 平台与现有单一对端目录模型，评估外部 ACP 入口、Windows 原生可执行启动配置与真实兼容性证据，不将同源项目、文档声明或旧 Harness 的能力当作已支持。

## Success Criteria

### 用户原始要求

> 我本地安装了 pi，pi和 omp需要适配一下。

> 先基于 Deepmind Harness  和 zcode 创建pi和 OMP 的 change 吧。

> 1. 修复上述5个问题
> 2. 本地已经安装了pi。但是当前项目不要绑定pi的版本

> 先去掉omp的适配吧，暂时先支持codex和pi

按前文已删除的 DeepSeek Harness 理解“Deepmind Harness”；OMP（Oh My Pi）已按最新确认暂缓适配并移除对应活跃变更。本次授权为修订规划与接入范围，不是执行安装、升级或产品代码改造。

### 硬约束与非目标

- 首期 Windows 节点与 Web；遵循 [总体设计](../../../docs/HARNESS_CONTROL_PLANE_REDESIGN.md)、当前长期规格和 AGENTS.md。
- **不绑定 Pi 版本**：不在仓库依赖、锁文件、启动配置模板、探针脚本或平台代码中固定、打包或校验特定 Pi 版本；直接对接用户本机已安装的 `pi`，不擅自安装、升级或降级本机 `pi`。
- 仅交付无版本绑定的接入配置及真实兼容性验收；复用通用 ACP 与现有单入口目录模型，不扩充双组件目录字段、不私改通用 Web 安装表单、不创建厂商协议客户端、私有历史读取器、永久聊天副本或自动能力兜底。
- 不恢复 DeepSeek/ZCode/OMP 计划，不继承旧变更的任务勾选、孤立顶层目录结构、历史验收或已失效的重放准入规则。
- 当前处于远期需求与设计定义阶段（`schema: spec-only`），本轮不运行真实 Harness、不修改本机认证、会话、密钥或产品代码；正式启动本变更时先升格为 `schema: spec-driven` 并基于当时真实代码树生成 `tests.md`、`plan.md` 与 `tasks.md`。

### 成功判据

- SC1（规划阶段）：完成 `proposal.md`、增量规格 `specs/pi-harness-compatibility/spec.md` 与 `design.md` 规划收敛并同步对齐 Codex + Pi 目标范围，消除与现有源码模型及仓库目录约定的冲突，严格校验通过；未解决的真实环境取证列为正式开工后的首阶段前置条件。
- SC2（后续交付）：在不绑定本机 Pi 版本的前提下，明确外部 ACP 服务端来源、许可证、隔离探针依赖边界与符合 Windows 无 Shell 约束的原生可执行启动链，形成无密钥配置模板及脱敏环境记录（仅只读记录当次实测观察到的本机 Pi 版本）。
- SC3（后续交付）：真实验证输入、结构输出、客户端空能力声明下的工具与委托行为、权限请求（若提供）、取消、页面断线、恢复/历史与完整进程树释放；区分底层配置合同与当前通用 Web 表单范围，支持、不支持、未验证及平台未实现分别记录。
- SC4（后续交付）：正式开工升格 `schema: spec-driven` 后补齐独立 `tests.md`、`plan.md` 与 `tasks.md`，经编码前门禁与实现后由独立 `tester`/`reviewer` 核对同一受测集成 `commit SHA`，逐例报告结果并按仓库套件规范同步长期资产；缺环境或所需证据记未执行，不以模拟或其他 Harness 结果代签。

## What Changes

- 新增 Pi 的无版本绑定接入规划、单入口配置与双组件证据归属边界。
- 候选为第三方 `pi-acp` ACP 服务端（由其拉起本机 `pi` RPC 子进程）；节点仅保存单个 ACP 启动入口与显式来源，不在运行时拆分双组件模型，不能将 Pi RPC 当作 ACP。
- 对齐平台真实代码边界：启动配置遵循 Windows `OwnedProcess` 无 Shell 原生可执行约束（不将 `.ps1` 作为直接可执行入口）；核实平台 `clientCapabilities: {}` 与非审批反向请求返回 `-32601` 下的工具/审批表现；区分底层 `format: 1` 配置（含 `mcp_servers`、`extra`）与当前通用 Web 表单已暴露字段。
- 收敛全部拟建资产路径至既有仓库目录（`testing/probes/pi-harness/`、`docs/implementation/`、`testing/runs/integrate-pi-harness/`、`openspec/specs/`、`testing/suites/`），不再新建孤立的 `configs/`、`scripts/`、`testing/harnesses/` 目录树。
- 当前接入目标范围收敛为 Codex 与 Pi（OMP 暂缓适配并移除对应 change），本变更不阻塞其他通用能力。

## Capabilities

### New Capabilities

- `pi-harness-compatibility`：Pi 无版本绑定接入配置、协议交互边界及兼容性证据要求，文件为 `specs/pi-harness-compatibility/spec.md`。

### Modified Capabilities

无。复用 `openspec/specs/harness-management`、`native-session-recovery`、`session-execution`、`structured-history`、`permission-approvals`、`management-input-upload` 等长期规格；专项不重定义通用协议或业务规则。

## Impact

- 依赖通用闭环的实际合同和现有实现；参考 [归档设计](../archive/2026-11-16-deliver-codex-session-loop/design.md)，运行规则以长期规格为准。
- 已有接入面为 `app/src/adapters/acp/node_configuration_profile.rs`、`catalog_draft_document.rs`、`strategy.rs`、`transport/state.rs`、`app/src/platform/process_owner.rs`、`app/src/node/catalog.rs` 与 `clients/web/src/management/workspace-catalog-view.ts`；不假定存在厂商专属模块。
- 后续在 `testing/probes/pi-harness/` 与 `docs/implementation/` 新增无版本绑定配置示例、隔离探针、验收入口与说明（具体工作包划分在开工阶段 `plan.md` 定义），归档时同包同步 `openspec/specs/` 与 `testing/suites/`；当前不加产品依赖、迁移或运行时版本白名单。

## Exploration

- 第一轮确认（2026-11-16）：用户曾要求参照旧两项 change 建立 Pi/OMP 规划；旧结构仅参考职责与验收分层，不复制旧方法、孤立目录或结果。
- 第二轮确认（2026-11-16）：用户确认**本机已安装 `pi`，且当前项目不要绑定 `pi` 的版本**，同时修复首轮规划与真实源码/目录核对出的 5 项问题：
  1. **身份与版本归属**：节点目录与 ACP 握手保持现有单一对端入口模型，双组件信息仅由探测/兼容性报告只读记录当次观测值，且项目配置、探针锁文件与代码均不锁定或绑定 `pi` 本体版本。
  2. **Windows 启动链**：对齐 `OwnedProcess::spawn` 直接调用 `CreateProcessW`（无隐式 Shell）的实现，采用 Windows 绝对路径原生 PE 可执行入口（如 `node.exe` + JS 入口），将相对路径可执行名与 `.ps1` 等非 PE 脚本归入拒绝验证。
  3. **Web 表单与客户端委托边界**：区分底层 `format: 1` 配置（支持 `mcp_servers`、`extra`、原始 UTF-16LE 参数）与当前通用 Web 表单（固定 `mcpServers: []`、`extra: {}` 且按行 `trim` 参数），并将平台 `clientCapabilities: {}` 及非审批反向请求返回 `-32601` 下的 `pi-acp` 工具/审批真实行为列为前置探测与闭环验收必查项。
  4. **目录收敛**：统一对齐仓库既有 `testing/probes/pi-harness/`、`docs/implementation/`、`testing/runs/integrate-pi-harness/`、`openspec/specs/` 与 `testing/suites/`，取消新建 `configs/`、`scripts/`、`testing/harnesses/`。
  5. **隔离探测写范围**：允许前置探测阶段在 `testing/probes/pi-harness/discovery/` 建立隔离协议探针（仅按需锁定候选适配器探针自身依赖，绝不安装或绑定 `pi` 本体）。
- 第三轮确认（2026-11-17）：用户确认**先去掉 OMP 的适配，暂时先支持 Codex 和 Pi**，并按远期变更轻量化规则切换为 `schema: spec-only`（正式排期开工时再升格为 `schema: spec-driven` 生成 `tests/plan/tasks`）。
- 实施前阻断输入：本机已安装 `pi` 的实际调用入口、候选外部 ACP 服务端可用性与许可证审查、隔离认证与会话根、Windows 完整进程树收敛验证方法。允许先完成需求与设计冻结；正式适配与验收前须在开工首阶段实证解决。
- 依赖选择若要求新增私有桥接、丢失关键结构或改变安全/数据权威，超出本 change；报告具体差异并单独决定，不通过降级绕过。
- 术语复核：Pi 是具体程序标识，不是新增业务对象；沿用接入来源、启动配置、原生身份及连接代际，GLOSSARY 无需变更。当前不形成新跨层决策，暂不新增 ADR。

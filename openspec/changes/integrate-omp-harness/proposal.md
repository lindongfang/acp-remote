# OMP（Oh My Pi） 接入配置与 ACP 兼容性验收

## Why

用户希望将 OMP（Oh My Pi） 纳入当前接入计划。复用通用 ACP 平台，独立建立真实入口、版本、启动配置与兼容性证据，不将同源项目或旧 Harness 的能力当作已支持。

## Success Criteria

### 用户原始要求

> 我本地安装了 pi，pi和 omp需要适配一下。

> 先基于 Deepmind Harness  和 zcode 创建pi和 OMP 的 change 吧。

按前文已删除的 DeepSeek Harness 理解“Deepmind Harness”，OMP 暂按 Oh My Pi 标识；本次授权为创建规划，不是执行安装、升级或产品适配。

### 硬约束与非目标

- 首期 Windows 节点与 Web；遵循 [总体设计](../../../docs/HARNESS_CONTROL_PLANE_REDESIGN.md)、当前长期规格和 AGENTS.md。
- 仅接入配置及真实兼容性验收；复用通用 ACP，不创建厂商协议客户端、私有历史读取器、永久聊天副本或自动能力兜底。
- 不恢复 DeepSeek/ZCode 计划，不继承旧变更的任务勾选、历史验收或已失效的重放准入规则。
- 本轮不安装或运行真实 Harness、不修改本机认证、会话、密钥或产品代码；不声明任何程序版本已经兼容。

### 成功判据

- SC1（本轮）：形成 proposal、增量规格、design、独立 tests、plan、tasks 六类规划，严格校验通过；未解决的真实环境输入明确列为实施前条件。
- SC2（后续交付）：固定程序及其 ACP 入口版本和来源、许可证、依赖与 Windows 启动链，形成可复现的隔离配置和脱敏证据。
- SC3（后续交付）：真实验证输入、结构输出、权限请求（若提供）、取消、页面断线、恢复/历史与释放；支持、不支持、未验证及平台未实现分别记录，不从恢复推断历史完整。
- SC4（后续交付）：独立 tester/reviewer 核对同一最终版本，逐例报告结果；缺环境或所需证据记未执行，不以模拟或其他 Harness 结果代签。

## What Changes

- 新增 OMP（Oh My Pi） 的接入兼容性规划和可验证能力边界。
- 候选为 OMP 上游提供的 omp acp 入口；不得以 omp --mode rpc 或交互终端代替 ACP。
- 通过现有启动配置及来源登记入口接入，Web 复用通用能力/错误展示；通用标准缺陷在公共模块修复，不复制实现。
- 将当前目标范围更新为 Codex、Pi、OMP；两个新 change 相互独立，不阻塞其他通用能力。

## Capabilities

### New Capabilities

- omp-harness-compatibility：OMP（Oh My Pi） 接入配置、能力边界及版本证据，文件为 specs/omp-harness-compatibility/spec.md。

### Modified Capabilities

无。复用 openspec/specs/harness-management、native-session-recovery、session-execution、structured-history、permission-approvals 等长期规格；专项不重定义通用协议或业务规则。

## Impact

- 依赖通用闭环的实际合同和现有实现；参考 [归档设计](../archive/2026-11-16-deliver-codex-session-loop/design.md)，运行规则以长期规格为准。
- 已有接入面为 app/src/adapters/acp/node_configuration_profile.rs、catalog_draft_document.rs、app/src/node/catalog.rs 与 clients/web/src/management/workspace-catalog-view.ts；不假定旧方案里的 app/src/harnesses/omp/ 已存在。
- 后续新增独立配置示例、使用说明和验收资产；具体写范围见 plan。当前不加依赖、迁移或运行时白名单。

## Exploration

- 已确认：2026-11-16 用户要求先参照旧两项 change 建立 Pi/OMP 规划；Pi 本地已安装仅为用户陈述，安装路径与版本尚未被本会话定位，OMP 安装情况未知。
- 技术候选：候选为 OMP 上游提供的 omp acp 入口；不得以 omp --mode rpc 或交互终端代替 ACP。 来源及风险见 design，未将建议或用户沉默记录为最终依赖选择。
- 旧结构参考：Git 53034ffc0c9f7644a3125788d1d2b98e51619f88 中 integrate-deepseek-harness 与 integrate-zcode-harness；仅复用职责、产物和验收组织，不复制旧方法、版本、过时前置或结果。
- 实施前阻断输入：实际可执行路径、版本/摘要、可用 ACP 入口、隔离认证与会话根、Windows 完整进程树清理办法。允许先创建规划；产品编码和正式适配验收前必须解决。
- 依赖选择若要求新增私有桥接、丢失结构内容或改变安全/数据权威，超出本 change；报告具体差异并单独决定，不通过降级绕过。标准操作不支持时直接报错，不新增运行时用户裁决流程。
- 术语复核：OMP（Oh My Pi） 是具体程序标识，不是新增业务对象；沿用接入来源、启动配置、原生身份及连接代际，GLOSSARY 无需变更。当前不形成新跨层决策，暂不新增 ADR。

# 实施顺序与规划范围

本轮只准备规划，三个变更的实现任务均保持未勾选；现有实现和权威协议文档仍描述当前机制，不能将规划文档当作已交付功能。

| 顺序 | 变更 | 交付内容 | 开始实施的前提 |
| --- | --- | --- | --- |
| 1 | [access-owner-data-stateless](proposal.md) | Access 仅保留连接资料，远程来源流、回源查询和旧数据清理 | 本变更合同检视通过 |
| 2 | [custom-session-title](../custom-session-title/proposal.md) | 顶部双击原地编辑，Owner 保存自定义标题并最高优先展示 | 第 1 项完成实现、迁移、独立检视和端到端验证 |
| 3 | [scoped-offline-page](../scoped-offline-page/proposal.md) | 两段连接故障分别保护、独立离线页及恢复导航 | 前两项完成实现与验证 |

每项均包含 proposal、specs、design、plan、tasks；执行方式遵循 [WORKFLOW](../../../docs/WORKFLOW.md)。旧共享目录、过期实现假设或单纯“文件存在”不算依赖完成证据。

共同边界：Owner 负责资源持久化和命令防重复执行；Access 不持久保存 Owner 资源、请求状态、事件收据或来源进度；运行期通信和渲染所需处理有界且随来源失效清理。节点本身的 owned 会话及连接信任仍按原要求保存。旧端能力不足时明确降级，不能恢复旧缓存方式兼容。

架构取舍见 [ADR-0013](../../../docs/adr/0013-stateless-access-owner-data.md)，用户已确认的问答见 [interview](../custom-session-title/interview.md)。所有计划中的 wire/DDL/端口变化在各变更 WP1/WP2/WP3 同步到机器合同和权威文档，不在本轮提前改代码或已接受规范。

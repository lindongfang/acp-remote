# Proposal

## Why

用户要求统一会话模型，仅保留空闲、运行中、待处理，而非只合并页面标签。现有七态把会话活跃程度、交互原因和任务结果混在一起，`failed`/`closed` 还是不可恢复状态；授权请求的持久化也尚未完整驱动等待状态。

## What Changes

- **BREAKING** 核心、存储、Sync、Node Link 和 PWA 的会话主状态统一为 `idle`（空闲）、`running`（运行中）、`pending`（待处理）；移除会话级 queued/waiting_input/waiting_permission/failed/closed 和关闭时间概念。
- 优先级固定为有效未解决交互 > 未结束任务（包括排队）> 空闲；会话创建成功且未提交任务时为空闲，页面切换不改变状态。
- 一轮任务仍保留排队、执行、等待原因、成功、失败、取消等细节；命令仍保留既有终态与结果无法确认语义，不压成三态。
- 普通消息不解除交互；正式应答、明确撤回或所属任务确认结束才解除。收到单个撤回信号的能力仅在适配器确实支持时提供，不虚构 Agent 能力。
- 失败任务不封死会话；后续排队任务继续按顺序执行。崩溃结果无法确认时不自动重试，旧执行未确认停止前不推进后续任务。
- 断线保留最后确认状态并提示可能已变化；Owner 为状态唯一写入者，Access 不自行推断。
- 提供版本化数据库升级与明确的 wire 兼容边界；保留历史结果与 ACP 原文，不伪造旧授权或旧任务的执行结果。

原始要求与已确认边界：Q1 统一全系统；Q2 排队算运行中；Q3 有待处理请求时优先待处理；Q4 本轮失败不封死会话；Q5 只凭明确事实解除交互；Q6 本轮结束清理所属交互；Q7 断线保留最后确认值；Q8 失败后继续排队任务；Q9 不自动重试结果无法确认的任务。

非目标：不增加关闭、删除或归档会话功能，不改变忙碌时排队/拒绝策略，不实现无信号推断授权撤回，不削弱 ACP、不改变认证授权和 no-content-cache 边界。

成功判据：新建、排队、并发交互、跨设备应答、取消、失败、恢复与重连均产生可重放且一致的三态；失败后可继续提交；旧库升级不丢历史；新旧协议不会互相误解状态。

## Capabilities

### New Capabilities

- `session-activity`: 会话三态的权威判定、交互生命周期、失败与恢复规则以及跨协议一致性。

### Modified Capabilities

- `pwa-web-client`: 目录筛选改为统一三态，状态与任务结果、连接可用性分别呈现。
- `storage-schema-v2-migration`: 三态版本迁移、旧状态归并、重放及恢复约束。

## Impact

涉及 core 的 model/ports/use_cases/broker、storage-sqlite、agent-host、Sync/Node Link DTO 与服务端映射、app 调度与恢复、PWA domain/state/features/components；合同资产含 schemas/fixtures/compatibility 与 Rust/TS 同源校验。

实施时同步 INITIAL_DESIGN、CORE_PORTS_AND_STORAGE、SYNC_PROTOCOL、NODE_LINK_PROTOCOL、FRONTEND_DESIGN、MODULE_ARCHITECTURE 及相关 ACP 矩阵（仅在支持声明变化时）。不新增 core 依赖或 crate。

与在途 `conversation-area-complete` 共享 broker、交互、协议、存储与客户端区域：先以其最终落地版本重定基线，再串行实施本变更，不覆盖其在途修改。该变更的新可恢复交互能力不会自动授权本变更修改历史应答边界。

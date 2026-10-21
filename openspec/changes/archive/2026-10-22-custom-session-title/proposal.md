# Proposal

## Why

用户要求：“会话页，顶部双击标题，支持自定义标题。自定义标题的优先级最高。”当前标题只能来自 Agent 通知，没有标题的 Agent 导致全部显示未命名，且用户无法自行区分会话。

## What Changes

- 顶部双击标题进入编辑，Enter 保存、Esc 取消、点击输入框外保存；不提供恢复或额外编辑入口。
- 标题优先级固定为自定义标题 > Agent 标题 > 未命名会话；Agent 后续更新不能覆盖自定义标题。
- 自定义标题仅由 Owner 持久保存并同步给各客户端；Access 不持久保存标题，相关离线表现另行明确。
- 增加可重试的标题修改命令及能力协商；旧端明确禁用编辑。
- 替换现有“客户端不得重命名”限制，但保持 Agent 原始通知与标题数据保真。
- 非目标：自动生成标题、改写 Agent 内部会话名、目录改名、新增设备授权维度。
- 前置独立变更：[access-owner-data-stateless](../archive/2026-10-21-access-owner-data-stateless/proposal.md) 先交付，移除 Access 的 Owner 数据、命令状态与收据持久化；本变更不得重新引入它们。
- 后续独立变更：[scoped-offline-page](../archive/2026-10-21-scoped-offline-page/proposal.md) 实现离线状态与连接问题弹窗；本变更不新增离线路由。
- 成功判据：双击编辑后所有标题位置一致；Agent 更新、断连重连、Daemon 重启不丢自定义值；不提供清除或恢复，仅可再次修改自定义值。

## Capabilities

### New Capabilities

- `session-custom-title`: 自定义标题的保存、优先级、命令完成、多端及跨节点一致性。

### Modified Capabilities

- `core-derived-events`: 将 Agent 标题更新与用户覆盖值分开，移除禁止客户端改名的约束。
- `pwa-web-client`: 仅顶部双击进入编辑、Enter 和失焦保存、Esc 取消与错误提示。

## Impact

涉及 core、storage-sqlite、Sync/Node Link 协议及适配器、app 组合根、PWA 状态和组件；需要数据库迁移、命令与 feature 登记、共享 schema/fixture 和权威文档同步。不增加依赖。imported 会话仍由 Owner 权威保存，Access 不新增正文缓存。

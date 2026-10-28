# ADR-0002：Node Link 优先、Owner 单点正文与节点级信任

- 状态：Accepted
- 日期：2026-09-18
- 影响范围：MVP 顺序、Node Link、持久化、远程建会话与授权审计

## 背景

ACP Remote 的目标已经从“电脑向手机提供辅助访问”调整为通用节点中转站：拥有 Agent 的 Owner Node 可以把受约束的 Agent 能力导出给 Access Node，Access 再向本地 Zed、PWA、CLI 或其他客户端提供入口。

编码前需要固定首条可验证链路、远程会话创建、远程数据持久化和最终用户身份边界，避免先实现一套只适用于手机遥控的核心。

## 决策

1. 第一个产品纵向切片是 `Zed -> Access Node -> Node Link -> Owner Node -> fake ACP Agent`；Web/PWA 在该链路之后实现。
2. 首个 Node Link 切片支持受限远程 `session.create`，供 ACP facade 将 Zed `session/new` 转换为 Owner 命令。请求只能引用 Export 中的 Agent 和 workspace template，不能携带任意 Owner 路径或 Provider/MCP 凭据。
3. Owner Node 是会话正文的唯一持久化位置。第一阶段 Export 固定 `no-content-cache`；Access 和其远程客户端只持久化连接、来源、cursor/ACK、幂等、命令终态引用、event type/digest 和 local sequence 映射，正文仅作有界内存转发。
4. 第一阶段采用节点级信任。Owner 直接认证并授权 Access Node，Access 负责认证本地客户端；`localPrincipalRef` 只用于审计归因，不是 Owner 直接认证的最终用户身份。

## 结果

- Owner 离线时，Access 不提供 imported 会话正文，也不接受或排队远程命令。
- Access 重启后依靠无正文交付索引和 origin cursor 向 Owner 回源重放。
- Zed 可以通过标准 `session/new` 创建受 Owner Export Policy 限制的会话。
- `no-content-cache` 不能约束 Zed 等第三方 ACP Client 自身的存储；允许其接收正文属于显式数据披露，需依赖受管终端策略控制第三方留存。
- Owner 可按 Access Node 授权、限流和撤销，但第一阶段不提供员工级端到端身份或不可抵赖审计。
- 若未来需要离线正文缓存或 Owner 直接认证最终用户，必须新增协商 feature、安全设计和独立 ADR。

# Spec Delta

## Purpose

定义访问节点面向本节点客户端提供远程会话的行为，通过经过身份认证和能力协商的节点连接转发会话读取、命令、事件与附件，保持资源所属节点的唯一权威以及访问节点不保存远程正文的边界。

## ADDED Requirements

### Requirement: 出站连接与远程会话能力交集

Access SHALL 建立并管理到 Owner 的已认证 Node Link 连接，支持重连、能力协商与撤销。远程会话 SHALL 提供实际端到端能力交集内的读取、命令、审批、提问及附件操作；MUST NOT 使用 Access 的本地文件路径代替 Owner 的工作目录路径，MUST NOT 重新导出 imported Agent。

#### Scenario: Owner 缺少附件能力
- **WHEN** 本地客户端支持附件但 Owner 未协商该能力
- **THEN** 远程会话明确不支持附件，文本能力按实际交集保留

### Requirement: 远程命令保持稳定身份与结果

可重试远程命令 SHALL 保持稳定请求标识及来源身份；Access MUST NOT 因连接断开重新执行无法确定结果的副作用。命令受理、完成、失败与结果不确定 SHALL 分别呈现；Owner 的资源范围、导出授权及实际能力限制 SHALL 在执行前校验。

#### Scenario: 接受后连接中断
- **WHEN** Owner 已接受消息而 Access 未收到结果即断线
- **THEN** 恢复时查询或重放原结果，不产生第二次 Agent 执行；无法确定时显示结果不确定

### Requirement: 无正文交付与回源历史

Owner SHALL 是事件、消息、思考、附件与命令结果的权威；Access SHALL 在广播 imported 事件前提交无正文交付收据。正文仅在内存转发且 MUST NOT 落入 Access 数据库、临时文件、日志或持久缓存。历史 SHALL 在线向 Owner 读取，Owner 不可达时 SHALL 明确不可用而非返回空历史。

#### Scenario: 事件重放与本地重复投递
- **WHEN** 同一 Owner 事件在重连后再次到达
- **THEN** Access 保持稳定来源与交付映射，客户端不重复呈现，Access 磁盘没有远程正文

#### Scenario: 离线后重新可达
- **WHEN** Owner 离线后恢复连接
- **THEN** Access 依据持久化来源游标补回事件并恢复历史读取，不依赖断线前的内存广播队列

### Requirement: 远程附件流向所属节点

远程会话上传和读取 SHALL 经 Access 有界内存转发至 Owner，不写 Access 临时磁盘；Owner SHALL 校验主体、会话和附件归属。撤销或路由失效 SHALL 中止传输并阻止后续提交；已转发上传不等于消息已提交。

#### Scenario: 传输期间撤销节点信任
- **WHEN** 用户向远程会话上传时节点信任被撤销
- **THEN** 传输和后续提交失败，Owner 不执行残缺消息，Access 没有附件落盘

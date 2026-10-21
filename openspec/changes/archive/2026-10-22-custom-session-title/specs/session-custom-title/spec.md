## Purpose

定义用户为会话设置共享自定义标题的行为，以及它相对于 Agent 标题的最高展示优先级。标题由 Owner 持久保存，远程访问沿用无资源存储的 Access 路径，命令结果与多端标题均以 Owner 提交为准。

## ADDED Requirements

### Requirement: 自定义标题由会话所属节点保存

系统 SHALL 支持设置和修改自定义标题，MUST NOT 提供清除或恢复 Agent 标题操作；按自定义标题、Agent 标题、未命名会话的优先级统一呈现。标题 SHALL 由会话所属节点持久保存并同步至所有获准访问的客户端；Access MUST NOT 成为远程标题的第二权威。刷新、重新认证及节点重启 MUST NOT 丢失已提交标题。

#### Scenario: 保存后多端重连
- **WHEN** 一个客户端保存自定义标题，其他客户端重连或所属节点重启
- **THEN** 对话页、会话列表与搜索使用同一个有效标题

#### Scenario: imported 会话修改
- **WHEN** 已配对、未撤销且具备端到端实际能力的 Access 请求设置远程会话标题
- **THEN** 修改在 Owner 提交并向获准客户端同步；Access 不持久保存标题、命令状态或转发进度，Owner 负责防止同一请求重复执行

#### Scenario: 标题仅由 Owner 保存
- **WHEN** Access 接收远程会话标题
- **THEN** Access 不将标题持久保存；Owner 保存的值仍是唯一权威，不提供远程数据离线展示能力，离线页由独立变更交付

#### Scenario: 旧客户端显示自定义标题
- **WHEN** 不支持编辑的新旧客户端读取已自定义的会话标题
- **THEN** 既有标题字段呈现有效自定义值，不因客户端版本退回 Agent 标题

### Requirement: 修改命令可靠完成并明确失败

修改 SHALL 使用稳定 requestId，重复请求 MUST NOT 重复执行；提交、命令终态与事件 SHALL 原子完成并先于广播。标题 SHALL 去除首尾空白、拒绝空串及超过 512 个 Unicode 标量值的输入，MUST NOT 自动截断或接受 null 清空。并发修改 SHALL 按所属节点提交顺序后写生效，其他设备更新 MUST NOT 打断编辑中草稿。未协商能力或未获授权 MUST 明确拒绝，MUST NOT 假装保存成功。

#### Scenario: 同一请求重试
- **WHEN** 保存请求断连后用相同标识重试
- **THEN** 返回既有结果且不重复修改版本或追加事件

#### Scenario: 旧端与非法输入
- **WHEN** 对端未协商能力或标题不满足限制
- **THEN** 不改变原标题，返回明确的不支持或输入错误

### Requirement: 相同文字也可确立自定义标题

当会话尚无自定义标题时，用户明确保存与 Agent 标题相同的文字 SHALL 确立自定义优先级，后续 Agent 改名 MUST NOT 覆盖。已有自定义值时保存同值 MUST NOT 因 Enter 与失焦重复触发产生第二次修改；输入去除首尾空白后的值 SHALL 作为保存值。

#### Scenario: 相同文字固定标题
- **WHEN** Agent 标题为“排查登录”，用户双击后原样保存，随后 Agent 改名
- **THEN** 有效标题仍为“排查登录”，不能因为文字未变化而忽略用户命名意图

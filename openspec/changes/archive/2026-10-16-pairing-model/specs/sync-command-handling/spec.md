# Spec Delta

## ADDED Requirements

### Requirement: 命令以已认证设备为前提

服务端 SHALL 以「该连接已通过设备认证」为执行任何业务命令的前提，设备方向不再有命令级授权判定（设备配对不授予授权）。未通过认证的连接 MUST NOT 执行任何业务命令，且 MUST NOT 因命令内容而产生副作用。`session.resume` 不属于 Sync 命令枚举，收到时 MUST 返回 `command.unsupported`。

#### Scenario: 未认证连接不执行命令

- **WHEN** 一个尚未完成设备认证的连接派发业务命令
- **THEN** 服务端以认证类失败拒绝，MUST NOT 启动 Agent、MUST NOT 写入事件、MUST NOT 改变会话状态

#### Scenario: 拒绝先于本机读取

- **WHEN** 未认证或已撤销的设备对某个不存在的会话派发命令
- **THEN** 返回的仍是认证类失败，MUST NOT 通过错误类型泄露该会话是否存在

#### Scenario: Sync 面不接受仅属其它传输面的命令

- **WHEN** 客户端在 Sync 面派发 `session.resume`
- **THEN** 服务端返回 `command.unsupported`，MUST NOT 在本机执行该操作

#### Scenario: 撤销后连接不再可用

- **WHEN** 某设备被撤销后，其连接又派发一条命令
- **THEN** 该命令被拒，MUST NOT 因连接仍在而继续可用

## REMOVED Requirements

### Requirement: 逐命令的 scope 授权与零副作用拒绝

**Reason**：设备配对不再授予授权（`pairing-model` 删除设备权限层），设备方向因此不存在命令级 scope 判定。该要求描述的判定维度、`authorization.scope_denied` 拒绝与「缩减 scope 立即生效」都不再成立。

**Migration**：设备方向的执行前提改为「该连接已通过设备认证」（见本 delta 的新增要求「命令以已认证设备为前提」）；跨节点方向仍按 Export grant 判定，不受本删除影响。

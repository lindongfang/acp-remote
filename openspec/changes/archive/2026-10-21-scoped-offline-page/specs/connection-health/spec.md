# Spec Delta

## Purpose

定义客户端在认证完成后如何用应用层 ping/pong 判断连接是否仍然可用，并把判定结果交给既有连接状态机与重连机制。

## ADDED Requirements

### Requirement: 认证后每秒探活

客户端 SHALL 在一条连接完成认证后、页面可见时，每 1000 毫秒发送一条 `control.ping`。每条 ping MUST 使用新的 16 字节随机 nonce 并以无填充 base64url 表示；MUST 复用现有 `control` 消息，MUST NOT 新增 wire 消息类型。认证前、未配对、配对中或页面隐藏时 MUST NOT 发送探活 ping。探活 MUST NOT 修改服务端 30 秒心跳或 90 秒静默超时。

#### Scenario: 在线空闲时持续探活

- **WHEN** 连接已认证且页面可见，业务上没有新事件
- **THEN** 客户端每秒发送一条带新 nonce 的 `control.ping`，并接收服务端原样回填的 `control.pong`

#### Scenario: 未认证不发探活

- **WHEN** 连接仍处于 `connecting` 或 `authenticating`
- **THEN** 客户端不发送 `control.ping`，也不据此宣称在线或离线

#### Scenario: 页面隐藏时暂停

- **WHEN** 页面变为隐藏
- **THEN** 客户端停止发送探活 ping，且不因探活静默而判定离线

### Requirement: 只有匹配的 pong 构成健康证据

客户端 SHALL 只把 nonce 与本连接一条未回应 ping 匹配的 `control.pong` 视为健康证据。业务事件、服务端发来的 `control.ping`、命令结果、快照分块或其它入站消息 MUST NOT 重置探活计时。最老一条未回应的 ping 超过 3000 毫秒仍未收到匹配 pong 时，客户端 SHALL 判定当前连接离线。

#### Scenario: 业务事件不能掩盖无 pong

- **WHEN** 服务端持续推送业务事件，但连续 3 秒没有回应客户端的探活 ping
- **THEN** 客户端仍判定离线，不把业务事件当作 pong 的替代

#### Scenario: 匹配 pong 重置计时

- **WHEN** 客户端收到 nonce 与未回应 ping 相同的 `control.pong`
- **THEN** 清除该条未回应记录并重置健康计时

#### Scenario: 不匹配的 pong 被忽略

- **WHEN** 客户端收到 nonce 不属于本连接未回应 ping 的 `control.pong`
- **THEN** 不把它当作健康证据，也不触发任何状态变化

### Requirement: 探活超时进入既有离线路径

探活超时后，客户端 SHALL 在本地结束当前连接，并通过既有连接事件流报告一次可重试的 `closed`，使连接状态机进入 `reconnecting`。它 MUST NOT 伪造服务端关闭、命令结果或 `uncertain` 终态；MUST NOT 自动重发任何 mutation；未确认的命令仍按既有命令终态规则处理。

#### Scenario: 静默断网

- **WHEN** 网络静默消失，3 秒内没有任何匹配 pong
- **THEN** 客户端关闭本地 socket，进入 `reconnecting`，并开始既有退避重连

#### Scenario: 超时期间有待确认命令

- **WHEN** 探活超时时存在已派发但未收到终态的命令
- **THEN** 客户端不自行判定成功或失败，也不生成新的 `requestId` 重发

### Requirement: 可见性恢复有宽限

页面从隐藏恢复为可见时，客户端 SHALL 立即发送一条探活 ping，并在约 3000 毫秒宽限内不因隐藏期间的静默判定离线。宽限内收到匹配 pong 即恢复正常探活；宽限结束仍无匹配 pong 才进入离线路径。

#### Scenario: 后台节流后切回

- **WHEN** 页面在后台停留较长时间后重新可见
- **THEN** 客户端先补发一次探活，并在宽限内等待 pong，不立即显示离线

### Requirement: 重连节奏保持不变

普通离线 SHALL 沿用既有指数退避（500 毫秒起步，上限 30 秒），MUST NOT 设置硬性最大重连次数；连接恢复在线后 SHALL 清零重连计数。探活在每次重新认证完成后重新开始。界面 SHALL 能读取当前重连次数与下次重试时间。

#### Scenario: 长时间断网

- **WHEN** 网络持续不可用
- **THEN** 客户端按 500ms → 1s → 2s → … → 30s 的节奏持续重试，不停机等待用户操作

#### Scenario: 恢复后清零

- **WHEN** 连接重新进入 `online`
- **THEN** 重连计数清零，下一次断线从最短退避重新开始

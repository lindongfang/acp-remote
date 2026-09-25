## Purpose

定义 CLI 与运行中 Daemon 之间本地管理通道的传输行为：endpoint 位置与 OS 用户级访问控制、字节 framing 与上限、管理载荷与 ACP 流两类 channel 的用途绑定，以及连接级失败关闭规则，确保本机之外的任何主体无法触达管理面。

## ADDED Requirements

### Requirement: endpoint 位置与创建

Daemon SHALL 在平台约定的位置创建本地 endpoint：Windows 为 Named Pipe `\\.\pipe\acp-remote-<userSidHashHex>-<instanceId>`，Unix 为 `$XDG_RUNTIME_DIR/acp-remote/<instanceId>.sock`（目录 `0700`、socket 文件 `0600`），`XDG_RUNTIME_DIR` 未设置时回落到 `<daemon.data_dir>/run/<instanceId>.sock` 且目录权限同为 `0700`。`<userSidHashHex>` MUST 为当前 OS 用户标识 SHA-256 摘要的小写十六进制前 16 字符（Windows 取 SID 字符串 UTF-8 字节、Unix 取十进制 UID UTF-8 字节，不附加换行或前缀）。本通道 MUST NOT 监听 TCP、使用 HTTP 或引入本地 token/设备身份。

#### Scenario: Windows 上创建 Named Pipe

- **WHEN** Daemon 在 Windows 启动
- **THEN** Named Pipe 按上述命名规则创建，pipe 名仅用于定位，不作为授权手段

#### Scenario: Unix 上创建 socket 且权限正确

- **WHEN** Daemon 在 Unix 启动且 `XDG_RUNTIME_DIR` 已设置
- **THEN** socket 目录权限为 `0700`、socket 文件权限为 `0600`；权限无法保证时拒绝启动

### Requirement: OS 用户级访问控制

endpoint SHALL 只允许启动 Daemon 的同一 OS 用户连接（Windows 使用仅含该用户 SID 的 SDDL；Unix 使用目录与文件权限）。连接建立后 MUST 校验对端凭据：Windows 经客户端进程 token 取 SID 与 Daemon 用户 SID 比对，Unix 经 `SO_PEERCRED` 取 uid 比对。凭据不一致时 MUST 不发送任何 frame、立即关闭连接，并记录一条 `authorization.denied` 审计事件。

#### Scenario: 同一用户连接被接受

- **WHEN** 与 Daemon 同一 OS 用户的进程连接 endpoint
- **THEN** 连接通过 endpoint 权限与对端凭据两层校验，可发送第一条 frame

#### Scenario: 跨用户连接被拒绝

- **WHEN** 其他 OS 用户的进程连接 endpoint（endpoint 权限层未能拦截的场景）
- **THEN** Daemon 不发送任何 frame、立即关闭连接，并新增一条 `authorization.denied` 审计记录

### Requirement: framing 与帧上限

每个 frame SHALL 为 `u32be length | payload`，`length` 只计 payload 字节数且 MUST 不超过 1 MiB；payload 第 1 字节为 channel（`0x01` 管理 JSON、`0x02` ACP 字节流）。`length == 0`、长度超限或 channel 未知时 MUST 关闭连接。framing 错误一律不返回错误帧，因为此时链路本身不可信。

#### Scenario: 合法帧被处理

- **WHEN** 已认证连接发送 `0x01` channel 的合法长度帧
- **THEN** 帧被完整读取并交给管理载荷处理

#### Scenario: 空帧或超长帧关闭连接

- **WHEN** 连接发送 `length == 0` 或 `length > 1048576` 的帧头
- **THEN** Daemon 关闭连接且不返回任何错误帧

#### Scenario: 未知 channel 关闭连接

- **WHEN** 首帧 channel 字节不是 `0x01` 或 `0x02`
- **THEN** Daemon 关闭连接且不返回任何错误帧

### Requirement: channel 用途绑定

第一条 frame 的 channel SHALL 决定连接用途：管理连接可在一次会话内承载多个请求/响应，`0x02` ACP 连接是长期双向流；同一连接上出现与首帧不同的 channel 时 MUST 关闭连接。`0x02` 连接建立即分配新的 `FacadeAttachmentId`，连接结束即作废；该标识 MUST NOT 与 Node Link 的 `attachmentId` 共用字段或跨层传递。本切片 `0x02` 只到连接级处理：facade 语义缺席时 MUST 以 design 定案的方式明确失败（不静默吞字节、不伪造 ACP 响应）。

#### Scenario: channel 混用被拒绝

- **WHEN** 连接首帧为 `0x01`，随后出现 `0x02` 帧（或反之）
- **THEN** Daemon 关闭连接且不返回错误帧

#### Scenario: `0x02` 连接的 attachment 生命周期

- **WHEN** 客户端建立一条 `0x02` 连接，断开后再建立新连接
- **THEN** 两次连接分配不同的 `FacadeAttachmentId`；旧 attachment 作废后到达的延迟帧被丢弃并记结构化警告，不命中新绑定

#### Scenario: facade 缺席时的 `0x02` 失败

- **WHEN** 本切片内 Daemon 接受 `0x02` 连接但 `server::acp_facade` 尚未实现
- **THEN** 连接按 design 定案的方式明确失败（明确关闭或明确错误），字节不被静默消费

### Requirement: 未完成请求上限

同一管理连接上同时最多 32 条未完成请求；超出时 MUST 视为协议滥用并关闭连接。`0x02` 连接上未完成的上游 ACP 请求上限同为 32（facade 落地时执行）；单方向未消费字节上限 1 MiB，达到上限时对应方向的读取方 MUST 停止拉取，不得无界缓存。

#### Scenario: 超出未完成请求上限

- **WHEN** 客户端在同一管理连接上未等响应连续发送第 33 条请求
- **THEN** Daemon 关闭该连接，已完成的响应不受影响

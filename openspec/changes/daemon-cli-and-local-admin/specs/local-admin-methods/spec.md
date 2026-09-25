## Purpose

定义本地管理通道上管理信封的校验规则、v1 方法集的请求/结果语义、`local.*` 错误码映射与重试/失败语义，使 CLI 能经同一套 core 用例完成 workspace、Agent、Provider、设备/节点配对、Export/Import 与审计导出管理。

## ADDED Requirements

### Requirement: 管理信封校验

管理请求 SHALL 为 closed object `{ "v": 1, "id": "<uuid>", "method": "<name>", "params": {…} }`；`v != 1` 时 MUST 关闭连接而不返回错误帧。信封非法（`id` 缺失/非 UUID/同连接重复、`method` 缺失或不匹配 `^[a-z][a-z0-9]*(\.[a-z0-9]+)*$`、`params` 不是 object）时返回 `local.invalid_request`；未知 `method` 返回 `local.unsupported`；`params` 缺字段、类型不符或含未知字段返回 `local.invalid_params`。每个请求 MUST 恰好产生一个响应并回带相同 `id`；响应 `error` 只含 `code` 与 `message`，`message` MUST 为简短英文描述且不含 secret、凭据值、堆栈或完整敏感路径；客户端 MUST 忽略未知 `result` 字段。

#### Scenario: 合法请求获得唯一响应

- **WHEN** 已认证管理连接发送合法信封与已知方法
- **THEN** 恰好收到一个响应，`id` 与请求相同，`ok` 判别成功或失败

#### Scenario: 未知方法与参数错误分别报错

- **WHEN** 分别发送未知 `method`、缺必填字段的 `params`、含未知字段的 `params`
- **THEN** 依次返回 `local.unsupported`、`local.invalid_params`、`local.invalid_params`，连接保持可用

#### Scenario: 版本未知即关闭连接

- **WHEN** 请求信封的 `v` 为 `2`
- **THEN** Daemon 关闭连接且不返回错误帧

### Requirement: 本地配置方法

`workspace.select` SHALL 校验 `alias`（`^[a-z0-9][a-z0-9._-]{0,63}$`）、`displayName`（≤128）与 `rootPath`：`rootPath` MUST 为已存在的绝对路径目录，写入前先 canonicalize 并以规范化结果为持久权威值，拒绝相对路径与含 `..` 的输入；同 `alias` 再调用为更新。`agent.configure` SHALL 写入 Agent profile（`envAllowlist` 是注入上限，`command` 不经 shell 拼接），变更只影响后续启动的 Agent 进程。`provider.configure` SHALL 只把凭据值写入平台 keystore，result 只回 `configuredFields` 字段名而永不回显 `values`；keystore 不可用时正式模式返回 `local.unavailable`，不得降级明文存储。

#### Scenario: workspace.select 规范化路径

- **WHEN** 以含符号链接或大小写差异的绝对目录路径调用 `workspace.select`
- **THEN** 持久化的是 canonicalize 后的路径，result 返回 `alias`/`displayName`/`rootPath`/`createdAt`

#### Scenario: workspace.select 拒绝非法路径

- **WHEN** `rootPath` 为相对路径、含 `..`、不存在或不是目录
- **THEN** 返回 `local.invalid_params`，不产生任何写入

#### Scenario: provider.configure 不回显凭据

- **WHEN** 以合法 `values` 调用 `provider.configure`
- **THEN** result 只含 `providerId`/`kind`/`configuredFields`；凭据值出现在 keystore，不出现在 SQLite、日志、错误消息或任何响应中

#### Scenario: keystore 不可用时失败关闭

- **WHEN** 正式模式下平台 keystore 不可用，调用 `provider.configure`
- **THEN** 返回 `local.unavailable`，凭据不以任何形式落盘

### Requirement: 设备配对与信任方法

`device.pair.begin` SHALL 创建一次性配对并返回 `pairingId`、`pairingUrl`、`expiresAt` 与展开后的 `scopes`（有效期固定 5 分钟，`expiresInMs` 只能收窄）；`pairingUrl` 只经本通道返回，不得进入日志。`device.pair.status` 在 claim 前只返回 `state` 与 `expiresAt`，claim 后返回名称、指纹、6 位 SAS 与请求 scopes。`device.pair.confirm` MUST 在持久状态提交成功后才返回，且以用户确认的最终 `scopes` 为准；`device.pair.reject` 终结配对。`device.list` 返回含 `pending`/`revoked` 的全部记录；`device.revoke` MUST 在持久提交后立即关闭该设备的 active connection 才返回。

#### Scenario: 设备配对全流程

- **WHEN** 依次调用 `device.pair.begin`、设备端完成 claim 后轮询 `device.pair.status`、用户确认后调用 `device.pair.confirm`
- **THEN** begin 返回 URL 与展开 scopes；status 在 claim 前只暴露 `state`/`expiresAt`、claim 后暴露指纹与 SAS；confirm 提交成功后设备记录为 `active`

#### Scenario: 过期与拒绝不创建信任

- **WHEN** 配对超过 5 分钟未确认，或对已过期/已拒绝的 `pairingId` 调用 `confirm`
- **THEN** 返回 `local.expired`，不创建任何设备信任记录

#### Scenario: 撤销立即生效

- **WHEN** 对 `active` 设备调用 `device.revoke`
- **THEN** 持久状态提交后该设备的 active connection 被关闭，方法才返回 `{ deviceId, revokedAt }`

### Requirement: 节点配对与信任方法

`node.pair.begin mode = "owner"` SHALL 在本节点创建一次性配对并返回二维码 URL；`node.pair.confirm` MUST 在持久提交后才返回，只有 Owner 侧本地确认才创建信任记录并分配初始 `grant.*`。`node.revoke` 后该节点 MUST NOT 再建立 Node Link 连接，本地停止重连并关闭 active connection。本切片 `mode = "access"`（需对 Owner 的 HTTPS claim）与 `node.rotate-key.begin` MUST 返回 `local.unsupported`，不得自行填充参数形状或伪造 claim。

#### Scenario: Owner 模式节点配对

- **WHEN** 以 `mode = "owner"` 调用 `node.pair.begin`，对端 claim 后经 `node.pair.status` 观察，本机用户确认后调用 `node.pair.confirm`
- **THEN** confirm 提交成功后节点记录为 `paired` 并带初始 grants；拒绝或过期路径不创建信任

#### Scenario: access 模式明确不支持

- **WHEN** 本切片内以 `mode = "access"` 调用 `node.pair.begin`，或调用 `node.rotate-key.begin`
- **THEN** 返回 `local.unsupported`，无状态变更、无对外网络请求

### Requirement: Export 与 Import 管理方法

`export.create` SHALL 校验 `ExportView` 全部必填字段：首切片 `agentIds`/`workspaceAliases`/`templates` 恰好 1 项、`default*` 一致性、`templates[].params` 必须为空（非空返回 `local.invalid_params`）、每个 `workspaceAliases[].alias` 必须已在本地 workspace 建立（否则 `local.not_found`）；`exportId` 已存在返回 `local.conflict`。`export.revoke` MUST 在持久提交后才返回。`import.add` 在本切片无 Node Link catalog 快照，MUST 返回 `local.unavailable`；`import.list`/`import.remove` 正常作用于本地记录，`import.remove` 不存在时返回 `local.not_found`。Export 的创建、修改与撤销 MUST NOT 经任何远程协议暴露。

#### Scenario: 创建 Export 成功

- **WHEN** workspace 已建立，以合法单 Agent、单 workspace alias、无参数 template 调用 `export.create`
- **THEN** result 返回完整 `ExportView`（`revokedAt` 为 `null`），记录持久化

#### Scenario: 有参数 template 被拒绝

- **WHEN** `export.create` 的 `templates[].params` 非空
- **THEN** 返回 `local.invalid_params`，不得静默忽略参数

#### Scenario: import.add 无快照时明确不可重试语义

- **WHEN** 本切片内调用 `import.add`（无论参数是否合法）
- **THEN** 返回 `local.unavailable`（可重试语义），不创建 Import 记录

### Requirement: 审计导出方法

`audit.export` SHALL 在 Daemon 内按时间区间与类别过滤审计记录，按记录时间升序写出 `jsonl`（每行一个对象）或 `csv`（首行列名），result 返回 `outputPath`、`recordCount` 与导出文件的 sha256 小写十六进制摘要。输出文件已存在时返回 `local.conflict`。导出内容 MUST 只含 `SECURITY_DESIGN.md` §14.2 的元数据字段，不得包含 prompt、回复、diff、终端内容、ACP `rawJson`、附件、凭据、pairing secret 或 QR payload。

#### Scenario: 导出审计记录

- **WHEN** 以合法 `outputPath`、`format`、时间区间与类别调用 `audit.export`
- **THEN** 生成升序排列的导出文件，result 的 `sha256` 与文件内容一致

#### Scenario: 输出路径冲突

- **WHEN** `outputPath` 指向已存在的文件
- **THEN** 返回 `local.conflict`，已有文件不被覆盖

### Requirement: 重试与失败语义

`* .list` 与 `* .status` 类方法 MUST 幂等可安全重复；`device.pair.begin`/`node.pair.begin` 每次调用创建新配对，旧配对自然过期；`export.create`/`import.add` 重试得到 `local.conflict`，`*.revoke`/`*.remove` 重试得到 `local.not_found`。`local.*` 错误码 MUST 匹配 `^[a-z][a-z0-9]*(\.[a-z0-9]+)*$`，与 Sync/Node Link 错误码空间不重叠，且不复用那两个协议的码。连接中断时通道侧不得代答或代提交任何 mutation。

#### Scenario: mutation 重试得到确定性错误

- **WHEN** 对已撤销的 Export 再次调用 `export.revoke`，或以相同 `exportId` 再次调用 `export.create`
- **THEN** 分别返回 `local.not_found` 与 `local.conflict`，状态不发生变化

#### Scenario: 审计事件覆盖拒绝与撤销

- **WHEN** 发生凭据校验拒绝连接、方法失败、设备/节点/Export 撤销与配对批准
- **THEN** 每类都产生 `SECURITY_DESIGN.md` §14.2 对应类别的审计事件，日志与审计不含 §14.1 禁止的字段

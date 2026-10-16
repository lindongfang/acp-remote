# sync-workspace-browse Specification

## Purpose

让已认证的设备能在节点上逐层查看目录，从而在创建会话时选定一个工作目录——只读取目录结构，不登记、不读取文件内容，也不返回文件。

## Requirements

### Requirement: 目录浏览命令与前提

系统 SHALL 提供一条 Sync 面的查询命令 `workspace.browse`，用于逐层查看节点上的目录。该命令 SHALL 以「该连接已通过设备认证」为唯一前提，MUST NOT 引入新的授权维度。该命令 SHALL 受 feature `core.workspace-browse.v1` 门控：未协商该 feature 的连接提交该命令时 MUST 返回 `protocol.feature_required`，且 MUST NOT 触碰文件系统。

#### Scenario: 未认证的连接被拒

- **WHEN** 一个尚未完成设备认证的连接提交 `workspace.browse`
- **THEN** 返回认证类失败，且不读取任何目录

#### Scenario: 未协商 feature 时明确拒绝

- **WHEN** 一台已认证但未协商 `core.workspace-browse.v1` 的设备提交 `workspace.browse`
- **THEN** 返回 `protocol.feature_required`，不读取任何目录，也不返回空列表冒充成功

#### Scenario: 已认证设备可浏览

- **WHEN** 一台已认证且协商了该 feature 的设备提交 `workspace.browse`
- **THEN** 该请求同步完成并返回该层的目录内容

### Requirement: 目录浏览的请求与结果形状

请求 SHALL 只含一个父路径字段，取值 SHALL 是绝对路径或 `null`；`null` 表示请求该节点的根列表。结果 SHALL 含该请求的父路径、该层的直接子目录条目与一个截断标记。每个条目 SHALL 只含该子目录的名称、其绝对路径与它是否为已登记 workspace。结果 MUST NOT 含文件条目，也 MUST NOT 含任何文件内容、大小、时间戳或权限信息。条目 SHALL 按名称以确定的顺序排列，使同一层在两次调用中给出相同顺序。

#### Scenario: 请求根列表

- **WHEN** 请求的父路径为 `null`
- **THEN** 结果给出该节点的根列表（例如各盘符或根挂载点），每个条目含名称与绝对路径

#### Scenario: 请求某一层

- **WHEN** 请求的父路径是一个存在且可读的目录
- **THEN** 结果只含该目录的直接子目录，不含文件，也不含更深层的条目

#### Scenario: 结果不含文件内容与元数据

- **WHEN** 检查任意一次浏览的返回内容
- **THEN** 其中不含文件条目、文件内容、字节大小、修改时间或权限位

#### Scenario: 顺序确定

- **WHEN** 对同一层连续提交两次 `workspace.browse`
- **THEN** 两次的条目顺序相同

### Requirement: 目录浏览的形状校验与失败分类

系统 SHALL 对父路径做与既有路径口径一致的形状校验：不是绝对路径、或含父目录引用时 MUST 以参数类错误拒绝，且 MUST NOT 触碰文件系统。形状合法但该层不存在、不可读或不是目录时 MUST 以服务端不可用类错误拒绝。两类失败都 MUST NOT 返回任何条目，也 MUST NOT 回退到任何替代目录（例如父层或根列表）。

#### Scenario: 相对路径与父目录引用是参数类错误

- **WHEN** 请求的父路径是相对路径或含 `..`
- **THEN** 返回参数类错误，且不访问文件系统

#### Scenario: 不可达的层是服务端错误

- **WHEN** 请求的父路径已被删除、是文件或不可读
- **THEN** 返回服务端不可用类错误，不返回任何条目

#### Scenario: 不回退到替代目录

- **WHEN** 请求的某一层不可达
- **THEN** 返回明确失败，MUST NOT 改为返回父层或根列表

### Requirement: 目录浏览的条目上限与截断标记

系统 SHALL 对单层返回的条目数施加上限，取值是固定的 v1 常量而 MUST NOT 是可由用户放宽的配置项。超过上限时系统 SHALL 只返回上限内的条目并把截断标记置为真；MUST NOT 静默丢弃条目而不置标记，也 MUST NOT 因超限而返回整次失败。

#### Scenario: 超出上限时置截断标记

- **WHEN** 某一层的子目录数超过固定上限
- **THEN** 结果返回上限内的条目且截断标记为真，客户端据此可告知用户还有更多

#### Scenario: 未超限时不置截断标记

- **WHEN** 某一层的子目录数在固定上限之内
- **THEN** 截断标记为假

### Requirement: 目录浏览不产生任何登记或持久化副作用

`workspace.browse` SHALL 是只读查询：它 MUST NOT 写入任何登记、缓存或会话状态，MUST NOT 改变本机 workspace 登记表，也 MUST NOT 让被查看的目录因此具备任何后续权限。被查看的目录取得会话归属的唯一路径仍然是创建会话时按已登记别名解析；经浏览选定但未登记的目录创建的会话因此呈现为「未分组」。

#### Scenario: 浏览不登记

- **WHEN** 一台设备浏览了若干层目录
- **THEN** 本机 workspace 登记表不因这些调用发生任何变化

#### Scenario: 浏览不产生审计记录

- **WHEN** 一次 `workspace.browse` 完成
- **THEN** 系统不写入审计记录，现有审计动作词表不因本能力扩张

#### Scenario: 未登记的目录不作为会话归属

- **WHEN** 用户经浏览选定一个未登记的目录并据此创建会话
- **THEN** 该会话的目录归属为「未分组」，系统 MUST NOT 为它补出一个别名

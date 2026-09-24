# acp-wire-protocol Specification

## Purpose

定义 ACP v1 在本仓库 Rust 侧的 wire 合同：JSON-RPC 信封与消息分类、消息方向与 required 字段校验、原始文档的逐字节保真、capability wire 形状、固定 v1 上限，以及未知判别子与扩展方法的显式处理语义。它是本机 Agent 后端与后续 ACP facade 共同消费的边界，不表达任何领域状态或会话生命周期。

## Requirements

### Requirement: 已知消息的解码与方向校验

系统 SHALL 解码固定上游快照中存在的方法、通知与响应，并校验 JSON-RPC 类别、消息方向与规范 required 字段；不合法输入 MUST 在解码边界被拒绝并返回可区分的错误分类，MUST NOT 用默认值补齐缺失字段。

#### Scenario: 合法请求、通知与响应可解码

- **WHEN** 输入是一条在固定上游快照下合法的 ACP 请求、通知或响应（包括 `initialize`、`session/new`、`session/prompt`、`session/update`、`session/request_permission`）
- **THEN** 解码成功，且类别（request/notification/response）、方向（client→agent 或 agent→client）与 required 字段被如实判定

#### Scenario: 缺失必填字段或方向错误被拒绝

- **WHEN** 输入缺少规范 required 字段，或把 agent→client 的方法按 client→agent 方向解码
- **THEN** 解码失败并返回与「结构合法但能力不支持」可区分的错误，不产生部分解码结果

### Requirement: 未知字段、`_meta` 与扩展 payload 的逐字节保真

系统 SHALL 在解析已知消息时保留原始 UTF-8 JSON document 字节；对未知字段、任意层级的 `_meta`、未来协议字段与超出 u64/i64 范围的整数字面量，重新编码后的结果 MUST 与输入逐字节相等。

#### Scenario: 未知字段与超大整数往返字节相等

- **WHEN** 读取 `fixtures/acp/v1/unknown-fields-and-large-integer.json` 与 `fixtures/acp/v1/meta-and-unknown-fields.json` 的原始字节并完成一次「解码 → 再编码」
- **THEN** 输出字节与输入字节逐字节相等，超大整数字面量不被改写为浮点数也不被截断

#### Scenario: 未识别字段不进入公共领域视图

- **WHEN** 一条含未识别字段的消息需要投影为公共领域视图
- **THEN** 原始 document 仍可原样取回，且未识别字段既不被丢弃也不被改写为规范化文本

### Requirement: 未知判别子的可见降级

系统 SHALL 把固定上游快照中不存在的 `sessionUpdate` 判别子按未知事件处理；该类事件 MUST 保留逐字节原文、MUST 对接收方可见、MUST NOT 被静默丢弃、MUST NOT 被降格为普通文本，也 MUST NOT 被当作解码失败。

#### Scenario: 未来 sessionUpdate 保留原文并可辨认

- **WHEN** 一条 `session/update` 使用固定快照中不存在的判别子（`fixtures/acp/v1/future-session-update.json`）
- **THEN** 结果为「未知但可见」的事件并附带逐字节原文，`acp.rawJson` 可被上层取得，且不产生空内容或静默成功

### Requirement: 下划线扩展方法的显式不支持

系统 SHALL 对固定上游快照中不存在的 `_` 前缀扩展方法返回显式的「方法不支持」结果，MUST NOT 转发该类消息、MUST NOT 静默丢弃、MUST NOT 以普通文本答复替代。

#### Scenario: 扩展方法得到显式拒绝

- **WHEN** 收到一条 `_` 前缀方法的消息（`fixtures/acp/v1/extension-method.json`）
- **THEN** 返回与「方法未找到」等价的明确错误，且该消息没有被转发给任何 Agent 进程

### Requirement: 结构化内容不被文本化

系统 SHALL 把 `session/update` 中的结构化内容（tool call、diff、terminal 内容块、权限与 elicitation 请求）按结构化类型解码；MUST NOT 把结构化内容合并为普通文本，MUST NOT 丢失类型判别子与工具调用标识。

#### Scenario: tool call 与 diff 保持结构

- **WHEN** 解码含 tool call 与 diff 的 `session/update`（`fixtures/acp/v1/tool-call-with-diff.json`）
- **THEN** 结果中工具调用标识、状态与 diff 内容块均为结构化字段，文本化输出不被接受为等价结果

### Requirement: capability wire 形状与真实性

系统 SHALL 按固定上游快照的形状解码与编码双方的 capability 声明，并保留未识别的能力字段；系统 MUST NOT 在没有对应实现的情况下产出「已支持」的宣告，也 MUST NOT 以空对象或默认值伪装未实现的可选能力。

#### Scenario: 能力声明往返保真

- **WHEN** 解码并重新编码一份 `initialize` 的 capability 声明（`fixtures/acp/v1/initialize-capabilities.json`，含未识别的能力字段）
- **THEN** 已识别能力被如实读出，未识别字段逐字节保留

#### Scenario: 未实现能力不出现在宣告中

- **WHEN** 上层请求生成能力宣告，而某个可选能力在本变更范围内没有实现
- **THEN** 该能力不出现在宣告里，且不以默认值、空对象或占位符表达为已支持

### Requirement: 固定 v1 消息上限与失败关闭

系统 SHALL 对单条 ACP 消息实施 1 MiB 的解析上限（固定 v1 常量，不是配置键）；超限输入 MUST 失败且不保留部分结果，消息处理 MUST 有界，MUST NOT 缓存完整会话正文。

#### Scenario: 超限消息被拒绝

- **WHEN** 一条 JSON-RPC 消息的字节长度超过 1 MiB
- **THEN** 解码失败并返回大小类错误，不产生部分解码结果，也不把该消息作为会话正文缓存

### Requirement: 夹具与矩阵驱动的契约证据

系统 SHALL 由 `fixtures/acp/v1/manifest.json` 与 `compatibility/acp/v1/matrix.json` 驱动解码、编码与保真测试，并在测试中引用矩阵登记的条目标识；新增或修改夹具与矩阵时 MUST 在同一变更内同步两者的登记与对应文档，MUST NOT 在实现侧维护另一份独立样例或隐式支持列表。

#### Scenario: 夹具用例全部真实执行

- **WHEN** 运行 ACP wire 契约测试
- **THEN** `manifest.json` 中每条 `valid` 用例成功解码、每条 `invalid` 用例按其声明的约束失败；存在未执行或被跳过的用例时不得判定通过

#### Scenario: 矩阵不变量有对应证据

- **WHEN** 检查矩阵 `invariants` 组的某项（`byte_exact`、`structured_not_text`、`explicit_unsupported`、`truthful_negotiation`、`visible_degradation`）
- **THEN** 存在引用同一条目标识的 Rust 测试并输出原始证据，且实现不存在矩阵未登记的支持项

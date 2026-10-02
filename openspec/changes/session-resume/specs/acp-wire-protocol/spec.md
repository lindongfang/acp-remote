<!-- 已有能力（acp-wire-protocol）的增量：省略 ## Purpose，只写需求变化。 -->

## ADDED Requirements

### Requirement: `session/resume` 的类型化解码与往返保真

系统 SHALL 把 `session/resume` 请求解码为类型化 DTO，字段为 required 的 `sessionId`（Agent 侧会话标识）与 required 的 `cwd`（绝对路径字符串）；响应 SHALL 解码为类型化 DTO。重编码时，未识别字段与 `_meta` 扩展 MUST 逐字节保真，MUST NOT 被丢弃、重排或经通用 JSON 值改写。缺失或类型不符的 required 字段 MUST 在解码边界被拒绝并返回可区分的错误分类，MUST NOT 用默认值补齐。

`session/load`、`session/list`、`session/delete`、`session/close` 的既有处理 MUST NOT 因本需求改变；它们继续保持「已知但未实现」的显式不支持语义。

#### Scenario: 正常解码并回写未知字段

- **WHEN** 解码一个携带 `sessionId`、`cwd` 以及上游快照未定义字段与 `_meta` 的 `session/resume` 请求
- **THEN** 得到类型化的 `sessionId` 与 `cwd`，重新编码后的字节与输入逐字节相同，未知字段与 `_meta` 原样保留

#### Scenario: 缺少 required 字段被拒绝

- **WHEN** 解码缺少 `cwd`（或 `cwd` 不是字符串、`sessionId` 缺失）的 `session/resume` 请求
- **THEN** 在解码边界返回可区分的错误分类，不构造 DTO，也不向下游传递补全后的请求

#### Scenario: session/load 仍为显式不支持

- **WHEN** 系统遇到 `session/load` 请求
- **THEN** 其处理与本次变更前一致（显式不支持），不因为新增了 `session/resume` 而被一起提升或降级为未知方法

# core-event-provenance Specification

## Purpose
定义 Owner 事件库里每条事件的**来源**（谁产生的）与**因果**（哪条命令导致的）如何被如实记录，以及来源词表能表达的生产者集合。它让下游（Sync 的设备事件流、Node Link 的归属诊断）能按事件自身回答「这条是谁产生的」，而不必从旁路推断或猜测。

## Requirements

### Requirement: 事件的来源反映真实产出者

每条事件持久化的 `origin.kind` SHALL 描述**产出该事件的主体**，取值 SHALL 为 `agent`、`device`、`local_cli`、`daemon`、`node` 之一：`agent` 为 ACP 侧 Agent 产生，`device` 为本机已授权设备经命令产生，`local_cli` 为本机 CLI 产生，`daemon` 为 Daemon 自身产生，`node` 为另一个节点经 Node Link 命令产生。服务端 MUST NOT 在无法确定产出者时默认记为 `device`，也 MUST NOT 按「有没有会话或 actor」推断该取值。

#### Scenario: 命令类事件按诱发命令的执行者分类

- **WHEN** 三条 turn 生命周期事件分别由本机 CLI 的命令、本机已授权设备的命令、另一个节点的命令诱发
- **THEN** 三者分别记为 `local_cli`、`device`、`node`，MUST NOT 都记为 `device`

#### Scenario: 诱发命令的执行者已离开作用域

- **WHEN** 提交一条命令类事件时，诱发它的命令的执行者已不在提交作用域内
- **THEN** 该事件的 `origin.kind` 仍取该诱发命令的执行者，MUST NOT 因「当前没有 actor」而记为 `device`

### Requirement: 由命令产生的领域事件携带因果 requestId

由命令产生的领域事件 SHALL 持久化 `causation` 为该命令的 `requestId`，**包括该命令诱发的 turn 内由 Agent 产出的消息与 delta 类事件**；没有命令直接导致的事件其 `causation` SHALL 为 `null`。`causation` 与 `origin.kind` 表达不同的事实，MUST NOT 互相替代。

#### Scenario: turn 内 Agent 产出的事件带因果

- **WHEN** 设备以 `session.prompt` 命令诱发一个 turn，Agent 在该 turn 内产出消息事件
- **THEN** 这些消息事件的 `origin.kind` 为 `agent`，且 `causation` 为该 prompt 命令的 `requestId`

#### Scenario: 无命令导致的事件因果为空

- **WHEN** Agent 在没有任何命令处于作用域时产出一条事件
- **THEN** 该事件的 `causation` 为 `null`

### Requirement: 来源词表扩展保留既有数据

当 `origin.kind` 的取值集合扩展时，存储层 SHALL 通过版本化 migration 扩展其取值约束，并 SHALL 在升级后保留既有事件的逐行内容不变。

#### Scenario: 旧库升级

- **WHEN** 一个只含原有四种 `origin.kind` 取值的旧事件库被升级
- **THEN** 升级后既有事件逐行内容一致，且扩展后的新取值可被写入

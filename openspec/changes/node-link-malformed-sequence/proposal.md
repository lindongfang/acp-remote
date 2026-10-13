# Proposal

## Why

Node Link 的入站序号判定对**形状类拒绝**不推进「入站期望序号」（`crates/server/src/node_link/conn/wire.rs` 约 118-134：`Malformed` 一族与 `ConnectionFieldsForbidden` 一律返回 `None`）。序号描述的是**对端已发出的消息**，本机是否接受不影响它已被消耗；于是对端下一条序号正常递增的合法消息被判 `sequence_invalid`，而它同样不推进 —— 连接退化成「只出不进」的哑管道，直到 90 秒静默超时才结束。

- 「未知 `type`」这一支本仓已修（`wire.rs` 约 102-107），但**同一族**的其余分支没修；
- sync 侧的同构实现在 WP3 修复轮已按「每一条消息级拒绝都推进」改对（`sync/conn/wire.rs` 的 `malformed_inbound` 与 `sequence_after_rejection`，配 `sync/conn/tests.rs` 的回归用例），node_link 侧没有对应要求、也没有用例；
- **且现有 node_link 用例⑤（`crates/server/src/node_link/conn/tests.rs` 约 1848-1857）把「形状错误不推进」写成了期望值** —— 只改代码不改测试会让 CI 变红，只改测试不改代码会把缺口永久固化。

## What Changes

- 形状类拒绝（未登记信封字段、缺 `messageId`、body 非 JSON 等）与 `ConnectionFieldsForbidden` 在拒绝时**照常推进入站期望序号**（照 sync 侧的 `sequence_after_rejection` 口径），使一次协议错误不会让连接变成哑管道。
- 把用例⑤的期望值改成正确口径，并补一条回归用例：形状错误之后的一条合法消息 MUST NOT 被判 `sequence_invalid`。
- 在 `node-link-owner-server` 规格里**新增**一条要求（既有规格没有它；sync 侧有对应条文）。

## Capabilities

### New Capabilities

（无。）

### Modified Capabilities

- `node-link-owner-server`：新增「每次消息级拒绝都推进入站期望序号」要求（两个场景：形状错误之后的合法消息、未知 `type` 与形状错误同口径）。

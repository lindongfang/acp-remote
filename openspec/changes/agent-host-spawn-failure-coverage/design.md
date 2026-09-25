# Design: agent-host-spawn-failure-coverage

<!-- 说明实现方案与决策理由；行为以 specs 为准，协作安排写 plan.md。 -->

## Context

`agent-host` 的启动失败路径现状（2026-09-25 复核确认）：

- `crates/agent-host/src/launch.rs:68` 把 spawn 失败收敛为 `HostError::SpawnFailed`，`error.rs:126` 将其映射为 `PortError::Unavailable(UnavailableKind::IoError)`；`host.rs` 的 `ensure_runtime` 失败不会登记 runtime。
- 已有测试只覆盖「相邻」路径：catalog 可用性探针把缺失二进制的条目标为 `available: false`（`catalog.rs:223`）、凭据解析失败在 spawn 前失败关闭、启动超时（`supervision.rs:167`）。**没有任何测试直接对缺失二进制的 profile 调用 `create()`**——即 spawn 失败本身及其端口映射、副作用边界均无回归保护。
- `HostError::to_port_error()`（16 个变体）整体无单元测试；映射表是 core 端口合同（`docs/CORE_PORTS_AND_STORAGE.md` §2/§8）在 adapter 侧的收敛点，静默改坏不会被现有测试发现。

本变更只补测试与规范场景，不动产品代码。

## Goals / Non-Goals

**Goals:**

- 集成测试钉死 spec 新场景「进程无法启动时返回明确不可用」：`create()` 对程序不存在的 profile 返回 `PortError::Unavailable(UnavailableKind::IoError)`，无进程副作用、无残留映射、重试不受毒化。
- 单元测试逐变体钉死 `HostError::to_port_error()` 映射表。
- 两类测试都成为常驻回归，在 Windows 本机与 Linux CI 上均可运行。

**Non-Goals:**

- 不修改 `HostError` 映射、launch/spawn 实现或任何产品代码。
- 不为其他 crate 的错误映射补测试。
- 不改动 wire/schema/fixture/compatibility 资产与权威文档（行为定义未变）。

## Decisions

### 1. 集成测试落在 `crates/agent-host/tests/catalog.rs`，不新建测试文件

`catalog.rs` 已有全部所需支撑：`profile_with`（构造指向不存在程序的 profile，":228 的 `agent-missing` 即先例）、`create()` 辅助、`FakeCredentials::ok()`、`wait_until_not_running`。复用可避免在新文件里复制一套 host/profile 构造。测试名建议 `spawn_failure_is_explicit_unavailable_without_side_effects`。

关键断言（对应 spec 场景的三个可观察结果）：

- `create()` 返回 `Err(PortError::Unavailable(kind))` 且 `kind == UnavailableKind::IoError`——精确匹配 kind，把「不可用（IO）」与「请求非法」「凭据不可用」「繁忙超时」区分开（spec 场景要求「可区分原因」）。
- 无副作用：失败后 `wait_until_not_running` 成立（没有半启动的 runtime 残留）；对同一会话调用 `open()` 报未知会话（没有留下会话映射）。
- 重试不毒化：紧接着第二次 `create()` 仍是干净的 `Unavailable(IoError)`（证明失败没有把 runtime 缓存成损坏状态，也没有泄漏「已存在」之类的二次错误）。

### 2. `to_port_error()` 单元测试放在 `error.rs` 内 `#[cfg(test)] mod tests`

`launch.rs:160` 已有 src 内单测先例；映射表是私有逻辑的纯函数，单测不需要 fake 子进程，放同文件最直接。逐变体表驱动：为 `HostError` 全部变体（当前 19 个，以代码为准）各构造一个实例，断言映射后的 `PortError` 类别与 kind。

### 3. 断言只钉错误类别与 kind，不钉消息文本

`PortError::InvalidRequest(&'static str)` 的消息文案是诊断细节，钉死会让改文案变成破坏测试；类别与 `ConflictKind`/`UnavailableKind` 才是调用方（未来的 `server::*`）据以决策的合同。这同时符合 error.rs 注释「错误消息只包含协议元数据」的边界。

### 4. 失败注入只用「程序名不存在」，不用权限/目录类手法

`acpr-does-not-exist-anywhere` 式的不存在程序名在 Windows 与 Linux 上都稳定、快速地以 spawn 错误失败； chmod/目录占用等手法平台差异大且可能受 CI 环境影响。权限类 spawn 失败不单独覆盖——映射相同，收益低于平台脆弱性成本。

## Risks / Trade-offs

- [补测试可能暴露实际行为与 spec 场景不符（例如失败后残留半个 runtime、第二次 create 报 Conflict）] → 按 proposal 的 decision_bounds 作为缺陷上报给用户决策，不在本变更顺手改实现；若属实则本变更转为「缺陷修复 + 回归测试」并同步更新 proposal/specs。
- [单元测试把映射表钉死后，未来合法调整映射需同步改测试] → 这正是目的：映射是端口合同的一部分，调整应显式发生；测试注释中说明这一点。
- [集成测试断言 `IoError` kind 依赖当前映射选择] → 该映射已由 spec 场景「区别于请求非法与凭据不可用」锚定语义；若未来改 kind，需先走规范变更。

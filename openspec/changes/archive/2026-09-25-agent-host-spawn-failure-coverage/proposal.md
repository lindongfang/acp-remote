# Proposal: agent-host-spawn-failure-coverage

<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

对照 `docs/DEVELOPMENT_PLAN.md` 切片 2「ACP 边界与本地 Agent」的验收标准复核时发现：`agent-host` 的「启动失败」路径覆盖不完整——catalog 可用性预检、凭据失败关闭与启动超时均有测试，但 **`create()` 时 spawn 本身失败**（profile 指向的程序不存在或不可执行）这条路径没有任何测试，`HostError::SpawnFailed → PortError::Unavailable(UnavailableKind::IoError)` 的映射及整个 `to_port_error()` 映射表也无单元测试。该映射一旦被改坏（例如错映射成 `InvalidRequest` 或静默吞掉），现有测试不会失败，违背切片 2「启动失败有明确结果」的验收承诺。

## What Changes

- 修改已有能力 `local-agent-host` 的增量规范：在「失败路径的明确结果与 turn 不设超时」需求下补一个 spawn 失败场景，钉死「程序无法启动时返回明确的不可用错误且不产生进程副作用」这一可观察行为。
- 新增 `agent-host` 集成测试：对程序不存在的 profile 直接调用 `create()`/`open()`，断言返回 `PortError::Unavailable(_)`、不留下 runtime/会话映射、不启动任何进程。
- 新增 `HostError::to_port_error()` 的单元测试：逐变体钉死错误映射表（含 `SpawnFailed → Unavailable(IoError)`），防止映射被静默改坏。
- 本变更**只补测试与规范场景，不改变任何产品代码行为**；若补测试过程中发现实际行为与规范场景不符，按缺陷处理并先报告，不顺手改实现。

## Capabilities

### New Capabilities

无。

### Modified Capabilities

- `local-agent-host`: 在「失败路径的明确结果与 turn 不设超时」需求下补充 spawn 失败场景——程序不存在或不可执行时，`create`/`open` 必须返回明确的不可用错误、不启动任何进程、不留下可复用的 runtime 或会话映射。行为定义不变，只是把切片 2 验收承诺中未被场景钉死的部分显式化。

## Impact

- **规范**：`openspec/specs/local-agent-host/spec.md`（归档时由增量同步；本变更只改 `openspec/changes/agent-host-spawn-failure-coverage/specs/local-agent-host/spec.md`）。
- **代码**：`crates/agent-host/tests/`（新增集成测试，预计落在 `catalog.rs` 或新文件）、`crates/agent-host/src/error.rs`（新增 `#[cfg(test)]` 单元测试模块）。
- **依赖/门禁**：无新依赖；`openspec validate --strict`、`npm run check`、`cargo test --locked -p agent-host --all-features` 须全绿。
- **不受影响**：wire 协议、schema/fixture/compatibility 资产、权威文档（行为定义未变，docs 无需同步）；CI 五个 job 的判定范围不变。

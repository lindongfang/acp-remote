# workspace-registration Acceptance Suite

- Source change: deliver-codex-session-loop
- Source definitions: openspec/changes/archive/2026-11-16-deliver-codex-session-loop/tests.md
- Long-term specification: openspec/specs/workspace-registration/spec.md
- Stable IDs: CL-028、CL-029

共用执行条件与归档源用例保持一致，运行结果另存 `testing/runs/deliver-codex-session-loop/`。裸 `specs` 路径在本长期套件中指 `openspec/specs`。

## Cases
### CL-028 目录失效

- 用例级别：L5
- 操作步骤：登记可用目录后删除/改为文件/撤销访问，再请求执行；对照有效绝对路径与含转义路径。
- 预期结果：节点解析真实路径，失效明确拒绝且不启动工作，不静默切默认目录；路径访问检查不冒充文件修改隔离。

### CL-029 两个会话共享目录

- 用例级别：L5
- 操作步骤：两个不同原生会话同时选择相同有效目录并执行；对照同一原生目标跨入口。
- 预期结果：不同会话允许并行，不因目录相同拒绝且不承诺冲突解决；同目标仍共享前台互斥。

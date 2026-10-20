## Execution Model

宿主没有 omp task/browser 工具。本次在主工作区串行实现并保留现有未提交修改，通过独立只读 agent 检视；不提交、不推送用户工作区。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 新目录成功创建后刷新、重投/并发/重连及提示 | none | primary | clients/app/src, clients/app/e2e/workspace-mkdir-live.mjs, docs/FRONTEND_DESIGN.md, openspec/specs/sync-workspace-browse, openspec/changes/session-created-directory-sync | 代码差异与验证记录 | PV1/PV2/PV3 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 单工作包，保留在途改动 | PV1/PV2/PV3 |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | npm.cmd run check / clients/app | 类型、全部单元测试及 Web 构建通过 |
| PV2 | npm.cmd run check / root | 合同检查通过 |
| PV3 | cargo test -p storage-sqlite --test workspace_alias / root | 目录保存/重开与 migration 回归通过 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | independent read-only agent | 快照顺序、断线意图、并发合并、路径来源与重复回复 | 无未闭环 major |

## Main E2E

```yaml
mode: required
```

优先运行已有浏览器/Daemon 场景；宿主能力不足时记录具体未执行项，不以单元测试代替真实浏览器验收。

## Risks

工作区包含用户在途修改；只改本次涉及的启动/创建链路，不覆盖分页等现有内容。真实浏览器验收取决于可用宿主工具，限制必须如实报告。

## Execution Evidence

- PV1：74 文件、1092 项测试通过，前端和 E2E 类型检查及 Web 构建通过。
- PV2：仓库 npm.cmd run check 全部通过。
- PV3：storage-sqlite/workspace_alias 的 5 项持久化/重开/migration 回归通过。
- 页面浏览器回归：`node clients/app/e2e/run-e2e.mjs` 在沙箱外输出 84/84 断言通过；结果输出后驱动未自行退出，主 Agent 中断清理，故不把该命令退出码记为成功。上面的独立真实 Daemon E2E 正常退出且通过。
- Main E2E：`cargo test -p app --test workspace_mkdir_browser --target-dir target/directory-sync-e2e -- --ignored --nocapture` 通过。真实 Chromium、隔离 TLS Daemon、隔离磁盘目录和受控 Agent，创建 project/src/api 后新建会话，返回目录页断言同一 sessionId 归属 api，页面真正重载后再次断言该目录和会话存在；独立 mkdir/取消不启动 Agent，最终 Agent cwd 正确。使用单独 target，未覆盖用户正在运行的 Daemon。沙箱内浏览器启动未完成，沙箱外隔离验收通过。
- 独立检视：review_directory_sync_ordered 为 pass。前两轮发现的异步快照与 caught_up 时序问题已修复为有界顺序暂存，延迟 digest + snapshot@10/event@11/caught_up@11、校验期间断线和队列超限的测试均通过。

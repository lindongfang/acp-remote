## Execution Model

沿用隔离工作包、patch 交付、独立检视、主检出串行合入与验证的项目流程。当前宿主不提供 omp task 工具，使用宿主子代理在独立文件副本中工作，不共享写目录；不伪称产生 agent:// 或 history:// URI。主代理保留报告与 patch 并执行 git apply --check / git apply。用户已明确授权本轮从已确认设计进入实现，不因自动采用规划技能重复请求许可。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 持久序号、迁移、片段边界 | none | isolated coder | crates/core, crates/storage-sqlite, crates/agent-host, docs/CORE_PORTS_AND_STORAGE.md, docs/ACP_COMPATIBILITY_MATRIX.md, compatibility/acp/v1/matrix.json | WP1.patch + 报告 | 主检出 cargo test -p storage-sqlite -p agent-host -p core |
| WP2 | 协议与本机/远程读取同域 | contract:WP1 | primary | crates/sync-protocol, crates/node-link-protocol, crates/server, crates/node-link-client, schemas, fixtures, docs/SYNC_PROTOCOL.md, docs/NODE_LINK_PROTOCOL.md, docs/MODULE_ARCHITECTURE.md, docs/INITIAL_DESIGN.md | 真实 diff + 报告 | 主检出 npm run check 与 Rust 测试 |
| WP3 | 页面、分页、易失内容保序 | contract:WP2 | isolated coder | clients/app, docs/FRONTEND_DESIGN.md | WP3.patch + 报告 | 主检出 clients/app npm run check |
| WP4 | 集成回归、独立检视、决策记录 | WP1,WP2,WP3 | primary + independent reviewer | .diagnostics, docs/adr/0015-agent-output-order.md, GLOSSARY.md, openspec/changes/preserve-agent-output-order | 检视报告、测试证据 | 主检出 npm run verify 与 Web E2E |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 先固定纯类型和持久事实 | storage/core/agent-host tests |
| 2 | WP2 | 协议消费 WP1 端口 | npm run check, cargo test |
| 3 | WP3 | 前端消费新增 wire 字段 | clients/app npm run check |
| 4 | WP4 | 验证集成 | npm run verify, Web E2E |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | cargo test --locked -p storage-sqlite -p agent-host -p core / root | 同毫秒、交替、迁移、清理后恢复 |
| PV2 | npm.cmd run check / root | 合同资产与依赖边界 |
| PV3 | npm.cmd run check / clients/app | 类型、测试与 Web 构建 |
| PV4 | npm.cmd run verify / root | 完整 Rust 与合同门禁 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | independent reviewer | 片段、事务、迁移与未知历史 | 无未闭环 major |
| WP2 | independent reviewer | 序号域、读取边界与原文保真 | 无未闭环 major |
| WP3 | independent reviewer | 实时/读取/分页一致性 | 无未闭环 major |
| WP4 | independent reviewer | 原始复现与完成证据 | 无未闭环 major |

## Main E2E

```yaml
mode: required
```

Web 断言思考、回复、工具穿插，刷新后 DOM 顺序一致；不操作用户真实会话或变更当前运行数据库。浏览器使用隔离测试宿主提供确定性协议数据；Rust 跨层测试覆盖真实 Agent 进程与持久化路径。

## Risks

已有未提交前端改动作为基线复制并保留，不覆盖用户工作。隔离副本不运行仓库级检查；全部检查在主检出串行执行。宿主执行原语差异如实记录，不修改项目长期流程。

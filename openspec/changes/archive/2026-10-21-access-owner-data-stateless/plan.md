# Plan

## Execution Model

主 Agent 使用 omp batch task 派发已满足依赖的包；task.isolation 为 patch、apply:false，每包独立工作区。交付 `<id>.patch`、`agent://<id>`、`history://<id>`，独立 reviewer spawn 检视，主检出按序 git apply --check 后 git apply。仓库检查只在主检出执行，不在缺少 target/node_modules 的隔离区运行。本轮只产规划，不派发代码包。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 冻结来源流、ACK、读取屏障和禁存合同 | none | wp-coder | AGENTS.md, docs/, compatibility/, schemas/, fixtures/, crates/sync-protocol/, crates/node-link-protocol/ | patch + wire fixture、规范变更索引 | PV1、PV2 |
| WP2 | 运行期远程目录/来源端口与 Owner 查询、owned 分域 | contract:WP1 | wp-coder | crates/core/ | patch + mock 端口行为与取消/来源隔离测试 | PV2 |
| WP3 | 删除 imported 持久路径与迁移清理 | code:WP2 | wp-coder | crates/storage-sqlite/, scripts/ | patch + 迁移、旧备份门禁与写入白名单证据 | PV1、PV2 |
| WP4 | Node Link、Sync 和组合根接通新路径 | contract:WP1, code:WP2, code:WP3 | wp-coder | crates/node-link-client/, crates/server/, crates/app/ | patch + 双节点重放/查询/ACK/重启测试 | PV1、PV2 |
| WP5 | 客户端来源流、重建合并与远程禁缓存 | contract:WP1, code:WP4 | wp-coder | clients/app/ | patch + TS 合同、来源去重和刷新恢复测试 | PV3、Main E2E |

WP1 统一维护产品/模块/协议/安全/core 存储文档及 ADR，其他包更新各自代码内文档并报告合同差异；若发现合同需订正，先更新 WP1 patch、串行重验，不越权改写别包文件。没有新增 core 第三方依赖。

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 合同先行，与后续包写范围不重叠 | PV1；实现缺口记录但不得标成通过 |
| 2 | WP2 | 冻结端口；不改 wire crate | PV2 |
| 3 | WP3 | 消费端口，切换数据库并清理旧列 | PV1、PV2 |
| 4 | WP4 | 消费端口和清理后的存储 | PV1、PV2 |
| 5 | WP5 | 以新服务端为基线验证双来源 | PV1、PV2、PV3、Main E2E |

后续 custom-session-title 会串行改相同模块，必须等本变更实现并验证后基于其提交重新读取代码；scoped-offline-page 最后接入，三变更不能按共享旧基线并行实现。

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | npm run check / 仓库根 | schema/fixture、词表、文档链接、边界、DDL/端口漂移和 OpenSpec 全绿 |
| PV2 | npm run check:rust / 仓库根 | fmt/clippy/workspace tests 通过；必须覆盖来源隔离、Owner 先提交、无收据、同请求不重复、迁移回滚与 owned 保留 |
| PV3 | npm run check / clients/app | TypeScript、E2E 类型、单元测试和 Web 导出通过；共享 fixture 同时经 Rust/TS 消费 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer，独立 spawn | 旧客户端降级、原文保真、ACK/游标/读取覆盖字段一致 | 无未解决 CRITICAL/MAJOR |
| WP2 | reviewer，独立 spawn | 端口纯度、owned 不变、来源身份与取消所有者 | 无未解决 CRITICAL/MAJOR |
| WP3 | reviewer，独立 spawn | 白名单完整、WAL/日志清理、失败门禁、无 owned 数据误删 | 无未解决 CRITICAL/MAJOR |
| WP4 | reviewer，独立 spawn | 认证撤销、查询回源、重放屏障、多客户端隔离 | 无未解决 CRITICAL/MAJOR |
| WP5 | reviewer，独立 spawn | 重建不重拼 delta、不落盘、断线清除和无自动副作用重发 | 无未解决 CRITICAL/MAJOR |

检视传入 patch 绝对路径及两类报告；changes-requested 按 WORKFLOW 另开修复/复核 spawn，最多三轮，不将作者自评作为通过证据。

## Main E2E

```yaml
mode: required
```

主检出运行 Owner、Access、可控 ACP Agent、两个浏览器客户端，逐项断言：完整消息及结构化事件、半条流式读取与追平重放无重复；Access 重启、页面刷新两种恢复；Owner 结果已提交但回复丢失；两个 Owner 同名/同 ID 隔离；单来源离线清除而其他来源与 owned 会话继续；撤销即时取消；慢端不阻塞。停止服务后检查 DB/日志/临时文件/浏览器持久存储无远程测试哨兵，连接信任与 owned 事件仍在。只看截图或退出码不算通过。

## Risks

删除旧机制跨度大，WP1 需先冻结协议再分包。中间提交不可上线，所有包及验证完成后才启用新 feature。缺少 omp/browser 工具时不得替换为共享目录隐式实现或宣称 E2E 完成，应记录缺失证据。

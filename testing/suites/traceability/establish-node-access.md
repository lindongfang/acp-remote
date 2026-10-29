# establish-node-access 用例追溯矩阵

- Source change: `establish-node-access`
- Source definitions: `openspec/changes/archive/2026-10-30-establish-node-access/tests.md`
- Long-term suites: `testing/suites/node-access/tests.md`、`testing/suites/client-trust/tests.md`
- Long-term specs: `openspec/specs/node-access/spec.md`、`openspec/specs/client-trust/spec.md`
- Definition totals: 2 capabilities, 15 Requirements, 28 Scenarios, 33 stable cases.
- 覆盖含义：下表只证明每个 Requirement/Scenario 被源用例的共同需求关联表引用；每例只保留用例级别、操作步骤和预期结果。不证明语义充分、实际执行或产品通过。
- 执行结果：另存 `testing/runs/establish-node-access/`，最新阶段见活动变更plan。旧NA-033安全汇总已合并删除，旧NA-034对应当前NA-033；不得按相同数字迁移通过状态。

## Coverage Matrix

| Capability | Requirement | Scenario | Stable cases | 定义覆盖 |
| --- | --- | --- | --- | --- |
| node-access | 完整节点独立启动 | 正常启动 | `NA-001`、`NA-011`、`NA-032` | 已引用 |
| node-access | 完整节点独立启动 | 初始化失败 | `NA-011` | 已引用 |
| node-access | 网络可达不授予管理权限 | 其他系统用户调用本机控制入口 | `NA-012` | 已引用 |
| node-access | 网络可达不授予管理权限 | 从可信网络发起未认证请求 | `NA-013` | 已引用 |
| node-access | 网络可达不授予管理权限 | 已认证客户端管理节点 | `NA-013` | 已引用 |
| node-access | 接入失败不泄漏敏感信息 | 认证失败 | `NA-002`、`NA-014` | 已引用 |
| node-access | 节点关闭具有资源所有权 | 正常关闭 | `NA-015` | 已引用 |
| node-access | 节点关闭具有资源所有权 | 关闭超时或失败 | `NA-016` | 已引用 |
| node-access | 重启保持可信状态 | 正常重启 | `NA-017` | 已引用 |
| node-access | 重启保持可信状态 | 身份资料损坏 | `NA-009`、`NA-017` | 已引用 |
| client-trust | 本机命令启动 Web 配对 | Web 输入配对码 | `NA-018` | 已引用 |
| client-trust | 本机确认后才建立信任 | 本机批准 | `NA-019`、`NA-033` | 已引用 |
| client-trust | 本机确认后才建立信任 | 本机拒绝或取消 | `NA-020`、`NA-033` | 已引用 |
| client-trust | 配对限时且不可转授 | 多个 Web 客户端并发认领 | `NA-021` | 已引用 |
| client-trust | 配对限时且不可转授 | 重放失效配对 | `NA-005`、`NA-022` | 已引用 |
| client-trust | 配对限时且不可转授 | 配对码猜测 | `NA-006`、`NA-023` | 已引用 |
| client-trust | 配对过程核实目标节点 | 节点身份被替换 | `NA-024` | 已引用 |
| client-trust | 身份必须经过验证 | 冒用已知客户端标识 | `NA-003`、`NA-004`、`NA-025` | 已引用 |
| client-trust | 身份必须经过验证 | 验证有效身份 | `NA-025` | 已引用 |
| client-trust | 信任建立必须可靠保存 | 保存成功 | `NA-026`、`NA-031` | 已引用 |
| client-trust | 信任建立必须可靠保存 | 保存失败 | `NA-007`、`NA-026`、`NA-031` | 已引用 |
| client-trust | 可信客户端可查看和撤销信任 | 撤销其他客户端 | `NA-027` | 已引用 |
| client-trust | 可信客户端可查看和撤销信任 | 撤销保存失败 | `NA-027` | 已引用 |
| client-trust | 可信客户端可查看和撤销信任 | 重复撤销 | `NA-027` | 已引用 |
| client-trust | 撤销覆盖已有连接 | 长连接被撤销 | `NA-028` | 已引用 |
| client-trust | 撤销覆盖已有连接 | 撤销与操作并发 | `NA-008`、`NA-028` | 已引用 |
| client-trust | 信任不代替运行时审批 | 后续模块请求权限 | `NA-010`、`NA-030` | 已引用 |
| client-trust | 认证材料不得作为管理数据返回 | 查询客户端列表 | `NA-029` | 已引用 |

## Stable ID Distribution

- node-access（11）：`NA-001`、`NA-002`、`NA-009`、`NA-011`、`NA-012`、`NA-013`、`NA-014`、`NA-015`、`NA-016`、`NA-017`、`NA-032`
- client-trust（22）：`NA-003`、`NA-004`、`NA-005`、`NA-006`、`NA-007`、`NA-008`、`NA-010`、`NA-018`、`NA-019`、`NA-020`、`NA-021`、`NA-022`、`NA-023`、`NA-024`、`NA-025`、`NA-026`、`NA-027`、`NA-028`、`NA-029`、`NA-030`、`NA-031`、`NA-033`

## Review Boundary

- `node testing/suites/verify-suite.mjs`只核对稳定ID、三个字段、源/长期用例定义一致以及规格和矩阵引用；不运行产品验收，不能替代reviewer的语义审阅或用户本人可理解性判断。
- 规格或用例调整后应同步本矩阵；不得通过重复ID、恢复旧人工签字要求或改写来源消除不一致。

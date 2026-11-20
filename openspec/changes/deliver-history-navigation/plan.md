# 历史导航实施计划

## Execution Model

主执行者按 openspec/role/ 分配角色。独立 reviewer 合审规格、设计及独立 tester 的用例后，coder 实现并自测。集成后由独立 tester 和 reviewer 对同一提交验收和检视。

## Parallel Assessment

轮次模型与协议字段冻结后，快照存储和 Web 分别在独立工作区开发。节点集成需要快照接口，浏览器联调需要节点命令，两者串行。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 可证明轮次、方向、参数与游标范围 | 无 | coder | app/src/domain、app/src/adapters/acp、app/src/management_ws | 分页模型、命令合同与单测 | HN-003 至 HN-007 |
| WP2 | 旧库首次翻页、增量索引及换代 | WP1 | coder | app/src/transcript_cache | 原子迁移与有界区间读测试 | HN-008、HN-009 |
| WP3 | 独立只读子历史查询、按来源选页、权限复核、跨代定位、实时衔接 | WP1、WP2 | coder | app/src/node、app/src/adapters/acp、app/src/application/native_dispatch、app/src/transcript_cache、app/src/application/shared_views、app/src/management_ws | 标准命令接线与集成测试 | HN-010 至 HN-024、HN-029 |
| WP4 | 父子独立翻页、折叠恢复与迟到响应处理 | WP1；联调依赖 WP3 | coder | clients/web/src、clients/web/tests | 分页组件、控制器与交互测试 | HN-002、HN-025 至 HN-027 |
| WP5 | 独立验收与交付检视 | WP1 至 WP4 | tester、reviewer | testing、当前变更 plan.md 与 tasks.md | 固定提交的执行证据与审查结论 | HN-001 至 HN-029 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 提供轮次和分页合同 | 模型与协议单测 |
| 2 | WP2 | 存储依赖轮次分类 | v1 迁移及索引一致性 |
| 3 | WP3 | 串行接入管理命令和存储 | 冷读、重放、撤权与并发读取 |
| 4 | WP4 | 在真实节点复核协议匹配 | Web 单测及浏览器翻页 |
| 5 | WP5 | 固定集成提交 | 全套独立验收与检视 |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 根目录运行 node testing/suites/verify-suite.mjs --change deliver-history-navigation 和 npm run check:spec | 场景映射与规划结构通过 |
| PV2 | 根目录运行 npm run check、npm run build:rust、git diff --check，并执行 tests.md HN-001 至 HN-026 | 工程与协议、存储、节点和 Web 断言通过 |
| PV3 | 按 tests.md HN-027、HN-028 从生产 Web 入口操作 | 受控和真实 ACP 链路各自有独立结果 |

## Code Review

| Gate / Scope | Reviewer Agent | Review Focus | Pass Condition | Status | Reviewed Snapshot / Notes |
| --- | --- | --- | --- | --- | --- |
| 编码前门禁 | 独立 reviewer | 父步骤入口、游标权限、轮次证明与用例充分性 | PV1 通过且语义阻断清零 | pending | 7aeff9a2 原范围已通过；独立子历史查询增量须复审后编码 |
| 集成交付门禁 | 独立 reviewer | 受测提交、真实链路、资源与安全 | 独立验收通过且阻断项关闭 | pending | 尚无受测提交 |

## Main E2E

```yaml
mode: required
```

使用隔离 data_dir、真实 Windows 节点与生产同源 Web。HN-027 使用受控 ACP，HN-028 使用已授权的真实 Codex；结束后验证子进程树退出并清理测试所建数据。

## Risks

跨代际按轮序号定位只保证当前范围；覆盖目标删除和迟到响应。手输 ID 属于不可信输入。HN-020 验证目标关系及访问权限，HN-027 从历史面板验证合法子目标可读。

## Handoff

交付本地功能分支受测 commit SHA、干净状态、当前变更规则与 tests.md。独立人员从固定提交执行，原始证据存于 `testing/runs/deliver-history-navigation/`。

## Findings

| ID | Source | Severity | Commit SHA | Finding | Fix Commit | Status | Retest / Re-review |
| --- | --- | --- | --- | --- | --- | --- | --- |
| HN-R01 | 独立 reviewer | 高 | 4dd96797 + 规划工作区 | 缺少可靠分轮证据 | 7aeff9a2 | closed | 原 reviewer 完成规划复核 |
| HN-R02 | 独立 reviewer | 高 | 4dd96797 + 规划工作区 | 实时新增轮次缺少分组边界 | 7aeff9a2 | closed | 原 reviewer 完成规划复核 |
| HN-R03 | 独立 reviewer | 高 | 4dd96797 + 规划工作区 | 游标范围编码与方向校验 | 7aeff9a2 | closed | 原 reviewer 完成规划复核 |
| HN-R04 | 独立 reviewer | 高 | 4dd96797 + 规划工作区 | 旧页遗漏迟到的工具补丁 | 7aeff9a2 | closed | 原 reviewer 完成规划复核 |
| HN-R05 | 独立 reviewer | 中 | 4dd96797 + 规划工作区 | 子首页与重新定位语义混用 | 7aeff9a2 | closed | 原 reviewer 完成规划复核 |
| HN-F01 | 独立 tester / HN-022 | 高 | 1808ce2 | 冷读旧页时，页内工具能显示最终状态，但指向页后 rawOutput 的引用缺少可读取的原始字节 | 待修复 | open | 已由真实 WSS 探针复现，待原 tester 对修复提交复测 |
| HN-F02 | 独立 tester / HN-012 | 高 | 1808ce2 | 根忙时按要求拒绝；根空闲但仅有根重放能力、无子快照或子事件时，子首次读取返回 unsupported_capability。原用例预期成功 | 用户已确认独立子历史查询 | open | 待分别验证有、无子数据路径及根名额 |
| HN-F03 | 独立 tester / HN-027 | 高 | 74e7c2bb | 双浏览器跨代定位到正确的 11–15 轮后提示读取或展示准备失败，第 15 轮新正文未显示 | 待修复 | open | 待真实浏览器复测 |

## Verification Results

当前为集成预验收，最终统一受测提交待锁定。独立 tester 在 1808ce2 的隔离检出加入验收探针，实测 HN-011 的未证身份拒绝、已证根首次重放、离线续页、淘汰后定位及 50/50 满额拒绝、释放一个名额重试。HN-018 的定位途中撤销子场景实测返回 4410，另一客户端仍可读，原生对端未收到取消。HN-022 的失败见 HN-F01。正式门禁状态等待验收代码合入和统一提交复验。

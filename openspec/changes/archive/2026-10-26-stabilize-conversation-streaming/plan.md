## Execution Model

- 使用仓库既有 omp batch `task` 派发；每个工作包一次独立 spawn，`isolated: true`，`task.isolation` 为 patch 模式且 `apply: false`，不直接修改主检出。
- 交付 `<id>.patch`、`agent://<id>`、`history://<id>`；工作包不创建提交或分支。检视由独立的内置 `reviewer` spawn 完成，传入 patch 绝对路径及报告。
- 主 Agent 串行执行 `git apply --check`、`git apply`；仓库级与前端检查、真实页面验证仅在主检出执行，不要求缺少依赖的隔离副本运行它们。
- 开始 apply 前检视本计划；主检出现有 `docs/evidence/omp-acp-tool-order.md` 修改是用户基线，不纳入本次 patch。远端落地遵循 `docs/WORKFLOW.md` §6.2。

## Work Packages

WP2 已完成三轮修复；综合检查遗留的宽度重排偏移单独收窄为 WP4，不继续扩大 WP2。WP1 第三轮仅处理临时回显位置与新增测试的真实条件，正式消息顺序保持不变。

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | D1/D2：即时增量、稳定内容身份、权威接替与无关渲染隔离 | none | wp-coder | `clients/app/src/components/ConversationTimeline.tsx`, `clients/app/src/components/Markdown.tsx`, `clients/app/src/components/conversation-timeline-view.test.ts`, `clients/app/src/components/markdown.test.ts`, `clients/app/src/features/conversation-timeline.ts`, `clients/app/src/features/conversation-timeline.test.ts`, `clients/app/src/features/client-store.ts`, `clients/app/src/features/conversation-submission.test.ts`, `clients/app/app/session/[id].tsx`, `clients/app/e2e/page/checks/conversation-reliability.ts`, `docs/FRONTEND_DESIGN.md` | `WP1.patch` + 报告/过程；真实调用链的收尾重建复现与回归用例、热路径对比方法 | 主检出 PV1/PV2；相关浏览器节点连续与即时输出断言 |
| WP2 | D3/D4：按实际尺寸补偿、上读锚点、悬浮操作/提示稳定 | none | wp-coder | `clients/app/src/components/ConversationStream.tsx`, `clients/app/src/components/ConversationComposer.tsx`, `clients/app/src/components/conversation-page.test.ts`, `clients/app/src/components/conversation-composer.test.ts`, `clients/app/e2e/page/checks/conversation-scroll-paging.ts`, `clients/app/e2e/page/checks/conversation-layout.ts`, `docs/FRONTEND_DESIGN.md` | `WP2.patch` + 报告/过程；同行/换行/上读/恢复可见/控件边界回归用例 | 主检出 PV1/PV2；逐帧位置与控件稳定断言 |
| WP0 | 对齐基线中 9 项旧浏览器用例的数据与现行诊断、授权、结构化条目规则，不改产品语义 | none | wp-coder | `clients/app/e2e/page/checks/r18-degradation.ts`, `clients/app/e2e/page/checks/conversation-timeline.ts`, `clients/app/e2e/page/checks/conversation-content.ts`, `clients/app/e2e/page/checks/conversation-interaction.ts`, `clients/app/e2e/page/checks/conversation-tool-detail.ts`, `clients/app/e2e/page/checks/session-files.ts` | `WP0.patch` + 报告/过程；逐失败项的旧假设与当前权威依据 | 主检出 PV1/PV2；保持诊断仅显式开启、有效请求才可授权等负向断言 |
| WP4 | 收窄处理窗口宽度变化与滚动事件竞争造成的阅读锚点偏移 | code:WP1, code:WP2 | wp-coder | `clients/app/src/components/ConversationStream.tsx`, `clients/app/e2e/page/checks/conversation-scroll-paging.ts` | `WP4.patch` + 报告/过程；准确锚点与逐帧位置依据，不放宽容差 | 主检出 PV1/PV2；SCROLL-REWRAP 与全滚动回归 |
| WP3 | 综合验证、真实部署构建确认、性能复测与证据 | code:WP1, code:WP2, code:WP4 | wp-coder | `docs/evidence/stabilize-conversation-streaming.md` | `WP3.patch` + 报告/过程；逐项结果、构建版本、可复现命令、采样条件与残余限制 | 主检出 PV1–PV4 与 Main E2E；不以旧部署代替新构建 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 0 | WP0 | 与产品工作包无写范围重叠，先验证旧测试条件修复，使新产品改动有全绿浏览器基线 | PV2；确认断言未被删除或改为常量成立，最终 PV1 再核对类型和全量测试 |
| 1 | WP2 | 已独立检视通过，先验证几何与控件；与 WP1 无代码写范围重叠 | PV1/PV2；同期 WP1 继续完成权威顺序与节点稳定的检视 |
| 2 | WP1 | 与 WP2 仅 `docs/FRONTEND_DESIGN.md` 写范围重叠，WP2 在末尾、WP1 在顶部新增说明，后合入校验上下文并重跑整体验证 | `git apply --check` 核对文档上下文；重跑 PV1/PV2，重点检查节点结构与锚点定位协作及权威序号不被重排 |
| 3 | WP4 | 在 WP2 最终代码基础上修复独立的宽度重排边界；与 WP2 同文件，串行合入 | PV1/PV2，全部滚动场景 |
| 4 | WP3 | 依据最终合并代码与部署构建采证，验证依赖包含 WP0、WP4 | PV1–PV4 与真实 Main E2E |

## Project Verify

全部命令只在主检出执行；隔离工作包内只做静态自查并交付 patch。

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `npm run check` / `clients/app` | 类型检查、E2E 类型检查、全部前端测试、Web 导出全部通过 |
| PV2 | `node e2e/run-e2e.mjs` / `clients/app` | 真实页面全部行为检查通过，包括本次新增的逐片段/节点连续/滚动与控件断言 |
| PV3 | `npm run verify` / 仓库根 | 合同门禁及 Rust 检查通过，未改变协议语义；CI 专属安全检查不声称本地通过 |
| PV4 | `openspec validate stabilize-conversation-streaming --strict` / 仓库根 | artifacts 与行为场景严格校验通过，任务证据可读 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer（独立 spawn） | 实际读取交接/本地回显/分组调用链；稳定节点、排序、交互与媒体状态比较完整；无正文延时 | 无未闭环 major/blocker，回归用例先复现后通过 |
| WP2 | reviewer（独立 spawn） | 实际尺寸判据、同步补偿、上读与零高/隐藏恢复、边界容差、键盘可访问与错误反馈 | 无未闭环 major/blocker，动态几何及控件用例通过 |
| WP0 | reviewer（独立 spawn） | 逐项旧测试条件与当前规范的对应，不改变产品行为、不删除必要断言，不全局开启诊断掩盖正式模式回归 | 无未闭环 major/blocker，9 项失败有可读修复依据 |
| WP3 | reviewer（独立 spawn） | 采样条件、真实加载构建、行为断言及失败记录与代码一致 | 不把启动/截图或旧资源算作通过，不漏记未验项 |
| WP4 | reviewer（独立 spawn） | 宽度重排前后同一锚点、用户滚动与程序滚动区别；不掩盖中间帧偏移 | 无未闭环 major/blocker，既有滚动断言不放宽 |

## Main E2E

```yaml
mode: required
```

- Web/PWA 经仓库 browser prelude 和真实页面入口验证；用户指定的现场复验使用 ego-browser，同一 TaskSpace 继续使用，不清理身份与缓存。
- 可控 host 驱动真实页面：单片段可见；同行追加滚动不变；换行底部同步跟随；上读、前置历史、媒体加载、宽度变化、思考收尾折叠与页面恢复保位；同一 messageId 在接替时节点引用连续；停止/发送和快捷收起边界稳定。
- 实际 Daemon `https://faraday.tail74842e.ts.net:8765/` 在确认加载新构建后复验。只在测试目录会话发送纯文字请求，不操作用户工作文件；真实授权/工具/失败等回归使用可控 host。
- 同设备、同内容、同采样条件比较脚本时间、长帧、布局次数和旧条目重渲染次数；时间指标作为对比证据，正确性以节点连续、阅读位置与逐片段可见断言为准。

## Risks

- 文档唯一重叠范围已串行登记；后合入者必须校验上下文并重跑集成检查，不能覆盖前一包内容。
- 当前环境是否可调用 omp batch `task` 在 apply 时核实；如不可用，应明确报告工具缺口，不擅自换调度器或声称已检视。
- 部署进程与静态目录需只读确认后按现有部署方式更新；未验证实际加载版本时不勾真实现场验收。
- CI 必需检查与远端权限在 apply 时核实，不绕过 main 保护。
- 实施前基线 PV2 为 128/137：9 项失败详情保存在本次外部会话目录的 `baseline-e2e.json`。WP0 仅维护这些旧测试；如果证据指向独立产品缺陷，另行报告，不通过放宽产品规则让测试通过。

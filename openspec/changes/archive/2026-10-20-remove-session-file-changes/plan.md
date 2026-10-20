# Plan

## Execution Model

- 本轮仅规划。实施遵循 `docs/WORKFLOW.md`：主 Agent 以 omp `task` batch 派发，每包一次 `wp-coder` spawn、`isolated: true`；`task.isolation` 使用 patch 模式、`apply: false`。
- 交付 `<id>.patch` + `agent://<id>` + `history://<id>`，不让子 Agent 建分支或提交；主 Agent 串行 `git apply --check`、`git apply`，只应用独立检视通过的最终补丁。
- 隔离区没有 `target/` 与 `node_modules/`；Project Verify 及真实浏览器验收只在主检出执行。
- 派发前记录现有未提交修改及 `session-file-browser` 基线；先完成旧文件浏览实现的合入再实施本变更，不重置、覆盖或自动暂存用户修改。若 `conversation-area-complete` 同时改相同组件，串行合入、重跑 PV1–PV3。
- WP1 与 WP2 写范围不重叠，可在同一 batch 派发；主 Agent 按下表串行合入，真实结果不足时不得将文档标为已验证。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 两来源、默认回退、刷新与专属查看清理、时间线保留及宽窄屏回归 | none | wp-coder | `clients/app/src/`、`clients/app/app/session/`、`clients/app/e2e/`，限文件栏及其直接调用方/测试，不改协议 DTO 与平台端口 | WP1.patch + 报告/过程；来源/状态/UI/组件与 E2E 用例共同更新，公共历史与差异渲染保留 | 主检出 PV1、PV2、PV3；所有规范场景有断言，非法来源不查询 |
| WP2 | 原型双来源、文档最终口径与实施差距说明 | none | wp-coder | `prototypes/acp-remote-pwa.html`、`prototypes/README.md`、`prototypes/IMPLEMENTATION-GAPS.md`、`docs/FRONTEND_DESIGN.md`、`docs/INITIAL_DESIGN.md`、`docs/conversation-area-discussion.md`、`docs/evidence/remove-session-file-changes.md`、`GLOSSARY.md` | WP2.patch + 报告/过程；原型与文档不再要求现行三来源，历史证据不改写；证据模板显式区分未执行项 | 主检出 PV2、PV3、PV4；原型两个来源且默认全部文件，文档不提前声称验证通过 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 行为与回归用例一起变更，无 WP2 写范围重叠 | PV1、PV2 |
| 2 | WP2 | 无写范围重叠，结合实际 WP1 行为核对设计资产 | PV2、PV4；完成 PV3 并由主 Agent 写入真实证据 |

主 Agent 仅维护当前变更 tasks 状态与真实验收证据；不改其他变更已勾选的历史任务。实施发现共享文件需扩展时先修订写范围及合入顺序，再派发。后续远端落地遵循工作流的分支、PR、必需检查及 squash 路径；当前请求不执行这些动作。

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `npm.cmd run check` / `clients/app` | 类型、E2E 类型、前端测试与 Web 导出通过；默认/旧值回退、无专属刷新、文件关闭恢复、时间线差异与历史回归有断言 |
| PV2 | `npm.cmd run check` / 仓库根 | 合同、文档链接、依赖边界及 OpenSpec 门禁通过；无协议/存储变更 |
| PV3 | `node e2e/run-e2e.mjs` / `clients/app`，以及主 Agent 的 omp browser prelude + 本机真实 Daemon | 假主机用例与下述真实 PWA 场景分层通过，报告平台、实际断言与未执行项 |
| PV4 | `openspec.cmd validate remove-session-file-changes --strict` / 仓库根；omp browser prelude 打开本地原型 | 规划校验通过；原型两来源、默认、文件开关及窄屏无已删除入口 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | 内置 reviewer，独立于作者的新 spawn | 来源穷尽、旧值回退、未接线回退组件、调用方、刷新调度与公共历史分离、时间线实时/恢复与远程门控 | 无未闭环 blocker/major，未删除结构化事件或虚构 Git 会话归属 |
| WP2 | 内置 reviewer，独立于作者的新 spawn | 原型脚本/入口、权威文档单一口径、在途规范同步顺序、既有证据保真 | 无未闭环 blocker/major，计划与实绩区分，原型和产品范围一致 |

检视输入包含 patch 绝对路径、`agent://<id>` 与 `history://<id>`，用工作流的 per-call outputSchema 输出 pass/changes-requested/blocked。修改与复核均使用新 spawn；最多三轮修复，超限回到拆包；blocked 先补证据。

## Main E2E

```yaml
mode: required
```

主 Agent 在主检出用 omp browser prelude，对真实 Daemon 托管的已认证 PWA 使用临时 Git 仓库、非仓库目录及已有会话验收，不修改用户仓库：

1. 宽屏、窄屏都仅有全部文件/Git 变更；首次进入默认全部文件。旧 session 选择通过状态回归测试覆盖，不伪造浏览器持久化迁移。
2. 人工修改文件后 Git 如实展示，界面说明修改可能来自其他会话/人工；干净仓库与非仓库不同，非仓库可继续浏览全部文件。
3. Agent 产生工具调用和文件差异，文件栏无会话清单，时间线仍可展开差异及原文诊断；重新进入恢复历史，重复事件不重复呈现。
4. 打开文本/图片与 Git 差异，快速切文件、关闭及跨宽窄屏仍保留草稿、滚动、展开；Agent 不停止，迟到响应不重开文件。
5. 手动刷新及文件事件只刷新目录/Git；对话历史仍可分页，不再有专属会话变更刷新。imported、离线和旧能力明确说明不可用，不访问替代目录。

证据由主 Agent 在 `docs/evidence/remove-session-file-changes.md` 补充，分开记录组件/假主机、真实 Daemon、原型及未执行场景；截图或启动成功不代替断言。

## Risks

- 主检出在途修改较多，派发前固定基线，冲突返回新 spawn 修补，不覆盖式复制。
- `session-file-browser` 尚未归档，本增量修改其新增要求；同步顺序必须先旧后新，不把本变更的规划通过误当作可直接同步全部增量。
- 当前未提供本轮真实运行证据；实施缺平台、浏览器或 Daemon 条件时据实记录阻塞，不虚报完成。

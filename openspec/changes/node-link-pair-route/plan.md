<!-- 执行安排：把 specs 的行为要求与 design 的技术方案转成可派发的工作包。 -->

# Plan

## Execution Model

- 派发：主 Agent 用 `task` 工具 batch 形状；本变更**一个工作包**（写范围小且互不重叠）。
- 隔离：`task.isolation`（patch 模式、`apply: false`）；交付 `<id>.patch` + `agent://<id>` + `history://<id>`。
- 合入：主 Agent 串行 `git apply`（命令行，不用 omp 原生 git）。
- 提交信息手写并按 `commitlint.config.mjs`：本变更落点在前端 ⇒ `fix(frontend)` 或 `feat(frontend)`（视最终形态：路由缺失属修复）。
- 前端工作包在隔离区**可以**跑 `npm`：`clients/app` 的依赖与 vitest 都在仓库内，但隔离区没有 `node_modules` ⇒ 以主检出验证为准，报告不得声称已在隔离区通过。

## Work Packages

### WP1 —— 前端配对路由与两类入口区分

- 写范围：`clients/app/app/`（新增 node-link 配对路由文件）、`clients/app/src/**`（配对入口组件的「期望类型」入参、载荷与类型的一致性校验、失败态）、`clients/app/src/**/*.test.ts` 与 `clients/app/e2e/**`（断言）、`docs/FRONTEND_DESIGN.md`（新路由与口径登记）。
- 依赖：无。
- 不做：不改 `crates/server/**`（D1 已定不改 URL 构造）、不改 `docs/NODE_LINK_PROTOCOL.md`（§13.1 不动）。

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 唯一代码包 | PV1–PV4 |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cd clients/app && npm run check`（主检出） | `typecheck` + `vitest run` + `export:web` 全绿 |
| PV2 | `cd clients/app && node e2e/run-e2e.mjs`（主检出，按 `e2e/` 下的实际入口为准） | 在真实静态面形态下打开 `/node-link/pair#data=…` 落到可用入口（DOM 断言），且两类入口交叉喂错内容时明确失败 |
| PV3 | 反例探针（主检出，临时改动后恢复） | 删掉新路由文件（或把路径写错）⇒ PV2 的断言必须红；恢复后复跑 PV1 |
| PV4 | 根目录 `npm run check`（主检出） | 全绿（docs/links 覆盖新增的文档改动） |
| PV5 | CI 两次「前端检查（clients/app）」+「合同门禁 + Rust 检查」 | 两次全绿 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 两类入口是否真做了**类型**校验（而非靠载荷形状猜）；「明确失败」是否可断言且不回显秘密；断言是否在路径被改错时确实变红；有没有顺手改服务端/协议文档 | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: not-applicable
reason: 本变更的观察面就是前端入口本身（打开二维码 URL 后的可用性），入口即前端 e2e（真实静态面形态 + DOM 断言），没有第二层可叠。
basis: docs/WORKFLOW.md §7 要求断言可观察行为；此处的可观察行为就是「打开该 URL 后页面可读/可提交」与「交叉内容明确失败」。
alternative_checks:
  - PV2：真实形态下的 DOM 断言（不是只断言路由文件存在）。
  - PV3：删路由/改错路径时断言变红，证明它可证伪。
  - PV5：CI 两次。
```

## Risks

- **[只在单测里断言路由]** 单测能验证「类型校验」这类纯逻辑，但抓不到「路由没挂上/被回退吞掉」⇒ 必须有 PV2 的 DOM 级断言（design 的 Risks 已登记）。
- **[前端隔离区没有 node_modules]** 工作包作者不得声称已跑通前端门禁；真实运行由主 Agent 按 PV1/PV2 执行。
- **[失败态的可断言性]** 「交叉喂错内容明确失败」若只写成文案容易变成不可证伪的断言 ⇒ 必须落到状态/DOM 可判定的事实上。

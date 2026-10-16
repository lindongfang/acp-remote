<!-- 执行安排：把 specs 的行为要求与 design 的技术方案转成可派发的工作包。 -->

# Plan

## Execution Model

- 派发：主 Agent 用 `task` 工具 batch 形状；本变更**一个工作包**（前端两处校验 + 断言 + 文档）。
- 隔离：`task.isolation`（patch 模式、`apply: false`）；交付 `<id>.patch` + `agent://<id>` + `history://<id>`。
- 合入：主 Agent 串行 `git apply`（命令行，不用 omp 原生 git）。
- 提交信息手写并按 `commitlint.config.mjs`：落点在前端 ⇒ `fix(frontend)`。
- 隔离区**没有 `node_modules`**：工作包不得声称跑过前端门禁；真实运行由主 Agent 在主检出做。

## Work Packages

### WP1 —— 配对入口的两条校验（来源一致 / 安全上下文）

- 写范围：`clients/app/src/features/pairing-model.ts`（两条判定与失败态、文案）、`clients/app/src/features/client-store.ts`（唯一入口处接线）、`clients/app/src/composition.ts`（环境事实的注入面）、`clients/app/src/**/*.test.ts` 与 `clients/app/e2e/**`（断言）、`docs/FRONTEND_DESIGN.md`（§4.5 与登记条目改成实绩）。
- 依赖：无。
- 不做：不改 `crates/**`（服务端已按要求校验 canonical origin，前端缺的是入口侧）；不改协议文档的行为要求。
- **错误码**（design 的 D3）：来源不符复用既有 `auth.origin_mismatch`（零新增）；非安全上下文新增 `auth.insecure_context`，必须按封闭词表的同步清单一起改：`compatibility/errors/v1/errors.json`、对应 schema enum、相应 fixture、`clients/app/src/protocol/common.ts` 的 union，并让 `npm run check`（含 `check:errors`）与前端门禁全绿。

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 唯一代码包 | PV1–PV4 |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cd clients/app && npm run check`（主检出） | `typecheck` + `vitest run` + `export:web` 全绿 |
| PV2 | `cd clients/app && node e2e/run-e2e.mjs`（主检出） | 既有 53 条不回归；若新增端到端断言（例如非安全上下文形态）也全绿 |
| PV3 | 反例探针（主检出，临时改动后恢复） | ① 去掉「来源一致」判定 ⇒ 1.1 的用例必须红；② 去掉「安全上下文」判定 ⇒ 1.2 的用例必须红；③ 去掉「异常文本不透传」⇒ 1.3 的用例必须红。实验后还原并复跑 PV1 |
| PV4 | 根目录 `npm run check`（主检出） | 全绿；`check:docs`/`check:drift` 覆盖 §4.5 的文档改动 |
| PV5 | CI 两次「前端检查（clients/app）」+「合同门禁 + Rust 检查」 | 两次全绿 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 两条判定是否落在**唯一入口**且在任何请求之前；来源比较是否用同一条归一化口径（不是随手字符串比较）；安全上下文经的是可注入的环境面（不是散落的 `window` 读取）；失败码是否取自既有词表且语义贴合（若新增，是否同步了合同资产）；三条断言是否真能变红、且文案不透传秘密 | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: not-applicable
reason: 观察面就是配对入口本身（粘贴内容后的失败态与请求计数），入口即前端单测与 e2e；没有第二层可叠。
basis: docs/WORKFLOW.md §7 要求断言可观察行为；本变更的可观察行为是「来源不符/非安全上下文时进入可区分失败态且零请求」，已在单测与 e2e 上断言。
alternative_checks:
  - PV3：三条判定各自的反例探针，证明断言有判别力。
  - PV2：既有 53 条 e2e 不回归。
  - PV5：CI 两次。
```

## Risks

- **[环境事实的注入面]** 安全上下文与当前来源必须可注入（否则非安全上下文这条在正常测试环境里造不出来）；若注入面需要动 `src/platform/**` 的冻结装配面，工作包必须停下来说明，不得顺手改。
- **[来源比较的归一化口径]** 直接字符串比较会在端口/尾部斜杠/大小写等形态上误判 ⇒ 必须复用协议对 `canonicalOrigin` 的既有归一化口径（前端已有实现），不得新造第二套。
- **[断言只测文案]** 三条判据都必须落到可判定的事实（失败态 + claim/status 计数为 0 + 秘密不出现在文案或错误对象里），不能只断言「有一句提示」。

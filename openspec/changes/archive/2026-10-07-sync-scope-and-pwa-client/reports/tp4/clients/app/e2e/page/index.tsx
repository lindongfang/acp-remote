// 【交接副本】来源 worktree .worktrees/tp4 的 clients/app/e2e/page/index.tsx
// 交付提交 0a19d39b4c63b723c7e7d7edbaaf64e60cd82e83（分支 feat/tp4），基线 f6ea252。
// 用途：premerge 门禁按「相对变更目录」解析 test_delivery.artifacts 的可读性；本副本在权威交付物之外。
// 除顶部本 4 行说明外与来源逐字节相同（权威可运行副本在 worktree 内，由 PV3 执行）。
/**
 * 浏览器页面的入口：把检查集合挂到 `window.__tp4`，供 `e2e/run-e2e.mjs` 经 CDP 调用。
 *
 * 页面里**没有**任何断言实现——断言全在 `./checks/`，序列化的结果直接交给 Node 侧打印。
 */

import { R15_CHECKS } from "./checks/r15-directory";
import { R16_CHECKS } from "./checks/r16-create";
import { R18_CHECKS } from "./checks/r18-degradation";
import { COMPOSITION_CHECKS } from "./checks/composition-root";
import { runChecks } from "./support";
import type { CheckResult } from "./support";

/**
 * 检查顺序：先跑组合根接线（它是其它所有用例的前提），再跑三条需求各自的用例。
 *
 * 组合根在最前面还有一个实际好处：它失败时后面每一条都会失败，输出里一眼看得出
 * 「不是需求没实现，是运行时没起来」。
 */
const ALL_CHECKS = [...COMPOSITION_CHECKS, ...R15_CHECKS, ...R16_CHECKS, ...R18_CHECKS];

declare global {
  interface Window {
    __tp4: { readonly runChecks: () => Promise<CheckResult[]> };
  }
}

window.__tp4 = { runChecks: () => runChecks(ALL_CHECKS) };
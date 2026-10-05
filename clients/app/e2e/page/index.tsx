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
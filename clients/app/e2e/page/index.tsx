/**
 * 浏览器页面的入口：把检查集合挂到 `window.__tp4`，供 `e2e/run-e2e.mjs` 经 CDP 调用。
 *
 * 页面里**没有**任何断言实现——断言全在 `./checks/`，序列化的结果直接交给 Node 侧打印。
 */

import { R15_CHECKS } from "./checks/r15-directory";
import { R15_DETAIL_CHECKS } from "./checks/r15-directory-detail";
import { R16_CHECKS } from "./checks/r16-create";
import { R18_CHECKS } from "./checks/r18-degradation";
import { COMPOSITION_CHECKS } from "./checks/composition-root";
import { CONVERSATION_TIMELINE_CHECKS } from "./checks/conversation-timeline";
import { CONVERSATION_INTERACTION_CHECKS } from "./checks/conversation-interaction";
import { CONVERSATION_CONTENT_CHECKS } from "./checks/conversation-content";
import { PAIRING_FIRST_RUN_CHECKS } from "./checks/pairing-first-run";
import { PAIRING_KIND_CHECKS } from "./checks/pairing-kinds";
import { runChecks } from "./support";
import type { CheckResult } from "./support";

/**
 * 检查顺序：先跑组合根接线（它是其它所有用例的前提），再跑各条需求各自的用例。
 *
 * 组合根在最前面还有一个实际好处：它失败时后面每一条都会失败，输出里一眼看得出
 * 「不是需求没实现，是运行时没起来」。
 *
 * R15 的目录页与目录详情页分成两个文件（各自一个检查集），共用同一套 `data-*` 口径与
 * 夹具来源；两者之间没有顺序依赖，放在一起只是让同一需求的用例连成一段输出。
 *
 * 跨路由的用例（进入某个目录的详情页、返回目录页后查询词仍在、零会话的目录也可达）
 * 放在**详情集**里：它们要先挂目录页、再导航过去，而共用的等待与打字机关
 * （`waitForDirectoryList` / `typeIntoInput` / `withScenario`）都在目录页那一半，
 * 由详情集单向导入——反过来导入会成环。
 *
 * 对话页（`pwa-conversation-ui`）按主题分三个检查集：时间线与结构化历史、输入/授权/设置、
 * 变更文件右栏/来源降级/富文本。三者的共用机关（`sessionScenario` / `enterRoute` /
 * `withSessionPage` / `commandBody`）与 R15 同一取舍，放在**时间线**那一半，另外两个单向导入。
 *
 * 配对链路（`pairing-first-run.ts`）放在**最后**：它是唯一从「未配对」出发的检查集，为此会清掉
 * 本地缓存里的 `paired-host/<origin>` 记录（见 `support.tsx` 的 `seedPaired: false`）。
 * 排在最后既不影响其它检查集的起点，也让「配对链路」这块证据在输出末尾自成一段。
 *
 * 两类入口的区分（`pairing-kinds.ts`）排在它之后：同样从「未配对」出发（同一条清缓存的理由），
 * 而它验的是「打开 node-link 二维码落在可用入口」与「两类内容喂错入口都明确失败」。
 */
const ALL_CHECKS = [
  ...COMPOSITION_CHECKS,
  ...R15_CHECKS,
  ...R15_DETAIL_CHECKS,
  ...R16_CHECKS,
  ...R18_CHECKS,
  ...CONVERSATION_TIMELINE_CHECKS,
  ...CONVERSATION_INTERACTION_CHECKS,
  ...CONVERSATION_CONTENT_CHECKS,
  ...PAIRING_FIRST_RUN_CHECKS,
  ...PAIRING_KIND_CHECKS,
];

declare global {
  interface Window {
    __tp4: { readonly runChecks: () => Promise<CheckResult[]> };
  }
}

window.__tp4 = { runChecks: () => runChecks(ALL_CHECKS) };
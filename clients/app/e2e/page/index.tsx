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
import { CONVERSATION_LAYOUT_CHECKS } from "./checks/conversation-layout";
import { CONVERSATION_TOOL_DETAIL_CHECKS } from "./checks/conversation-tool-detail";
import { SESSION_FILES_CHECKS } from "./checks/session-files";
import { SESSION_TITLE_CHECKS } from "./checks/session-title";
import { FRAME_ACCEPTANCE_CHECKS } from "./checks/frame-acceptance";
import { PAIRING_FIRST_RUN_CHECKS } from "./checks/pairing-first-run";
import { PAIRING_KIND_CHECKS } from "./checks/pairing-kinds";
import { UI_SHELL_CHECKS } from "./checks/ui-shell";
import { UI_STYLESHEET_CHECKS } from "./checks/ui-stylesheet";
import { runChecks } from "./support";
import type { CheckResult } from "./support";

/**
 * 检查顺序：**样式形态最前**，其次是组合根接线，然后才是各条需求各自的用例。
 *
 * 样式检查（`checks/ui-stylesheet.ts`）放最前有两条理由：
 *
 * 1. 它是本页在既定 CSP 下**能不能有样式**的前提，而本目录的其它检查**大多**靠 DOM 断言
 *    （属性、文本、卡片数）——样式整表失效时那些断言**照样通过**，「页面一片白板」也能报全绿。
 *    唯一的例外是 R15-16（`checks/r15-directory-detail.ts`）：它读
 *    `getComputedStyle(toolbar).flexDirection`（宽档 `row` / 窄档 `column`），样式整表失效时
 *    窄档那条断言**会**红，所以「只有样式检查能看出白板」这句话不适用于它。
 *    把它排第一条，失败时输出第一行就是它，不会被后面一长串「通过」淹没。
 * 2. 它不依赖任何别的检查集铺垫（自带假主机场景），也不改变共享起点：与 CR-1 一样只预置
 *    「已配对」记录、只挂自己的容器，卸载时 dispose 的是它自己的组合根。
 *
 * 组合根在其次还有一个实际好处：它失败时后面每一条都会失败，输出里一眼看得出
 * 「不是需求没实现，是运行时没起来」。
 *
 * 共享外壳（`checks/ui-shell.ts`）夹在这两者之间：外壳是**全部路由的共同外层**，它渲染不出来时
 * 后面的路由用例会连带失败，因此与样式检查同属「页面骨架」这一层、排在需求用例之前；它自带场景
 * （真实握手 + 已认证连接），不依赖别的检查集铺垫，也不占用 `mountRootLayout` 的唯一一次挂载
 * （那是 CR-1 的）。
 *
 * R15 的目录页与目录详情页分成两个文件（各自一个检查集），共用同一套 `data-*` 口径与
 * 夹具来源；两者之间没有顺序依赖，放在一起只是让同一需求的用例连成一段输出。
 *
 * 跨路由的用例（进入某个目录的详情页、返回目录页后查询词仍在、零会话的目录也可达）
 * 放在**详情集**里：它们要先挂目录页、再导航过去，而共用的等待与打字机关
 * （`waitForDirectoryList` / `typeIntoInput` / `withScenario`）都在目录页那一半，
 * 由详情集单向导入——反过来导入会成环。
 *
 * 对话页（`pwa-conversation-ui`）按主题分五个检查集：时间线与结构化历史、输入/授权/设置、
 * 变更文件右栏/来源降级/富文本，以及布局/左栏（`conversation-layout.ts`）与工具详情/代码行号
 * （`conversation-tool-detail.ts`）。共用机关（`sessionScenario` / `enterRoute` /
 * `withSessionPage` / `commandBody`）与 R15 同一取舍，放在**时间线**那一半，其余四个单向导入。
 *
 * 布局检查排在内容检查之后：它依赖 `support.tsx` 的 `HarnessLayout` 与 `app/_layout.tsx` 同构
 * （真实 `Shell`）。只挂 `Stack` 时对话页拿不到视口高度，三栏独立滚动在浏览器里无从成立。
 *
 * 帧受理面（`frame-acceptance.ts`）排在**各需求用例的最后一段**（配对用例之前）：它验的是
 * 解码边界的受理面——`origin.kind = "node"` 的协议合法事件进了账本、被页面呈现、游标继续前进。
 * 为此它自带场景（已配对起点 + 真实握手 + 真实路由，自己挂、自己卸），既不占用
 * `mountRootLayout` 的唯一一次挂载，也不动配对用例那条「清掉已配对记录」的起点。它借道对话页
 * 观察呈现结果（时间线卡片由结构化记录派生），因此放在对话页三段之后，输出上也与它们连成一段；
 * 但它不依赖这三段里任何一条用例先跑过。
 *
 * 配对链路（`pairing-first-run.ts`）放在**最后**：它是唯一从「未配对」出发的检查集，为此会清掉
 * 本地缓存里的 `paired-host/<origin>` 记录（见 `support.tsx` 的 `seedPaired: false`）。
 * 排在最后既不影响其它检查集的起点，也让「配对链路」这块证据在输出末尾自成一段。
 *
 * 两类入口的区分（`pairing-kinds.ts`）排在它之后：同样从「未配对」出发（同一条清缓存的理由），
 * 而它验的是「打开 node-link 二维码落在可用入口」与「两类内容喂错入口都明确失败」。
 */
const ALL_CHECKS = [
  ...UI_STYLESHEET_CHECKS,
  ...UI_SHELL_CHECKS,
  ...COMPOSITION_CHECKS,
  ...R15_CHECKS,
  ...R15_DETAIL_CHECKS,
  ...R16_CHECKS,
  ...R18_CHECKS,
  ...CONVERSATION_TIMELINE_CHECKS,
  ...CONVERSATION_INTERACTION_CHECKS,
  ...CONVERSATION_CONTENT_CHECKS,
  ...CONVERSATION_LAYOUT_CHECKS,
  ...CONVERSATION_TOOL_DETAIL_CHECKS,
  ...SESSION_FILES_CHECKS,
  ...SESSION_TITLE_CHECKS,
  ...FRAME_ACCEPTANCE_CHECKS,
  ...PAIRING_FIRST_RUN_CHECKS,
  ...PAIRING_KIND_CHECKS,
];

declare global {
  interface Window {
    __tp4: { readonly runChecks: (namePrefix?: string) => Promise<CheckResult[]> };
  }
}

window.__tp4 = { runChecks: (namePrefix = "") => runChecks(ALL_CHECKS.filter(check => check.name.startsWith(namePrefix))) };

/**
 * 共享外壳（`spec` 的「共享外壳承担连接状态与全局入口」，`design.md` D1）的浏览器级验证。
 *
 * ## 为什么自己挂一层容器
 *
 * `app/_layout.tsx` 的组合根是**模块级单例**，而 `dispose()` 之后 `start()` 会变成 no-op
 * （`src/composition.ts` 的 `disposed` 闸门）。`support.tsx` 的 `mountRootLayout` 因此只能在本页
 * 用**一次**，它已归 CR-1；本检查集改为把真实的 `Shell` 挂在场景自己的 store 上——路由与连接状态
 * 仍然是真实输入（路由替身的当前条目 + 真实连接机 + 真实 `ClientStore`），组合根只装配一次。
 *
 * ## 判别力
 *
 * - 「配对路由不呈现顶栏」用**差分**判据：同一个外壳实例、同一份 store，只把当前路径换成根路径，
 *   顶栏就必须出现——「整页什么都没渲染」在这种写法下过不去。
 * - 「配对页呈现自己的头部」同样用差分，且内容区交给路由替身的出口（`Stack`）：换到根路径时那个
 *   页头必须随页面一起消失、全局顶栏顶上来——页头因此被证明是**这一页自己的**，而不是外壳顺手画的
 *   或「壳里摆了某个 `data-*`」。
 * - 「在线时胶囊不追加短词」同时断言**别的锚点也没有状态**：外壳容器里不得出现第二个状态徽标
 *   （`data-connection-state`），那是「连接状态只由胶囊表达」的可证伪形态。
 * - 「胶囊是进入主机与连接面板的入口」不是看有没有 `data-action`，而是真点一次、核对路由替身
 *   的当前路径变成了 `/hosts`。
 *
 * 断言一律属性型（`[data-shell-top-bar="visible"]` 这类锚点），不看 class 名或标签名。
 */

import { createElement } from "react";
import type { ReactNode } from "react";
import { createRoot } from "react-dom/client";
import type { Root } from "react-dom/client";

import { Shell } from "../../../src/components/shell/Shell";
import { ClientStoreProvider } from "../../../src/features/runtime";
import { AGENTS, DIRS_ROUTE, HOSTS_ROUTE, PAIR_ROUTE, WORKSPACES_WITH_PATHS, sessionsForBothDirectories } from "../fixtures";
import { Stack, currentPath } from "../router-stub";
import {
  CANONICAL_ORIGIN,
  click,
  expectEqual,
  expectThat,
  mountRoute,
  mountScenario,
  query,
  waitFor,
} from "../support";
import type { Check, Mounted, ScenarioOptions } from "../support";

/** 与其它检查集同一形状的场景：真实 `start()` + 真实握手 + 已认证连接。 */
function baseScenario(overrides: Partial<ScenarioOptions> = {}): ScenarioOptions {
  return {
    canonicalOrigin: CANONICAL_ORIGIN,
    workspaces: WORKSPACES_WITH_PATHS,
    sessions: sessionsForBothDirectories(),
    agents: AGENTS,
    authenticate: true,
    tamperHostProof: false,
    deliverSnapshot: true,
    barrierGlobalSequence: "10",
    events: [],
    readResult: () => ({ sessionId: "", resources: { messages: [] }, hasEarlier: false }),
    route: DIRS_ROUTE,
    ...overrides,
  };
}

/**
 * 挂一个场景，并把**真实的 `Shell`** 挂在它的 store 上（挂在场景容器之外，互不影响）。
 *
 * `content` 是外壳内容区（`data-shell-content`）里渲染的东西，缺省是一个空 `<div>`：外壳自己的
 * 判定（顶栏呈现与否、胶囊）不看页面内容就已经可测。要断言**页面自己**呈现了什么时得传真东西，
 * 那里传 `createElement(Stack)`——路由替身的出口，与 `app/_layout.tsx` 里的那一份等价。
 *
 * 卸载顺序：先卸外壳容器，再卸场景（场景的 `unmount` 会 dispose 组合根）。
 */
async function withShell(
  options: ScenarioOptions,
  body: (context: { readonly container: HTMLElement; readonly mounted: Mounted }) => Promise<void>,
  content: ReactNode = createElement("div", null),
): Promise<void> {
  const mounted = await mountScenario(options);
  const container = document.createElement("div");
  document.body.append(container);
  const root: Root = createRoot(container);
  root.render(
    createElement(ClientStoreProvider, {
      store: mounted.composition?.store ?? null,
      // `children` 走 props 而不是 `createElement` 的第三参数：`ClientStoreProvider` 的 props
      // 把 `children` 声明为必填，第三参数那条重载因此不满足它。
      children: createElement(Shell, null, content),
    }),
  );
  try {
    await body({ container, mounted });
  } finally {
    root.unmount();
    container.remove();
    await mounted.unmount();
  }
}

const checks: readonly Check[] = [
  {
    name: "UI-SHELL-1 根路由呈现全局顶栏：在线时胶囊只有正常档与设备标识，没有状态短词，也没有第二处状态锚点",
    async run() {
      await withShell(baseScenario({ extraRoutes: [HOSTS_ROUTE] }), async ({ container, mounted }) => {
        await waitFor(
          () => query(container, '[data-shell-top-bar="visible"]') !== null,
          "根路由呈现全局顶栏",
        );
        await waitFor(() => mounted.composition?.store.state.connection.state === "online", "连接机进入 online");

        const capsule = query(container, "[data-device-capsule]");
        expectThat(capsule !== null, "顶栏里必须有设备胶囊这一个控件");
        expectEqual(capsule?.getAttribute("data-capsule-tone"), "ok", "在线时胶囊档位");
        expectEqual(capsule?.getAttribute("data-capsule-state"), "online", "胶囊上的连接状态");
        expectEqual(capsule?.getAttribute("data-action"), "open-hosts", "胶囊即进入主机与连接面板的入口");
        expectEqual(query(container, "[data-capsule-short-word]"), null, "在线时胶囊不追加短词");
        expectEqual(query(container, "[data-connection-state]"), null, "外壳里不得出现第二个状态徽标");

        // 真的点一次：路由替身的当前路径必须变成 `/hosts`（而不是只挂了一个属性）。
        click(capsule, "设备胶囊");
        await waitFor(() => currentPath() === "/hosts", "点击胶囊后进入主机与连接面板");
      });
    },
  },
  {
    name: "UI-SHELL-2 配对路由不呈现全局顶栏：同一份外壳换到根路径就呈现（差分判据）",
    async run() {
      await withShell(baseScenario({ route: PAIR_ROUTE }), async ({ container }) => {
        await waitFor(() => query(container, '[data-shell="root"]') !== null, "外壳本体渲染出来");
        expectEqual(query(container, '[data-shell-top-bar="visible"]'), null, "配对路由的全局顶栏锚点");
        expectEqual(query(container, "[data-device-capsule]"), null, "配对路由的设备胶囊");
        expectThat(
          query(container, '[data-shell-content="true"]') !== null,
          "配对路由仍渲染路由内容区（只是没有全局顶栏）",
        );

        // 差分：把当前路径换成根路径，同一个外壳实例立刻呈现顶栏与胶囊。
        mountRoute(DIRS_ROUTE);
        await waitFor(
          () => query(container, '[data-shell-top-bar="visible"]') !== null,
          "换到根路径后顶栏出现",
        );
        expectThat(query(container, "[data-device-capsule]") !== null, "换到根路径后胶囊出现");
      });
    },
  },
  {
    name: "UI-SHELL-3 配对页呈现自己的头部：全局顶栏缺席，页头随路由进退（差分判据）",
    async run() {
      await withShell(
        baseScenario({ route: PAIR_ROUTE, extraRoutes: [DIRS_ROUTE] }),
        async ({ container }) => {
          // 内容区走路由替身的出口：等的是 `app/pair.tsx` 真的渲染出来，而不是外壳里摆了某个
          // `data-*`（页面挂不上时这条等待超时判红）。
          await waitFor(
            () => query(container, '[data-shell="root"] [data-route-header="pair"]') !== null,
            "真实配对页渲染进外壳的内容区",
          );

          // (a) 全局顶栏缺席：锚点口径与 UI-SHELL-1 相同（顶栏锚点 + 它里面的设备胶囊）。
          expectEqual(query(container, '[data-shell-top-bar="visible"]'), null, "配对页的全局顶栏锚点");
          expectEqual(query(container, "[data-device-capsule]"), null, "配对页的设备胶囊");

          // (b) 该页有自己的头部，且可区分：它挂在路由内容区里，并带着离开本页的入口。
          expectThat(
            query(container, '[data-shell-content="true"] [data-route-header="pair"]') !== null,
            "配对页的页头属于路由内容区（不是外壳自带的顶栏）",
          );
          expectThat(
            query(container, '[data-route-header="pair"] [data-action="back-to-dirs"]') !== null,
            "页头里带着离开本页的入口：顶栏缺席不等于丢掉返回根路径的能力",
          );

          // 差分：换到根路径，页头必须随页面一起让位给全局顶栏——这才证明页头是**这一页自己的**，
          // 而不是外壳顺手画的；把 `app/pair.tsx` 的页头删掉，上面那两处锚点断言立刻判红。
          mountRoute(DIRS_ROUTE);
          await waitFor(
            () => query(container, '[data-shell-top-bar="visible"]') !== null,
            "换到根路径后全局顶栏出现",
          );
          expectEqual(query(container, '[data-route-header="pair"]'), null, "配对页头随页面离开内容区");
        },
        createElement(Stack),
      );
    },
  },
];

export const UI_SHELL_CHECKS = checks;

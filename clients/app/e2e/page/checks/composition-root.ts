/**
 * 组合根接线在**真实浏览器**里的实测。
 *
 * ## 为什么这组用例是本工作包的核心交付
 *
 * 前几轮检视登记了两条事实：
 *
 * 1. 既有组件测试全部走 `renderToStaticMarkup`——**没有事件、没有 `useEffect`**。
 *    因此 `app/_layout.tsx` 里 `useEffect` 中的 `await composition.start()`
 *    在任何测试中都没有执行过，组合根的接线正确性此前**没有任何自动化证据**。
 * 2. `dispose()` 置 `disposed = true` 且 `start()` 首行 `if (disposed) return`，
 *    React StrictMode 的双调用会让第二次 `start()` 变成 no-op、页面永远停在
 *    `RuntimeUnavailable`。当前配置未启用 StrictMode，但「页面确实能离开
 *    `RuntimeUnavailable`」这件事此前从未被观测过。
 *
 * CR-1 直接挂**真实的 `app/_layout.tsx`**（无参 `createComposition()`、真实 IndexedDB、
 * 真实 WebCrypto、真实 `WebSocket`），核对页面确实离开 `RuntimeUnavailable`；
 * CR-2 走同一条 `start()` 路径但把 `openSocket` 换成页内假主机，核对握手、快照与
 * 状态机的**每一段**都真的跑到了；
 * CR-3 是负向对照：伪造的 `hostProof` 必须让连接进入阻断态，
 * 证明验签不是被绕过的橡皮图章。
 */

import { AGENTS, DIRS_ROUTE, WORKSPACES_WITH_PATHS, sessionsForBothDirectories } from "../fixtures";
import {
  CANONICAL_ORIGIN,
  expectEqual,
  expectThat,
  mountRootLayout,
  mountScenario,
  mountRoute,
  query,
  queryAll,
  waitFor,
} from "../support";
import type { Check, ScenarioOptions } from "../support";

/** 走真实 `start()`、但 socket 换成页内假主机的场景。 */
function baseScenario(overrides: Partial<ScenarioOptions> = {}): ScenarioOptions {
  return {
    canonicalOrigin: CANONICAL_ORIGIN,
    workspaces: WORKSPACES_WITH_PATHS,
    sessions: sessionsForBothDirectories(),
    agents: AGENTS,
    scopes: ["session.list", "session.read", "session.create"],
    tamperHostProof: false,
    deliverSnapshot: true,
    barrierGlobalSequence: "10",
    events: [],
    readResult: () => ({ sessionId: "", resources: { messages: [] }, hasEarlier: false }),
    route: DIRS_ROUTE,
    ...overrides,
  };
}

const checks: readonly Check[] = [
  {
    name: "CR-1 真实的 app/_layout.tsx：start() 在浏览器里真的跑了，页面离开 RuntimeUnavailable",
    async run() {
      mountRoute(DIRS_ROUTE);
      const mounted = await mountRootLayout();
      try {
        // 先等目录页真的出现，再断言 RuntimeUnavailable 已消失：挂载后 React 的首帧
        // 还没跑，此时「没有 unavailable」与「什么都还没渲染」长得一模一样。
        await waitFor(
          () => query(mounted.container, '[data-route="dirs"]') !== null,
          "真实目录页渲染",
        );
        expectThat(
          query(mounted.container, '[data-runtime="unavailable"]') === null,
          "组合根装配完成后页面必须离开 RuntimeUnavailable",
        );
        // 主机面板出现「已配对」，证明 start() 读到了预置的 paired-host/<origin> 记录。
        await waitFor(
          () => query(mounted.container, '[data-host-identity="paired"]') !== null,
          "组合根读到预置的已配对主机记录",
        );
        expectEqual(
          query(mounted.container, "[data-host-address]")?.getAttribute("data-host-address") ?? null,
          CANONICAL_ORIGIN,
          "主机面板显示的规范化来源",
        );
        expectThat(
          query(mounted.container, '[data-connection-state="unpaired"]') === null,
          "状态机不得停在 unpaired",
        );
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "CR-2 组合根 → SyncClient → ClientStore 的接线：握手、快照与状态机每一段都跑到了",
    async run() {
      const mounted = await mountScenario(baseScenario());
      try {
        await waitFor(
          () => mounted.composition?.store.state.connection.state === "online",
          "连接机进入 online",
        );
        const state = mounted.composition?.store.state;

        // 设备证明必须由 IndexedDB 里那把不可导出的私钥真签出来。
        expectEqual(mounted.host?.deviceProofs.length ?? 0, 1, "客户端签出的设备证明条数");
        expectThat(
          (mounted.host?.deviceProofs[0] ?? "").length > 40,
          "设备证明应是一条真实签名（base64url 文本）",
        );
        // 连接 URL 由 `syncUrlFor` 从规范化来源推导。
        expectEqual(mounted.host?.socketUrls[0] ?? null, "wss://127.0.0.1:1/sync", "出站连接 URL");

        // 快照经真实两级摘要校验后才允许提交。
        expectThat(state?.resources !== null, "快照应已验证并进入 ClientStore");
        expectEqual(state?.resources?.workspaces.length ?? 0, 2, "快照里的目录数");
        expectEqual(state?.resources?.sessions.length ?? 0, 2, "快照里的会话数");
        expectEqual(state?.scopes?.length ?? 0, 3, "认证授予的 scopes");

        // 选择器链路：store → 模型 → 页面。
        expectThat(
          queryAll(mounted.container, "li[data-directory-alias]").length === 2,
          "目录页必须渲染出两条目录（模型已到达页面）",
        );
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "CR-3 负向对照：伪造的 hostProof 必须让连接进入阻断态，且客户端拒绝签设备证明",
    async run() {
      const mounted = await mountScenario(baseScenario({ tamperHostProof: true }));
      try {
        await waitFor(
          () => mounted.composition?.store.state.connection.state === "identity_changed",
          "连接进入 identity_changed 阻断态",
        );
        // 验签失败发生在签设备证明**之前**：客户端不会把挑战传给一条身份不明的连接。
        expectEqual(mounted.host?.deviceProofs.length ?? -1, 0, "验签失败后不得签出设备证明");
        expectThat(
          query(mounted.container, '[data-blocking-detail="host_identity_changed"]') !== null,
          "页面必须呈现阻断原因",
        );
        // 阻断不是崩溃：目录页仍然渲染（此刻是「尚未收到第一份快照」）。
        expectThat(query(mounted.container, '[data-route="dirs"]') !== null, "阻断态下页面仍应渲染目录页");
        expectThat(
          query(mounted.container, '[data-directory-state="loading"]') !== null,
          "阻断态下目录页应是「等待快照」而不是伪造空数据",
        );
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "CR-4 预置的已配对记录被改坏时按「从未配对」处理：状态机停在 unpaired，页面不得伪造空目录",
    async run() {
      const mounted = await mountScenario(baseScenario({ corruptPairedHostRecord: true }));
      try {
        await waitFor(
          () => mounted.composition?.store.state.connection.state === "unpaired",
          "记录非法时状态机停在 unpaired",
        );
        // 状态机断言看的是 store，可能先于 React 首帧满足；DOM 断言前先等页面渲染。
        await waitFor(
          () => query(mounted.container, '[data-route="dirs"]') !== null,
          "未配对时目录页仍应渲染",
        );
        // 验签绝不被跳过：连接连发都没发，设备证明自然也没有。
        expectEqual(mounted.host?.sent.length ?? -1, 0, "记录非法时不得发出任何 wire 帧");
        // 「未配对」不得被呈现成「本机没有任何目录」——那会诱导用户去删配置。
        expectThat(
          query(mounted.container, '[data-empty-state="no_directories"]') === null,
          "未配对时不得呈现「本机没有任何目录」",
        );
        expectThat(
          query(mounted.container, '[data-directory-state="loading"]') !== null,
          "未配对时目录页应是「尚未收到第一份快照」",
        );
        expectThat(
          query(mounted.container, '[data-host-identity="unpaired"]') !== null,
          "主机面板应明确呈现「尚未配对」",
        );
      } finally {
        await mounted.unmount();
      }
    },
  },
];

export const COMPOSITION_CHECKS = checks;
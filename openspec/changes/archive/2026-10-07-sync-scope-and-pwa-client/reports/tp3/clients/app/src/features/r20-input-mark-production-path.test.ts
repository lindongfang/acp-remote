// 【交接副本】来源 worktree .worktrees/tp3 的 clients/app/src/features/r20-input-mark-production-path.test.ts
// 交付提交 6e8307096629dcfd3fa63ac20559e25b0a3e7a7a（分支 feat/tp3），基线 f6ea252。
// 用途：premerge 门禁按「相对变更目录」解析 test_delivery.artifacts 的可读性；本副本在权威交付物之外。
// 除顶部本 4 行说明外与来源逐字节相同（权威可运行副本在 worktree 内，由 PV3 执行）。
/**
 * R20：imported 会话的输入**在 `ClientStore` 这条生产路径上**不得被标记为已发送。
 *
 * ## 为什么必须走 `ClientStore` 而不是直调判定函数
 *
 * `src/platform/r20-no-content-cache.test.ts` 断言的是 `mayMarkInputAsSent`，但它在全仓
 * **只有定义与 re-export，没有任何生产调用方**（孤儿导出）。真正决定界面能否标记已发送的是
 * `conversationCapabilities`（`features/conversation-model.ts`），它由
 * `ClientStore.conversationPage()` 经 `client-store.ts` 调用，用的是另一套判据。
 * 因此那条断言**不覆盖生产路径**：把 `conversationCapabilities` 的 `sendable` 判据改松
 * （例如去掉 `origin === "local"` 这一项）之后，全仓仍会全绿。
 *
 * 本文件把判定放在 `ClientStore.apply()` → 连接状态机 → `conversationPage()` 这条完整路径上，
 * 并先断言连接确实处于 `online`——否则「不可标记」会因连接未在线而平凡成立（既有
 * `r17-context-usage.test.ts` 的输入标记断言就落在 `online: false` 的场景里，抓不住来源判据）。
 *
 * ## 判别力
 *
 * - 反例 1：若 `sendable` 只看「连接在线」，`remote + online` 那条会变红；
 * - 反例 2：若 `sendable` 对 remote 来源放宽成「在线即可」，同上；
 * - 对照：本节点来源（`local`）在线时必须为真——否则上两条是「组件永远不出可发送」的平凡通过。
 */

import { describe, expect, it } from "vitest";

import type { SessionSummary } from "../protocol";
import { ClientStore } from "./client-store";
import { resources, session, storeWith } from "./testing/fixtures";

const REMOTE_SESSION = "5d73cd10-a465-43cd-b3f1-704e2d49e99e";
const LOCAL_SESSION = "aaaaaaaa-0000-4000-8000-000000000001";

/** imported（其它节点来源）会话的 `origin`；`online` 描述来源节点是否可达。 */
function remoteOrigin(online: boolean): SessionSummary["origin"] {
  return {
    kind: "remote",
    ownerNodeId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
    exportId: "export-1",
    originEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13",
    online,
  };
}

/**
 * 建一个已喂入快照、**且连接确实推到 `online`** 的仓库。
 *
 * 单独发 `online` 会被迁移表拒绝，状态会停在 `unpaired`，下游能力判定随之失真——
 * 那正是「用例绕过状态机」会出现的假绿。因此这里逐步走完整条路径。
 */
function onlineStore(sessions: readonly SessionSummary[]): ClientStore {
  const { store } = storeWith({ resources: resources({ sessions }) });
  store.pairSucceeded();
  store.apply({ kind: "authenticating" });
  store.apply({ kind: "replaying", cursor: null });
  store.apply({
    kind: "online",
    cursor: { serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13", globalSequence: "10" },
  });
  return store;
}

describe("R20：imported 会话的输入在 ClientStore 生产路径上不可标记为已发送", () => {
  it("连接在线时，imported 来源（无论来源节点是否可达）仍不可标记", () => {
    const store = onlineStore([
      session({ id: REMOTE_SESSION, state: "running", origin: remoteOrigin(true) }),
      session({ id: LOCAL_SESSION, state: "running", origin: { kind: "local" } }),
    ]);

    // 前提守卫：连接真的在线。否则下面的 false 可能只是「没连上」的平凡结果。
    expect(store.state.connection.state).toBe("online");

    const remotePage = store.conversationPage(REMOTE_SESSION);
    expect(remotePage).not.toBeNull();
    // 反例：若 `sendable` 只看在线（或对 remote 来源放宽），这里会是 true → 断言变红。
    expect(remotePage?.capabilities.canSend).toBe(false);

    // 对照：本节点来源在同一连接状态下必须可发送——否则上一条是平凡通过。
    const localPage = store.conversationPage(LOCAL_SESSION);
    expect(localPage?.capabilities.canSend).toBe(true);
  });

  it("来源节点不可达时同样不可标记：与「在线」无关的那一半判据", () => {
    const store = onlineStore([
      session({ id: REMOTE_SESSION, state: "running", origin: remoteOrigin(false) }),
    ]);
    expect(store.state.connection.state).toBe("online");

    // 反例：若 `sendable` 只查 `origin.kind === "local"` 而漏了连接侧，这里仍是 false（不判错）；
    // 但若把 remote 来源一律放开，这里立刻变红——它把「来源离线」这条 spec 场景钉在生产路径上。
    expect(store.conversationPage(REMOTE_SESSION)?.capabilities.canSend).toBe(false);
  });
});

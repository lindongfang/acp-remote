/**
 * 结构化历史的分页原语与 feature 门控（D2；`pwa-web-client` 的「两套独立分页」）。
 *
 * ## 判别力
 *
 * - **序号比较必须按数值**：`"10"` 与 `"9"` 的字典序给出错答案（`"10" < "9"`），本用例用
 *   两条位数不同的序号把这条钉住。
 * - **页码标记缺席不等于「没有更早」**：`historyHasEarlier` 缺席只说明这次请求没要历史，
 *   因此 `toLoadedHistoryPage` 返回 `hasEarlier: false` 而**不**抛错；为真却没有条目才是
 *   自相矛盾的结果（无法构造续取游标）。
 * - **分量缺失即抛错**：`sessionSequence` 是 schema 的必填字段，缺了就是协议违规，客户端不得
 *   猜一个序号替补（与消息侧的 `(createdAt, messageId)` 同一口径）。
 * - **feature 门控**：客户端广告 `core.session-history.v1`，但它是**非必需** feature——未协商
 *   时只是不发该 include（`ClientStore.#includesFor` 负责），握手不因此失败。
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import type { SessionReadResult, SnapshotItemHistory } from "../protocol";
import { repoRoot } from "../protocol/paths";
import {
  IncompleteReadCursorError,
  REQUIRED_FEATURES,
  SESSION_HISTORY_FEATURE,
  SUPPORTED_FEATURES,
  assertHistoryPagesDoNotOverlap,
  earliestHistoryCursor,
  historyCursorOf,
  toLoadedHistoryPage,
} from "./index";

/**
 * 一条结构化历史条目（`session.read` 的 `snapshotItem.history`）。
 *
 * 就地构造，**不**取 `src/features/testing/` 的共享夹具：夹具属第 3 层，本层（第 5 层）
 * 取它就是反向依赖（`src/layers.ts` 的不变量，`src/layers.test.ts` 会因此变红）。
 * 与 `paging.test.ts` 的消息构造器同一约定。
 */
function historyItem(input: {
  readonly eventId: string;
  readonly eventType: string;
  readonly sessionSequence?: string;
  readonly createdAt?: string;
  readonly view?: Record<string, unknown>;
}): SnapshotItemHistory {
  return {
    eventId: input.eventId,
    eventType: input.eventType,
    sessionSequence: input.sessionSequence ?? "1",
    createdAt: input.createdAt ?? "2026-10-01T00:00:02.000Z",
    view: input.view ?? {},
  };
}

/** 一页历史读取结果（只带历史那一份资源）。 */
function historyPage(items: readonly SnapshotItemHistory[], hasEarlier: boolean | undefined): SessionReadResult {
  return {
    sessionId: "aaaaaaaa-0000-4000-8000-000000000001",
    resources: { history: items },
    hasEarlier: false,
    ...(hasEarlier === undefined ? {} : { historyHasEarlier: hasEarlier }),
  };
}

describe("D2 历史分页：会话内序号游标", () => {
  it("最早一条按**数值**取（位数不同也不会选错）", () => {
    const items = [
      historyItem({ eventId: "e-1", eventType: "turn.started", sessionSequence: "10" }),
      historyItem({ eventId: "e-2", eventType: "turn.started", sessionSequence: "9" }),
      historyItem({ eventId: "e-3", eventType: "turn.started", sessionSequence: "100" }),
    ];
    // 反例：用字典序比较时这里会是 "10" → 断言变红。
    expect(earliestHistoryCursor(items)).toBe("9");
    expect(earliestHistoryCursor([])).toBeNull();
  });

  it("分量缺失或形状非法即抛错，不猜测也不补全", () => {
    expect(() => historyCursorOf(historyItem({ eventId: "e-1", eventType: "turn.started", sessionSequence: "" }))).toThrow(
      IncompleteReadCursorError,
    );
    expect(() =>
      historyCursorOf({ eventId: "e-1", eventType: "turn.started", sessionSequence: "01", createdAt: "x", view: {} }),
    ).toThrow(IncompleteReadCursorError);
  });

  it("historyHasEarlier 缺席不等于「没有更早」；为真却无条目是自相矛盾", () => {
    const items = [historyItem({ eventId: "e-1", eventType: "turn.started", sessionSequence: "4" })];
    const absent = toLoadedHistoryPage(historyPage(items, undefined));
    expect(absent.hasEarlier).toBe(false);
    expect(absent.nextBeforeHistory).toBeNull();

    const present = toLoadedHistoryPage(historyPage(items, true));
    expect(present.hasEarlier).toBe(true);
    expect(present.nextBeforeHistory).toBe("4");

    expect(() => toLoadedHistoryPage(historyPage([], true))).toThrow(/未返回任何条目/);
  });

  it("相邻两页历史条目重叠（同一 eventId）时抛错", () => {
    const shared = historyItem({ eventId: "e-dup", eventType: "turn.started", sessionSequence: "5" });
    expect(() => assertHistoryPagesDoNotOverlap([shared], [shared])).toThrow(/重叠/);
    // 不重叠：序号更小的一页合法。
    expect(() =>
      assertHistoryPagesDoNotOverlap(
        [historyItem({ eventId: "e-2", eventType: "turn.started", sessionSequence: "2" })],
        [shared],
      ),
    ).not.toThrow();
  });
});

describe("feature 门控：core.session-history.v1 是客户端广告的非必需 feature", () => {
  it("与机器登记一致，且不在必需列表里", () => {
    const registry = JSON.parse(
      readFileSync(join(repoRoot, "compatibility", "features", "v1", "features.json"), "utf8"),
    ) as { protocols: { sync: { features: { id: string; required: boolean }[] } } };
    const sync = registry.protocols.sync.features;
    const entry = sync.find((feature) => feature.id === SESSION_HISTORY_FEATURE);
    // 反例：客户端广告一个登记表里没有的 feature → 这里会是 undefined → 断言变红。
    expect(entry).toBeDefined();
    expect(entry?.required).toBe(false);
    expect(SUPPORTED_FEATURES).toContain(SESSION_HISTORY_FEATURE);
    // 未协商它不会让握手失败（非必需），因此服务端不广告时客户端只是退回不带该 include。
    expect(REQUIRED_FEATURES).not.toContain(SESSION_HISTORY_FEATURE);
  });
});

/**
 * R20：imported 资源不落盘、离线只显示元数据；本节点摘要走固定容量缓存。
 *
 * 判别力来源：
 * - 给 imported 加任何持久化例外 → 七类内容全部 `memory_only` 的用例变红；
 * - 让本地正文也进缓存 → 「只有摘要可持久化」用例变红；
 * - 离线时把输入标记为已发送 → 「来源离线时不标记已发送」用例变红。
 */

import { describe, expect, it } from "vitest";

import { mayMarkInputAsSent, routeContent } from "./imported-content";
import type { ContentKind, SessionOrigin } from "./imported-content";

const IMPORTED: SessionOrigin = {
  kind: "remote",
  ownerNodeId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
};
const LOCAL: SessionOrigin = { kind: "local" };

/** spec 逐项点名的七类正文类内容。 */
const BODY_KINDS: readonly ContentKind[] = [
  "prompt",
  "tool_content",
  "diff",
  "terminal_output",
  "attachment",
  "acp_raw",
  "summary",
];

describe("R20：imported 来源的一切内容都不进持久存储", () => {
  it("七类内容全部判为仅内存", () => {
    for (const kind of BODY_KINDS) {
      // 反例：任何一条走 local_summary_cache 分支 → 断言变红。
      expect(routeContent(IMPORTED, kind), kind).toEqual({ destination: "memory_only" });
    }
  });

  it("本地内容：只有摘要可进固定容量摘要缓存", () => {
    expect(routeContent(LOCAL, "summary")).toEqual({ destination: "local_summary_cache" });
    // 反例：若本地正文也进缓存，这里会变 → 断言变红（D1：明细一律现取）。
    for (const kind of BODY_KINDS.filter((item) => item !== "summary")) {
      expect(routeContent(LOCAL, kind), kind).toEqual({ destination: "memory_only" });
    }
  });

  it("同一份内容在两种来源下判定不同（判定确实依赖来源）", () => {
    expect(routeContent(LOCAL, "summary")).not.toEqual(routeContent(IMPORTED, "summary"));
  });
});

describe("R20：来源离线时输入不被标记为已发送", () => {
  it("imported 来源即便连接在线也不得标记为已发送（Owner 可能不可达）", () => {
    // 反例：若只看 online，这里会是 true → 断言变红。
    expect(mayMarkInputAsSent({ online: true, origin: IMPORTED })).toBe(false);
  });

  it("本地来源只有在连接在线时才可标记为已发送", () => {
    expect(mayMarkInputAsSent({ online: true, origin: LOCAL })).toBe(true);
    // 反例：若允许离线排队，这里会是 true → 断言变红（第一阶段不离线排队 prompt）。
    expect(mayMarkInputAsSent({ online: false, origin: LOCAL })).toBe(false);
  });

  it("两种来源在离线时都不标记", () => {
    expect(mayMarkInputAsSent({ online: false, origin: LOCAL })).toBe(false);
    expect(mayMarkInputAsSent({ online: false, origin: IMPORTED })).toBe(false);
  });
});
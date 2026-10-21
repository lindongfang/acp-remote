/**
 * 远程原子读取的合并判定（design D4）：覆盖页不重复拼接、旧页不回滚、未返回消息的事件保留。
 */

import { describe, expect, it } from "vitest";

import type { OriginCursor } from "../protocol";
import {
  coverageAllowsReplace,
  coverageFromRead,
  isDeltaCovered,
  mergeRemoteCoverage,
  pruneCoveredDeltas,
} from "./remote-merge";

const EPOCH = "c1d2e3f4-5061-4728-8394-95a6b7c8d9e0";

function cursor(sequence: string): OriginCursor {
  return { originEpoch: EPOCH, originSequence: sequence };
}

describe("远程读取覆盖合并", () => {
  it("覆盖页包含的增量不再拼接，之后的增量保留", () => {
    const coverage = coverageFromRead([
      { messageId: "m1", through: cursor("20"), lastDeltaIndex: "2" },
    ]);
    expect(isDeltaCovered(coverage, "m1", "0")).toBe(true);
    expect(isDeltaCovered(coverage, "m1", "2")).toBe(true);
    expect(isDeltaCovered(coverage, "m1", "3")).toBe(false);
    // 未返回的消息不受本页覆盖。
    expect(isDeltaCovered(coverage, "m2", "0")).toBe(false);

    const deltas = new Map([
      ["0", "你"],
      ["1", "好"],
      ["2", "！"],
      ["3", "之后"],
    ]);
    const pruned = pruneCoveredDeltas(deltas, coverage.get("m1"));
    expect([...pruned.entries()]).toEqual([["3", "之后"]]);
  });

  it("同一条消息的覆盖只前进：旧页不得替换较新内容", () => {
    const newer = coverageFromRead([{ messageId: "m1", through: cursor("20"), lastDeltaIndex: "2" }]);
    const older = coverageFromRead([{ messageId: "m1", through: cursor("10"), lastDeltaIndex: "1" }]);

    expect(coverageAllowsReplace(newer.get("m1"), older.get("m1"))).toBe(false);
    expect(coverageAllowsReplace(older.get("m1"), newer.get("m1"))).toBe(true);

    const merged = mergeRemoteCoverage(newer, older);
    expect(merged.get("m1")?.through.originSequence).toBe("20");
  });

  it("覆盖只适用于本次返回的消息：未返回消息的覆盖位置不被其它页改写", () => {
    const pageA = coverageFromRead([{ messageId: "m1", through: cursor("20"), lastDeltaIndex: null }]);
    const pageB = coverageFromRead([{ messageId: "m2", through: cursor("21"), lastDeltaIndex: null }]);
    const merged = mergeRemoteCoverage(pageA, pageB);
    expect(merged.get("m1")?.through.originSequence).toBe("20");
    expect(merged.get("m2")?.through.originSequence).toBe("21");
  });
});

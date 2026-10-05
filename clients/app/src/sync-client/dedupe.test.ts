/**
 * R14：按稳定 `eventId` 去重、不跳过、无丢失区间。
 *
 * 判别力来源：
 * - 把去重键换成连接序号 → 「同一 eventId 不同 messageId」用例变红；
 * - 把去重键换成 `messageId` → 同上（WSS 信封 ID 每次重连都变）；
 * - 去掉缺口判定 → 「序号跳跃」用例变红；
 * - 把「序号已处理过」当作新事件 → 「重复事件不推进游标」用例变红。
 */

import { describe, expect, it } from "vitest";

import { EventLedger, isImportedEventIdConsistent } from "./dedupe";
import type { EventVerdict } from "./dedupe";
import { makeEvent } from "./testing/harness";

const EPOCH = "00384a03-bc90-4095-b65d-82fb8cc47e13";

function evaluate(ledger: EventLedger, event: ReturnType<typeof makeEvent>): EventVerdict {
  const verdict = ledger.evaluate(event);
  if (verdict.kind === "new") ledger.commit(event);
  return verdict;
}

describe("R14：按 eventId 去重", () => {
  it("同一 eventId 重复投递判为重复", () => {
    const ledger = new EventLedger({ serverEpoch: EPOCH });
    const event = makeEvent({ globalSequence: "1", eventId: "aaaaaaaa-0000-4000-8000-000000000001" });

    expect(evaluate(ledger, event)).toEqual({ kind: "new" });
    expect(evaluate(ledger, event)).toEqual({ kind: "duplicate" });
    // 游标不因重复前进。
    expect(ledger.cursor?.globalSequence).toBe("1");
  });

  it("去重键是 eventId：messageId 不同但 eventId 相同仍判重复", () => {
    const ledger = new EventLedger({ serverEpoch: EPOCH });
    const first = makeEvent({ globalSequence: "1", eventId: "aaaaaaaa-0000-4000-8000-000000000001" });
    // 换一个 `messageId`（模拟重连后服务端重新分配信封 ID）但保留 `eventId`。
    const second = { ...first, messageId: "ffffffff-1111-4111-8111-999999999999" };

    expect(evaluate(ledger, first)).toEqual({ kind: "new" });
    // 反例：若去重键用 messageId，这里会判成 new 并再次呈现 → 断言变红。
    expect(evaluate(ledger, second)).toEqual({ kind: "duplicate" });
  });

  it("去重键不是连接序号：序号相同但 eventId 不同仍判重复", () => {
    const ledger = new EventLedger({ serverEpoch: EPOCH });
    const first = makeEvent({ globalSequence: "1", eventId: "aaaaaaaa-0000-4000-8000-000000000001" });
    const second = makeEvent({ globalSequence: "1", eventId: "aaaaaaaa-0000-4000-8000-000000000002" });

    expect(evaluate(ledger, first)).toEqual({ kind: "new" });
    // 反例：若键是 `globalSequence`，第二条会被当新事件 → 断言变红。
    expect(evaluate(ledger, second)).toEqual({ kind: "duplicate" });
  });
});

describe("R14：连续性与丢失区间", () => {
  it("从续接游标起步：第一条必须是游标的下一条", () => {
    const ledger = new EventLedger({ serverEpoch: EPOCH, resumeFrom: { serverEpoch: EPOCH, globalSequence: "10" } });
    expect(ledger.cursor?.globalSequence).toBe("10");

    expect(evaluate(ledger, makeEvent({ globalSequence: "12", eventId: "aaaaaaaa-0000-4000-8000-000000000012" }))).toEqual({
      kind: "gap",
      expected: "11",
      received: "12",
    });
    expect(evaluate(ledger, makeEvent({ globalSequence: "11", eventId: "aaaaaaaa-0000-4000-8000-000000000011" }))).toEqual({
      kind: "new",
    });
  });

  it("去重键是 eventId 而非 messageId/序号：已见过的 ID 即使换了序号也不得重新呈现", () => {
    const ledger = new EventLedger({ serverEpoch: EPOCH });
    const original = makeEvent({ globalSequence: "1", eventId: "aaaaaaaa-0000-4000-8000-000000000001" });
    expect(evaluate(ledger, original)).toEqual({ kind: "new" });

    // 同一条事件的 `eventId` 不变，但重放时服务端换了 `messageId` 与 `globalSequence`
    // （例如事件库重建后序号重新分配）。按 `eventId` 去重必须丢弃它；
    // 按 `messageId` 或按序号去重的实现会把它当新事件，游标跳到 5 并重复呈现。
    const replayed = makeEvent({ globalSequence: "5", eventId: "aaaaaaaa-0000-4000-8000-000000000001" });
    expect(evaluate(ledger, replayed)).toEqual({ kind: "duplicate" });
    expect(ledger.cursor?.globalSequence).toBe("1");
    expect(ledger.seenCount).toBe(1);
  });

  it("首次同步从序号 1 开始；不从 1 开始判缺口", () => {
    const ledger = new EventLedger({ serverEpoch: EPOCH });
    expect(evaluate(ledger, makeEvent({ globalSequence: "1", eventId: "aaaaaaaa-0000-4000-8000-000000000001" }))).toEqual({
      kind: "new",
    });
    expect(ledger.cursor?.globalSequence).toBe("1");
  });

  it("大序号按十进制字符串比较而非字典序", () => {
    const ledger = new EventLedger({ serverEpoch: EPOCH, resumeFrom: { serverEpoch: EPOCH, globalSequence: "9" } });
    // "10" 在字典序上小于 "9"，按数值才是下一条。
    expect(evaluate(ledger, makeEvent({ globalSequence: "10", eventId: "aaaaaaaa-0000-4000-8000-000000000010" }))).toEqual({
      kind: "new",
    });
  });

  it("缺口不推进游标，后续补齐后可继续", () => {
    const ledger = new EventLedger({ serverEpoch: EPOCH, resumeFrom: { serverEpoch: EPOCH, globalSequence: "1" } });
    expect(evaluate(ledger, makeEvent({ globalSequence: "3", eventId: "aaaaaaaa-0000-4000-8000-000000000003" }))).toEqual({
      kind: "gap",
      expected: "2",
      received: "3",
    });
    expect(ledger.cursor?.globalSequence).toBe("1");

    expect(evaluate(ledger, makeEvent({ globalSequence: "2", eventId: "aaaaaaaa-0000-4000-8000-000000000002" }))).toEqual({
      kind: "new",
    });
    expect(evaluate(ledger, makeEvent({ globalSequence: "3", eventId: "aaaaaaaa-0000-4000-8000-000000000003" }))).toEqual({
      kind: "new",
    });
    expect(ledger.cursor?.globalSequence).toBe("3");
  });
});

describe("去重窗口有界", () => {
  it("超出保留窗口时按插入顺序淘汰最旧的键", () => {
    const ledger = new EventLedger({ serverEpoch: EPOCH, retention: 3 });
    for (let index = 1; index <= 5; index += 1) {
      expect(evaluate(ledger, makeEvent({ globalSequence: String(index), eventId: `aaaaaaaa-0000-4000-8000-${String(index).padStart(12, "0")}` }))).toEqual({ kind: "new" });
    }
    expect(ledger.seenCount).toBe(3);
    // 被淘汰的第一条重投时靠「序号已处理过」判为重复，而不是被当新事件。
    expect(evaluate(ledger, makeEvent({ globalSequence: "1", eventId: "aaaaaaaa-0000-4000-8000-000000000001" }))).toEqual({
      kind: "duplicate",
    });
  });
});

describe("imported 事件 ID 一致性（§9.6）", () => {
  it("remoteOrigin.originEventId 必须等于 eventId", () => {
    const matching = makeEvent({
      globalSequence: "1",
      eventId: "aaaaaaaa-0000-4000-8000-000000000001",
      remoteOrigin: {
        ownerNodeId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
        exportId: "export-1",
        originEpoch: "11111111-1111-4111-8111-111111111111",
        originEventId: "aaaaaaaa-0000-4000-8000-000000000001",
        originSequence: "5",
      },
    });
    expect(isImportedEventIdConsistent(matching)).toBe(true);

    const mismatched = makeEvent({
      globalSequence: "1",
      eventId: "aaaaaaaa-0000-4000-8000-000000000002",
      remoteOrigin: {
        ownerNodeId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
        exportId: "export-1",
        originEpoch: "11111111-1111-4111-8111-111111111111",
        originEventId: "aaaaaaaa-0000-4000-8000-000000000001",
        originSequence: "5",
      },
    });
    // 反例：若不校验，跨节点同一事件会在 Access 与 Owner 两侧各呈现一次 → 断言变红。
    expect(isImportedEventIdConsistent(mismatched)).toBe(false);
  });

  it("本地事件（remoteOrigin 为 null）恒一致", () => {
    expect(isImportedEventIdConsistent(makeEvent({ globalSequence: "1", eventId: "aaaaaaaa-0000-4000-8000-000000000001" }))).toBe(true);
  });
});
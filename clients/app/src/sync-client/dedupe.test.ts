/**
 * R14：按稳定 `eventId` 去重、不跳过、无丢失区间。
 *
 * 判别力来源：
 * - 把去重键换成连接序号 → 「同一 eventId 不同 messageId」用例变红；
 * - 把去重键换成 `messageId` → 同上（WSS 信封 ID 每次重连都变）；
 * - 重新引入「序号必须连续 ⇒ 丢失」的判定 → 「scope 过滤造成的空洞」用例变红；
 * - 把「水位之下未见过」直接判重复 → 「真实丢失仍可检出」用例变红（静默丢失）。
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

  it("去重键不是序号：序号相同但 eventId 不同不是重复事件，按丢失证据补齐", () => {
    const ledger = new EventLedger({ serverEpoch: EPOCH });
    const first = makeEvent({ globalSequence: "1", eventId: "aaaaaaaa-0000-4000-8000-000000000001" });
    const second = makeEvent({ globalSequence: "1", eventId: "aaaaaaaa-0000-4000-8000-000000000002" });

    expect(evaluate(ledger, first)).toEqual({ kind: "new" });
    // 同一个序号、不同 eventId = 服务端复用了一个事件位。§9.3 的屏障声明过「这条之前都已交付」，
    // 因此这是**另一条**从未交付的事件，不是第一条的重投。
    // 反例：若按序号去重（判 duplicate 丢弃），这条事件就永久丢了 → 断言变红。
    expect(ledger.evaluate(second)).toEqual({ kind: "late", watermark: "1", received: "1" });
  });
});

describe("R14：scope 过滤造成的空洞不是丢失", () => {
  it("续接游标之后的序号跳跃直接接受，不判缺口也不要求补齐", () => {
    const ledger = new EventLedger({ serverEpoch: EPOCH, resumeFrom: { serverEpoch: EPOCH, globalSequence: "10" } });
    expect(ledger.cursor?.globalSequence).toBe("10");

    // §9.2：「全局 sequence 对单个设备可以有不可见的空洞，不能据此推断隐藏事件。」
    // 反例：若仍按 `last + 1` 判缺口，这里会返回 gap → 断言变红。
    expect(evaluate(ledger, makeEvent({ globalSequence: "12", eventId: "aaaaaaaa-0000-4000-8000-000000000012" }))).toEqual({
      kind: "new",
    });
    expect(ledger.cursor?.globalSequence).toBe("12");
    // 空洞的那条若后来补投，它落在水位之下 → 判 `late`（真实丢失证据），而不是被静默丢弃。
    expect(ledger.evaluate(makeEvent({ globalSequence: "11", eventId: "aaaaaaaa-0000-4000-8000-000000000011" }))).toEqual({
      kind: "late",
      watermark: "12",
      received: "11",
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

  it("首次同步：没有水位时任何序号都直接接受", () => {
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

  it("水位之下未呈现过的事件判 late；补齐后水位不动，再投判重复", () => {
    const ledger = new EventLedger({ serverEpoch: EPOCH, resumeFrom: { serverEpoch: EPOCH, globalSequence: "1" } });
    // 服务端声明过「1..10 都已交付」，随后补投其中一条 → 真实的丢失区间证据。
    expect(ledger.advanceTo({ serverEpoch: EPOCH, globalSequence: "10" })).toEqual({ kind: "advanced" });
    const missed = makeEvent({ globalSequence: "4", eventId: "aaaaaaaa-0000-4000-8000-000000000004" });

    // 反例：若判 duplicate 丢弃，这条事件永久不可恢复 → 断言变红。
    expect(ledger.evaluate(missed)).toEqual({ kind: "late", watermark: "10", received: "4" });
    // 补齐呈现后水位仍停在屏障：§9.5 的 ACK 是「最高已连续处理的可见事件 cursor」，
    // 未呈现过的序号不满足「已处理」。
    ledger.commit(missed);
    expect(ledger.cursor?.globalSequence).toBe("10");
    expect(ledger.seenCount).toBe(1);
    // 再次重投同一条 → 判重复，不二次呈现。
    expect(ledger.evaluate(missed)).toEqual({ kind: "duplicate" });
  });

  it("水位之下的乱序投递同样判 late：服务端不能靠倒序把事件塞过检测", () => {
    const ledger = new EventLedger({ serverEpoch: EPOCH });
    expect(evaluate(ledger, makeEvent({ globalSequence: "8", eventId: "aaaaaaaa-0000-4000-8000-000000000008" }))).toEqual({
      kind: "new",
    });
    // 屏障尚未到达时先收到更旧的 5：这不是 scope 空洞（空洞只能是服务端不投递），
    // 而是一次真实的乱序投递，客户端必须记住它而不是按连续性丢在门外。
    expect(ledger.evaluate(makeEvent({ globalSequence: "5", eventId: "aaaaaaaa-0000-4000-8000-000000000005" }))).toEqual({
      kind: "late",
      watermark: "8",
      received: "5",
    });
  });
});

describe("去重窗口有界", () => {
  it("超出保留窗口时按插入顺序淘汰最旧的键", () => {
    const ledger = new EventLedger({ serverEpoch: EPOCH, retention: 3 });
    for (let index = 1; index <= 5; index += 1) {
      expect(evaluate(ledger, makeEvent({ globalSequence: String(index), eventId: `aaaaaaaa-0000-4000-8000-${String(index).padStart(12, "0")}` }))).toEqual({ kind: "new" });
    }
    expect(ledger.seenCount).toBe(3);
    // 被淘汰的第一条重投时落在「已释放的记忆段」里：既无法回忆它是否呈现过，也就不把它当成
    // 新的丢失区间去补齐呈现——窗口只约束记忆，不约束正确性。
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

describe("屏障是权威水位（§9.3/§9.4/§9.5）", () => {
  it("采纳屏障后，屏障之上的序号跳跃仍直接接受（空洞不是丢失）", () => {
    const ledger = new EventLedger({ serverEpoch: EPOCH });
    // 屏障 100：序号 1..100 之中可能有过滤掉的不可见段（§9.2/§9.5 明确允许）。
    expect(ledger.advanceTo({ serverEpoch: EPOCH, globalSequence: "100" })).toEqual({ kind: "advanced" });
    expect(ledger.cursor).toEqual({ serverEpoch: EPOCH, globalSequence: "100" });

    // 屏障之下未投递的空洞已被服务端消解；屏障之上到达的可见事件同样不必连续。
    expect(evaluate(ledger, makeEvent({ globalSequence: "103", eventId: "aaaaaaaa-0000-4000-8000-000000000103" }))).toEqual({
      kind: "new",
    });
    expect(ledger.cursor?.globalSequence).toBe("103");
  });

  it("重复或更旧的屏障是 already_covered：水位不动，且与 advanced 区分开", () => {
    const ledger = new EventLedger({ serverEpoch: EPOCH });
    expect(ledger.advanceTo({ serverEpoch: EPOCH, globalSequence: "100" })).toEqual({ kind: "advanced" });
    // 连接层据此判断「恢复通道是否真的推进了水位」；把二者混为一谈会让退避上限不可达。
    expect(ledger.advanceTo({ serverEpoch: EPOCH, globalSequence: "100" })).toEqual({ kind: "already_covered" });
    expect(ledger.advanceTo({ serverEpoch: EPOCH, globalSequence: "40" })).toEqual({ kind: "already_covered" });
    expect(ledger.cursor).toEqual({ serverEpoch: EPOCH, globalSequence: "100" });
  });

  it("跨 epoch 的屏障被拒绝（事件库已重建，序号不可比）", () => {
    const ledger = new EventLedger({ serverEpoch: EPOCH, resumeFrom: { serverEpoch: EPOCH, globalSequence: "7" } });
    expect(ledger.advanceTo({ serverEpoch: "11111111-2222-4333-8444-555555555555", globalSequence: "99999" })).toEqual({
      kind: "rejected",
      reason: "epoch_mismatch",
    });
    // 反例：若强行采用不相干的游标，真实水位会被一段无关序号覆盖。
    expect(ledger.cursor).toEqual({ serverEpoch: EPOCH, globalSequence: "7" });
  });

  it("越界与形状非法的屏障被拒绝：故障服务端不得把水位推到无法自愈的位置", () => {
    const ledger = new EventLedger({ serverEpoch: EPOCH, resumeFrom: { serverEpoch: EPOCH, globalSequence: "7" } });
    // `decodeCaughtUp` 只做 Cursor 形状检查，不查上界，因此上界必须由账本自己守住。
    expect(ledger.advanceTo({ serverEpoch: EPOCH, globalSequence: "999999999999999999999" })).toEqual({
      kind: "rejected",
      reason: "sequence_out_of_range",
    });
    expect(ledger.advanceTo({ serverEpoch: EPOCH, globalSequence: "-5" })).toEqual({
      kind: "rejected",
      reason: "malformed_sequence",
    });
    // 反例：越界屏障被采纳后，此后每条真实事件都落在水位之下 → 事件流静默停摆且无法自愈。
    expect(ledger.cursor).toEqual({ serverEpoch: EPOCH, globalSequence: "7" });
    expect(ledger.evaluate(makeEvent({ globalSequence: "8", eventId: "aaaaaaaa-0000-4000-8000-000000000008" }))).toEqual({
      kind: "new",
    });
  });

  it("形状非法的续接游标在构造期就响亮地失败，不静默退化成无游标", () => {
    expect(
      () => new EventLedger({ serverEpoch: EPOCH, resumeFrom: { serverEpoch: EPOCH, globalSequence: "0x10" } }),
    ).toThrow(/malformed_sequence/);
  });
});
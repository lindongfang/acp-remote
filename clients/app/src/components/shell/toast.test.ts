/**
 * 全局提示的判定：显示时通知订阅方、驻留到时长后自行收起、连续提示只由最后一条负责收起。
 *
 * 用**自建仓库 + 假计时器**断言，而不是等真实 1.8 秒：驻留时长因此是可断言的输入，
 * 生产的那一份（`toastStore`）与这里被断言的是同一段实现。
 */

import { afterEach, describe, expect, it, vi } from "vitest";

import { createToastStore } from "./toast";

describe("全局提示", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("显示时通知订阅方，驻留到时长后自行收起并再通知一次", () => {
    vi.useFakeTimers();
    const store = createToastStore(1000);
    let notified = 0;
    const unsubscribe = store.subscribe(() => {
      notified += 1;
    });

    store.show("已复制");
    expect(store.get()).toBe("已复制");
    expect(notified).toBe(1);

    vi.advanceTimersByTime(999);
    expect(store.get()).toBe("已复制");
    vi.advanceTimersByTime(1);
    expect(store.get()).toBeNull();
    expect(notified).toBe(2);

    unsubscribe();
    store.show("又一条");
    expect(notified).toBe(2);
  });

  it("连续提示重新起算，且只有最后一条的计时负责收起", () => {
    vi.useFakeTimers();
    const store = createToastStore(1000);

    store.show("第一条");
    vi.advanceTimersByTime(600);
    store.show("第二条");
    // 第一条的计时到点了，但它已经不是最新的一条：不得把第二条收掉。
    vi.advanceTimersByTime(400);
    expect(store.get()).toBe("第二条");
    vi.advanceTimersByTime(600);
    expect(store.get()).toBeNull();
  });
});

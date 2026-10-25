import type * as ReactModule from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

const harness = vi.hoisted(() => ({
  pathname: "/pair",
  phase: "awaiting_code",
  hasHost: true,
  connectionState: "online",
  replace: vi.fn(),
  effects: [] as Array<() => void | (() => void)>,
}));

vi.mock("react", async (original) => ({
  ...await original<typeof ReactModule>(),
  useEffect: (effect: () => void | (() => void)) => { harness.effects.push(effect); },
}));

import { usePairingNavigation } from "./pairing-navigation";

function runEffects(): Array<void | (() => void)> {
  usePairingNavigation({
    kind: "sync",
    isCurrentRoute: harness.pathname === "/pair",
    state: harness.hasHost ? { host: {}, connection: { state: harness.connectionState } } : null,
    approvedSync: harness.phase === "approved",
    router: { replace: harness.replace },
  });
  return harness.effects.map((effect) => effect());
}

afterEach(() => {
  harness.effects.length = 0;
  harness.pathname = "/pair";
  harness.phase = "awaiting_code";
  harness.hasHost = true;
  harness.connectionState = "online";
  vi.clearAllMocks();
  vi.useRealTimers();
});

describe("后台配对页的导航", () => {
  it("当前配对页仍将已有身份的设备带回目录", () => {
    runEffects();
    expect(harness.replace).toHaveBeenCalledExactlyOnceWith("/");
  });

  it.each(["awaiting_code", "approved"])("离开配对页后，%s 不得重新导航", (phase) => {
    vi.useFakeTimers();
    harness.pathname = "/dir/example";
    harness.phase = phase;
    runEffects();
    vi.runAllTimers();
    expect(harness.replace).not.toHaveBeenCalled();
  });

  it.each(["identity_changed", "revoked"])("身份变化或撤销（%s）后不据旧主机记录跳回目录", (state) => {
    harness.connectionState = state;
    runEffects();
    expect(harness.replace).not.toHaveBeenCalled();
  });

  it("配对成功后延迟进入目录，离开时取消尚未执行的跳转", () => {
    vi.useFakeTimers();
    harness.phase = "approved";
    const cleanups = runEffects();
    expect(harness.replace).not.toHaveBeenCalled();
    for (const cleanup of cleanups) cleanup?.();
    vi.runAllTimers();
    expect(harness.replace).not.toHaveBeenCalled();
  });

  it("停留在配对页时，成功过渡结束后只跳转一次", () => {
    vi.useFakeTimers();
    harness.phase = "approved";
    runEffects();
    vi.advanceTimersByTime(1599);
    expect(harness.replace).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(harness.replace).toHaveBeenCalledExactlyOnceWith("/");
  });
});

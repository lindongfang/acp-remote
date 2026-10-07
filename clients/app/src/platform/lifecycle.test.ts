/**
 * 生命周期端口的 Web 侧行为。
 *
 * Node 测试环境没有 `document`/`window`，因此这里既核对「无 DOM 时不崩溃」，
 * 也用一个最小 DOM 替身核对订阅/退订与可见性映射。
 */

import { afterEach, describe, expect, it } from "vitest";

import type { LifecycleEvent } from "./lifecycle";
import { openLifecycle } from "./lifecycle.web";

interface ListenerBook {
  visibilitychange: (() => void)[];
  pagehide: (() => void)[];
}

/** DOM 替身：类型化句柄，测试里直接读 `visibilityState` 而不做内联断言。 */
interface DomDouble {
  readonly listeners: ListenerBook;
  readonly document: { visibilityState: string };
  setVisibility(state: "visible" | "hidden"): void;
}

const ORIGINAL_DOCUMENT = Object.getOwnPropertyDescriptor(globalThis, "document");
const ORIGINAL_WINDOW = Object.getOwnPropertyDescriptor(globalThis, "window");

/** 装一个最小 `document`/`window` 替身。 */
function installDomDouble(initialVisibility: "visible" | "hidden"): DomDouble {
  const listeners: ListenerBook = { visibilitychange: [], pagehide: [] };
  const state = { visibilityState: initialVisibility as string };
  const addEventListener = (type: keyof ListenerBook, listener: () => void) => {
    listeners[type]?.push(listener);
  };
  const removeEventListener = (type: keyof ListenerBook, listener: () => void) => {
    const list = listeners[type];
    if (list === undefined) return;
    const index = list.indexOf(listener);
    if (index >= 0) list.splice(index, 1);
  };
  const documentDouble = {
    get visibilityState(): string {
      return state.visibilityState;
    },
    addEventListener,
    removeEventListener,
  };
  Object.defineProperty(globalThis, "document", { value: documentDouble, configurable: true });
  Object.defineProperty(globalThis, "window", {
    value: { addEventListener, removeEventListener },
    configurable: true,
  });
  return {
    listeners,
    document: documentDouble,
    setVisibility(next) {
      state.visibilityState = next;
    },
  };
}

afterEach(() => {
  if (ORIGINAL_DOCUMENT === undefined) Reflect.deleteProperty(globalThis, "document");
  else Object.defineProperty(globalThis, "document", ORIGINAL_DOCUMENT);
  if (ORIGINAL_WINDOW === undefined) Reflect.deleteProperty(globalThis, "window");
  else Object.defineProperty(globalThis, "window", ORIGINAL_WINDOW);
});

describe("生命周期端口", () => {
  it("无 DOM 时不崩溃：按 visible 返回且订阅为空操作", () => {
    Reflect.deleteProperty(globalThis, "document");
    Reflect.deleteProperty(globalThis, "window");
    const lifecycle = openLifecycle();
    expect(lifecycle.currentVisibility()).toBe("visible");
    expect(() => lifecycle.subscribe(() => {})()).not.toThrow();
  });

  it("可见性变化映射为 visibility 事件，退订后不再收到", () => {
    const dom = installDomDouble("visible");
    const lifecycle = openLifecycle();
    const received: LifecycleEvent[] = [];
    const unsubscribe = lifecycle.subscribe((event) => received.push(event));

    dom.setVisibility("hidden");
    dom.listeners.visibilitychange.forEach((listener) => listener());
    expect(received).toEqual([{ kind: "visibility", state: "hidden" }]);
    expect(lifecycle.currentVisibility()).toBe("hidden");

    unsubscribe();
    dom.setVisibility("visible");
    dom.listeners.visibilitychange.forEach((listener) => listener());
    expect(received).toHaveLength(1);
    expect(dom.listeners.visibilitychange).toHaveLength(0);
    expect(dom.listeners.pagehide).toHaveLength(0);
  });

  it("pagehide 产生 pagehide 事件；不注册 beforeunload（bfcache 失格诱因）", () => {
    const dom = installDomDouble("visible");
    const lifecycle = openLifecycle();
    const received: LifecycleEvent[] = [];
    const unsubscribe = lifecycle.subscribe((event) => received.push(event));

    dom.listeners.pagehide.forEach((listener) => listener());
    expect(received).toEqual([{ kind: "pagehide" }]);

    // bfcache：注册 `beforeunload` 监听器会让页面失去资格，因此实现不得注册它。
    const windowDouble = globalThis.window as unknown as { addEventListener: (type: string) => void };
    const registered: string[] = [];
    const originalAdd = windowDouble.addEventListener;
    windowDouble.addEventListener = (type: string) => registered.push(type);
    const second = openLifecycle();
    const off = second.subscribe(() => {});
    windowDouble.addEventListener = originalAdd;
    expect(registered).not.toContain("beforeunload");
    expect(registered).toContain("pagehide");
    off();
    unsubscribe();
  });

  it("持久化请求在缺少 navigator.storage 时返回 false（不抛错）", async () => {
    const lifecycle = openLifecycle();
    await expect(lifecycle.requestPersistentStorage()).resolves.toBe(false);
  });
});

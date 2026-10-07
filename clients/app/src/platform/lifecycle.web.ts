/**
 * 生命周期端口的 **Web/PWA** 实现。
 *
 * 归属 WP5b。把 `document.visibilitychange` 与 `window.pagehide` 收敛成
 * {@link LifecyclePort}，让 WP6/WP7 不直接依赖 DOM。
 *
 * v1 只交付 Web：**不写** `.native.ts`（`tasks.md` 2.6）。
 */

import type { LifecycleEvent, LifecyclePort, Unsubscribe, VisibilityState } from "./lifecycle";

/** 打开生命周期端口。 */
export function openLifecycle(): LifecyclePort {
  return new WebLifecycle();
}

class WebLifecycle implements LifecyclePort {
  subscribe(listener: (event: LifecycleEvent) => void): Unsubscribe {
    if (typeof document === "undefined") {
      // 非浏览器（Node 测试/SSR）：没有平台事件可订阅，返回一个空取消函数。
      return () => {};
    }
    const onVisibilityChange = () => {
      listener({
        kind: "visibility",
        state: document.visibilityState === "hidden" ? "hidden" : "visible",
      });
    };
    const onPageHide = () => listener({ kind: "pagehide" });

    document.addEventListener("visibilitychange", onVisibilityChange);
    // 只用 `pagehide`：它在正常卸载与 bfcache 两侧都可靠。**不**挂 `beforeunload`——
    // 注册该监听器本身就会让页面在桌面 Chrome/Firefox 失去 bfcache 资格
    // （https://web.dev/articles/bfcache），把前进/后退退化成整页重载并重跑握手，
    // 正好抵消选择 `pagehide` 的初衷。
    window.addEventListener("pagehide", onPageHide);

    return () => {
      document.removeEventListener("visibilitychange", onVisibilityChange);
      window.removeEventListener("pagehide", onPageHide);
    };
  }

  currentVisibility(): VisibilityState {
    if (typeof document === "undefined") return "visible";
    return document.visibilityState === "hidden" ? "hidden" : "visible";
  }

  async requestPersistentStorage(): Promise<boolean> {
    if (typeof navigator === "undefined" || navigator.storage === undefined) return false;
    try {
      return await navigator.storage.persist();
    } catch {
      // 用户拒绝或浏览器不支持：不是错误，按「未获批」处理。
      return false;
    }
  }
}

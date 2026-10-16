/**
 * 「减少动效」的当前取值（`prefers-reduced-motion: reduce`）。
 *
 * ## 为什么走 JS 而不是 CSS 媒体查询
 *
 * RNW 0.21 的 `StyleSheet.create` **不支持**把 `@media` 当样式键（它只对
 * `placeholderTextColor` / `pointerEvents` / `scrollbarWidth` 做特例编译，其余键按属性名直通），
 * 而本仓库的 CSP 基线也不允许新增 CSS 文件、内联 `<style>` 或 `style=` 属性。因此「用户要求
 * 减少动效」只能由这里读出来，消费方据此换用**另一份静态样式类**（不带时长），
 * 仍然不写内联样式。
 *
 * 无 `matchMedia` 的运行时（node / 组件测试）按「不减少」处理：那里没有动效可言。
 */

import { useCallback, useSyncExternalStore } from "react";

import { REDUCED_MOTION_QUERY } from "../theme";

function reducedMotionQuery(): MediaQueryList | null {
  if (typeof globalThis.matchMedia !== "function") return null;
  try {
    return globalThis.matchMedia(REDUCED_MOTION_QUERY);
  } catch {
    // 取值非法或浏览器拒绝该查询：按「不减少」处理，不因此让页面渲染失败。
    return null;
  }
}

/** 用户是否要求减少动效。 */
export function useReducedMotion(): boolean {
  const subscribe = useCallback((listener: () => void) => {
    const media = reducedMotionQuery();
    if (media === null) return () => {};
    media.addEventListener("change", listener);
    return () => {
      media.removeEventListener("change", listener);
    };
  }, []);
  const snapshot = useCallback(() => reducedMotionQuery()?.matches ?? false, []);
  return useSyncExternalStore(subscribe, snapshot, () => false);
}

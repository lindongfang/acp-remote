import { createElement, isValidElement } from "react";
import { describe, expect, it, vi } from "vitest";

vi.mock("expo-router", () => ({ useRouter: () => ({ push: vi.fn() }) }));

import { renderAuthenticatedScreen } from "./AuthenticatedScreen";

describe("screenLayout 回调", () => {
  it("能在 React 组件外按路由逐个直接调用，不在导航器的调用栈中执行 Hook", () => {
    // 真实 useDescriptors 在一次 render 中为每条路由调用 layout，路由数量会随导航变化。
    // 回调若直接执行含 Hook 的组件，这里会抛 invalid hook call，或使导航器的 Hook 数量变化。
    for (const name of ["index", "pair", "index", "dir/[directoryId]"]) {
      const children = createElement("div", null, name);
      const element = renderAuthenticatedScreen({ children, route: { name } });
      expect(isValidElement(element)).toBe(true);
      expect(element.props).toMatchObject({ children, route: { name } });
    }
  });
});

/**
 * 配对页主题开关的静态呈现。
 *
 * 判别力：按钮必须复用共享偏好（`data-theme-preference` 与 `useThemePreference()` 一致），
 * 并在浅/深两态给出相反的动作标签与图形；把主题状态复制到页面私有字段、或把标签写反，
 * 都会让这两条断言失败。点击行为与持久化由浏览器 e2e（PW-11）覆盖。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it } from "vitest";

import { PairingThemeToggle } from "./PairingThemeToggle";
import { themePreferenceStore } from "./theme";

afterEach(() => {
  themePreferenceStore.set("system");
});

describe("配对页主题开关", () => {
  it("浅色：月亮图形、白天动作标签、共享偏好锚点", () => {
    themePreferenceStore.set("light");
    const html = renderToStaticMarkup(createElement(PairingThemeToggle));
    expect(html).toContain('data-action="toggle-theme"');
    expect(html).toContain('data-pairing-theme-toggle="true"');
    expect(html).toContain('data-theme-preference="light"');
    expect(html).toContain('aria-label="当前：白天，点击切换"');
  });

  it("深色：太阳图形、黑夜动作标签、共享偏好锚点", () => {
    themePreferenceStore.set("dark");
    const html = renderToStaticMarkup(createElement(PairingThemeToggle));
    expect(html).toContain('data-theme-preference="dark"');
    expect(html).toContain('aria-label="当前：黑夜，点击切换"');
  });
});

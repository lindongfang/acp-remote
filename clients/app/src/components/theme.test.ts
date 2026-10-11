/**
 * 样式基座的判定（`design.md` D1）：
 *
 * - 浅色与深色**两套**调色板（`prototypes/acp-remote-pwa.html:22-41` 的 `:root` 与
 *   `:root[data-theme="dark"]`）键集合必须完全相同——少一个键，深色下就会出现「没换色」的
 *   元素，而它在浅色下看不出来；
 * - 每个取值必须是非空字符串（空值在 RNW 里等于没写样式，同样是静默失效）；
 * - 键集合必须**恰好**是 `ColorToken` 的 22 个键。
 *
 * 22 个键写成显式字符串数组而不是从 `ColorToken` 派生：派生写法会跟着类型一起漂移，
 * 删掉一个 token 时两侧同时变，断言不会红。
 */

import { describe, expect, it } from "vitest";

import type { ColorToken } from "./theme";
import { palettes } from "./theme";

/** `ColorToken` 的 22 个键（原型 CSS 变量名的驼峰化：`--panel-2` → `panel2`）。 */
const COLOR_TOKENS: readonly ColorToken[] = [
  "bg",
  "panel",
  "panel2",
  "line",
  "line2",
  "ink",
  "ink2",
  "ink3",
  "accent",
  "accentInk",
  "accentSoft",
  "ok",
  "okSoft",
  "warn",
  "warnSoft",
  "danger",
  "dangerSoft",
  "think",
  "thinkSoft",
  "tool",
  "toolSoft",
  "shadow",
];

describe("主题调色板", () => {
  it("清单本身有 22 个不重复的键", () => {
    expect(COLOR_TOKENS).toHaveLength(22);
    expect(new Set(COLOR_TOKENS).size).toBe(COLOR_TOKENS.length);
  });

  it("浅色与深色的键集合完全相同，且恰好等于 ColorToken 的 22 个键", () => {
    const expected = [...COLOR_TOKENS].sort();
    expect(Object.keys(palettes.light).sort()).toEqual(expected);
    expect(Object.keys(palettes.dark).sort()).toEqual(expected);
  });

  it("每个取值都是非空字符串", () => {
    for (const [scheme, palette] of [
      ["light", palettes.light],
      ["dark", palettes.dark],
    ] as const) {
      for (const token of COLOR_TOKENS) {
        const value = palette[token];
        expect(typeof value, `${scheme}.${token} 必须是字符串`).toBe("string");
        expect(value.length, `${scheme}.${token} 不得为空`).toBeGreaterThan(0);
      }
    }
  });

  it("深色不是浅色的副本：两套取值必须有区别", () => {
    // 反例：把浅色整套复制到深色时，上面三条断言仍然全绿——这条专门挡这个错。
    expect(palettes.dark).not.toEqual(palettes.light);
  });
});

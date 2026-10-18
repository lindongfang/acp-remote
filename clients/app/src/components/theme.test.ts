/**
 * 样式基座的判定（`design.md` D1/D2/D9）：
 *
 * - 浅色与深色**两套**调色板（`prototypes/acp-remote-pwa.html:22-41` 的 `:root` 与
 *   `:root[data-theme="dark"]`）键集合必须完全相同——少一个键，深色下就会出现「没换色」的
 *   元素，而它在浅色下看不出来；
 * - 每个取值必须是非空字符串（空值在 RNW 里等于没写样式，同样是静默失效）；
 * - **键集合只有一处定义**：`theme.ts` 的 `COLOR_TOKENS` 是唯一来源，`ColorToken` 与两套
 *   `palettes` 都由它派生，本文件断言三者逐字一致。颜色键仍**恰好 22 个**：本次新增的是
 *   **尺度**（断点/字重/层级/动效，`D9`），它们各有自己的键清单，不得混进颜色键；
 * - 主题偏好是三态且可持久化：显式取值换一个仓库实例仍读得回来（≡ 跨刷新保留），
 *   缺失与非法取值退回跟随系统，跟随系统由 `resolveColorScheme` 按系统取值解析。
 */

import { createElement } from "react";
import type { ReactElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { ColorToken, ThemePreferenceStorage } from "./theme";
import {
  COLOR_TOKENS,
  THEME_PREFERENCES,
  THEME_PREFERENCE_STORAGE_KEY,
  breakpoints,
  createThemePreferenceStore,
  fontWeight,
  motion,
  palettes,
  parseThemePreference,
  resolveColorScheme,
  themePreferenceStore,
  useTheme,
  zIndex,
} from "./theme";

/** `ColorToken` 的 22 个键（原型 CSS 变量名的驼峰化：`--panel-2` → `panel2`）。 */
const EXPECTED_COLOR_TOKENS: readonly string[] = [
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

/**
 * 编译期断言：清单里的每个键都是 `ColorToken`（`ColorToken` 反过来由该清单派生）。
 *
 * 两个方向都不写在测试里也不行：只有「清单 → 类型」由 `theme.ts` 保证，因此这里补上
 * 「类型 → 清单」这一半，测试里的逐键取值就不需要断言。
 */
const COLOR_KEYS: readonly ColorToken[] = COLOR_TOKENS;

describe("主题调色板", () => {
  it("颜色键恰好 22 个、不重复，且与 ColorToken 的键逐字一致", () => {
    expect(COLOR_KEYS).toHaveLength(22);
    expect(new Set(COLOR_KEYS).size).toBe(COLOR_KEYS.length);
    expect([...COLOR_KEYS].sort()).toEqual([...EXPECTED_COLOR_TOKENS].sort());
  });

  it("浅色与深色的键集合完全相同，且等于唯一的颜色键清单", () => {
    const expected = [...COLOR_KEYS].sort();
    expect(Object.keys(palettes.light).sort()).toEqual(expected);
    expect(Object.keys(palettes.dark).sort()).toEqual(expected);
  });

  it("每个取值都是非空字符串", () => {
    for (const [scheme, palette] of [
      ["light", palettes.light],
      ["dark", palettes.dark],
    ] as const) {
      for (const token of COLOR_KEYS) {
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

describe("尺度 token（断点 / 字重 / 层级 / 动效）", () => {
  it("每类尺度各有自己的键清单，且键集合与取值都是确定的", () => {
    expect(Object.keys(breakpoints)).toEqual(["wide"]);
    expect(Object.keys(fontWeight).sort()).toEqual(["bold", "medium", "regular", "value"]);
    expect(Object.keys(zIndex).sort()).toEqual(["directoryPicker", "drawer", "modal", "scrim", "toast", "topBar"]);
    expect(Object.keys(motion).sort()).toEqual(["base", "drawer", "fast", "pulse"]);

    // 取值本身：断点为正数、层级按原型递增、动效时长为正、字重是原型用到的那三档。
    expect(breakpoints.wide).toBeGreaterThan(0);
    expect(zIndex.topBar).toBeLessThan(zIndex.scrim);
    expect(zIndex.scrim).toBeLessThan(zIndex.drawer);
    expect(zIndex.drawer).toBeLessThan(zIndex.toast);
    for (const duration of [motion.fast, motion.base, motion.drawer, motion.pulse]) {
      expect(duration).toBeGreaterThan(0);
    }
    // 四档取值逐项对齐原型用到的 400/500/600/700：这个精确数组断言本身就能拦住「两档同值」
    // （同一档位被定义两次），键名漂移则由上面的键集合断言拦住。
    expect([fontWeight.regular, fontWeight.value, fontWeight.medium, fontWeight.bold]).toEqual([
      400, 500, 600, 700,
    ]);
  });

  it("尺度键与颜色键不重名：新增的是尺度，不是颜色键", () => {
    // 反例：把 `fontWeight` 之类的尺度塞进 `palettes`（或反过来把颜色键写进尺度），
    // 这条会红；「颜色键恰好 22 个」那条也会一起红。
    const colorKeys = new Set<string>(COLOR_KEYS);
    for (const key of [
      ...Object.keys(breakpoints),
      ...Object.keys(fontWeight),
      ...Object.keys(zIndex),
      ...Object.keys(motion),
    ]) {
      expect(colorKeys.has(key), `${key} 与颜色键重名`).toBe(false);
    }
  });
});

/** 一份内存存储：两个仓库实例共用它 ≡ 同一个浏览器存储被两次页面加载复用。 */
function memoryStorage(): ThemePreferenceStorage {
  const values = new Map<string, string>();
  return {
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => {
      values.set(key, value);
    },
  };
}

describe("主题偏好（三态 + 持久化）", () => {
  it("取值的封闭清单是 跟随系统 / 白天 / 黑夜，且落在自己的 UI 偏好键上", () => {
    expect(THEME_PREFERENCES).toEqual(["system", "light", "dark"]);
    // 这个键就是「客户端 UI 偏好」的槽位（`docs/FRONTEND_DESIGN.md` §7）。它与设备身份、
    // 会话正文与缓存摘要分槽：不是本地缓存的键前缀，也不是任何身份键。
    expect(THEME_PREFERENCE_STORAGE_KEY).toBe("acpr-theme-preference");
    expect(THEME_PREFERENCE_STORAGE_KEY.startsWith("paired-host/")).toBe(false);
  });

  it("显式取值跨刷新保留：换一个仓库实例读同一份存储仍是该取值", () => {
    const storage = memoryStorage();
    createThemePreferenceStore(storage).set("dark");
    expect(createThemePreferenceStore(storage).get()).toBe("dark");

    createThemePreferenceStore(storage).set("light");
    expect(createThemePreferenceStore(storage).get()).toBe("light");
  });

  it("缺省与旧用户（没有这个键）跟随系统；非法取值同样退回跟随系统", () => {
    expect(createThemePreferenceStore(memoryStorage()).get()).toBe("system");
    for (const raw of [null, undefined, "", "morning", "SYSTEM", "Dark"]) {
      expect(parseThemePreference(raw)).toBe("system");
    }
    for (const raw of ["system", "light", "dark"] as const) {
      expect(parseThemePreference(raw)).toBe(raw);
    }
  });

  it("跟随系统随系统变化：同一偏好 + 两个系统取值 → 两个结果；显式取值不受系统影响", () => {
    expect(resolveColorScheme("system", true)).toBe("dark");
    expect(resolveColorScheme("system", false)).toBe("light");
    expect(resolveColorScheme("dark", false)).toBe("dark");
    expect(resolveColorScheme("light", true)).toBe("light");
  });

  it("写入通知订阅方，取消订阅后不再收到", () => {
    const store = createThemePreferenceStore(memoryStorage());
    let notified = 0;
    const unsubscribe = store.subscribe(() => {
      notified += 1;
    });
    store.set("dark");
    expect(notified).toBe(1);
    expect(store.get()).toBe("dark");
    // 写入同一取值不重复通知（订阅方不必为一模一样的状态重画）。
    store.set("dark");
    expect(notified).toBe(1);
    unsubscribe();
    store.set("light");
    expect(notified).toBe(1);
  });

  it("存储不可用（读取抛错）时读回跟随系统，页面照常渲染", () => {
    const hostile: ThemePreferenceStorage = {
      getItem: () => {
        throw new Error("隐私模式");
      },
      setItem: () => {
        throw new Error("配额不足");
      },
    };
    const store = createThemePreferenceStore(hostile);
    expect(store.get()).toBe("system");
    expect(() => {
      store.set("dark");
    }).not.toThrow();
    expect(store.get()).toBe("dark");
  });
});

/** 探针：把 `useTheme()` 的返回值渲染成文本，用来断言偏好真的接到了主题上。 */
function ThemeProbe(): ReactElement {
  const theme = useTheme();
  return createElement("span", null, theme.dark ? "dark" : "light");
}

describe("useTheme() 接到主题偏好上", () => {
  it("显式取值接到主题上：黑夜渲染深色、白天渲染浅色（与系统取值无关）", () => {
    const previous = themePreferenceStore.get();
    try {
      themePreferenceStore.set("dark");
      expect(renderToStaticMarkup(createElement(ThemeProbe))).toContain("dark");
      themePreferenceStore.set("light");
      expect(renderToStaticMarkup(createElement(ThemeProbe))).toContain("light");
    } finally {
      // 全局仓库要在用例结束时恢复原状，否则同文件后面的用例会看到脏值。
      themePreferenceStore.set(previous);
    }
  });
});

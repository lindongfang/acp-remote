/**
 * 样式基座（`design.md` D1）：**唯一一套**设计 token 与随系统深浅色的 `useTheme()`。
 *
 * ## 取值从哪来
 *
 * `prototypes/acp-remote-pwa.html:22-41` 定义了整套 CSS 变量（浅色取 `:root`，深色取
 * `:root[data-theme="dark"]`），`docs/FRONTEND_DESIGN.md` §9 第 1 条把该原型定为目录页与
 * 目录详情页的交互口径来源。两套调色板的键集合**完全相同**（`theme.test.ts` 逐键断言），
 * CSS 变量名按「去连字符 + 驼峰」映射到 token：`--panel-2` → `panel2`、`--ink-2` → `ink2`、
 * `--accent-soft` → `accentSoft`。
 *
 * ## 为什么运行时绑定放在本文件
 *
 * `clients/app` 是 Expo 工程，而 `react-native` 这个入口本身是含 Flow 语法的源码：Node
 * （`vitest`）与 e2e 的 esbuild 都解析不了它（实测 `Unexpected token 'typeof'`），实际渲染器
 * 是 `react-native-web`——它**没有随包发布类型声明**（包内没有任何 `.d.ts`，`tsc` 报 TS7016）。
 * 项目既有的同类取舍是加一个最小 `.d.ts`（见 `src/components/testing/react-dom-server.d.ts`），
 * 但新增文件不在本工作包的写范围里，因此把「**运行时**取 `react-native-web`、**类型**取
 * `react-native`」这一处收敛在这里，供三个原语与后续迁移的组件共享，避免每个文件各写一份抑制。
 *
 * ## 主题偏好（`design.md` D2）
 *
 * `useTheme()` 不再只跟随系统：它按**主题偏好**（`system | light | dark`）解析出实际深浅色，
 * 偏好落在 `docs/FRONTEND_DESIGN.md` §7 允许持久化的「客户端 UI 偏好」槽位
 * （一个独立的存储键，与设备身份、会话正文与缓存摘要分槽），缺省与旧用户 = 跟随系统。
 *
 * ## 尺度 token 与网页专属属性
 *
 * 除颜色与既有四类尺度外，本文件还是**断点 / 字重 / 层级 / 动效**四类 token 的唯一位置
 * （`design.md` D9），以及网页专属 CSS 属性的唯一断言点（{@link webStyle}）——两者都不另立第二份。
 */

import { useCallback, useSyncExternalStore } from "react";
import type * as ReactNative from "react-native";

// @ts-expect-error `react-native-web` 不发布类型声明；把它的运行时值绑到 `react-native` 的公开类型上。
import * as ReactNativeWeb from "react-native-web";

/**
 * `dataSet` 是 react-native-web 的转发表成员（`data-directory-id` 这类 `data-*` 属性由它
 * 生成），但没有进入 `react-native` 的公开类型——`expo/types/react-native-web.d.ts` 的 web 补丁
 * 也没加它。本变更把 `data-*` 当作 e2e 的唯一机器证据，因此这里补上宿主元素的这一项：
 * `Pressable` 的 props 继承 `ViewProps`，而 `Text` 的 props 在类型里是独立的一支，需要各自声明。
 */
declare module "react-native" {
  interface ViewProps {
    dataSet?: Record<string, string | number> | undefined;
  }

  interface TextProps {
    dataSet?: Record<string, string | number> | undefined;
  }
}

/**
 * 渲染原语：运行时是 `react-native-web` 的实现，类型是 `react-native` 的公开声明。
 *
 * 页面与组件只从本模块取原语，保证「同一个原语在同一个地方定义」——不写 CSS 文件、
 * 也不引入第二套样式机制（D1）。`useWindowDimensions` 也在其中：D7 的自适应布局要在
 * 列表/详情组件里按断点切换列数，而它们同样绕不开上面那条「RN 入口不可被 Node/esbuild 解析」
 * 的约束。
 */
export const View: typeof ReactNative.View = ReactNativeWeb.View;
export const Text: typeof ReactNative.Text = ReactNativeWeb.Text;
export const Pressable: typeof ReactNative.Pressable = ReactNativeWeb.Pressable;
export const TextInput: typeof ReactNative.TextInput = ReactNativeWeb.TextInput;
export const StyleSheet: typeof ReactNative.StyleSheet = ReactNativeWeb.StyleSheet;
export const useWindowDimensions: typeof ReactNative.useWindowDimensions =
  ReactNativeWeb.useWindowDimensions;

/**
 * `TextInput` 的 props 类型。
 *
 * `TextInputProps` 在 `react-native` 的类型里继承 `ViewProps`，因此上面的 `dataSet` 增补
 * 对它同样生效；这里再导出一个别名，让搜索框组件不必为了一个类型去 import `react-native`
 * （那会把「原语只从本模块取」这条约定捅出一个口子）。
 */
export type TextInputProps = ReactNative.TextInputProps;

/** 系统深浅色只由 `useTheme()` 消费，因此不再单独对外暴露。 */
const useColorScheme: typeof ReactNative.useColorScheme = ReactNativeWeb.useColorScheme;

/**
 * 调色板的键：与原型 `:root` 的 CSS 变量一一对应（`--panel-2` → `panel2`）。
 *
 * 这是颜色键集合的**唯一**定义位置：`ColorToken` 由它派生，两套 `palettes` 由 `ColorToken`
 * 约束（少一个键就编译不过），`theme.test.ts` 进一步断言三者逐字一致且恰好 22 个。
 * 尺度（断点/字重/层级/动效）另有各自主键清单，**不**混进颜色键。
 */
export const COLOR_TOKENS = [
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
] as const;

/** 调色板的键。 */
export type ColorToken = (typeof COLOR_TOKENS)[number];

/** 一套颜色 token 的全部取值。 */
export type Palette = Readonly<Record<ColorToken, string>>;

/**
 * 两套调色板，逐项取自 `prototypes/acp-remote-pwa.html:22-41`。
 *
 * `shadow` 是 CSS `box-shadow` 的整串取值（RNW 的 `boxShadow` 样式接受字符串），
 * 它同样分浅/深两套，所以留在调色板里而不是另立一个维度。
 */
export const palettes: { readonly light: Palette; readonly dark: Palette } = {
  light: {
    bg: "#f6f7f9",
    panel: "#ffffff",
    panel2: "#fbfbfc",
    line: "#e4e6eb",
    line2: "#eef0f3",
    ink: "#12151a",
    ink2: "#4a515c",
    ink3: "#7c8593",
    accent: "#3b5bdb",
    accentInk: "#2c47b8",
    accentSoft: "#eef2ff",
    ok: "#128a4e",
    okSoft: "#e8f7ef",
    warn: "#a45b00",
    warnSoft: "#fff5e6",
    danger: "#c62828",
    dangerSoft: "#fdecec",
    think: "#6b4eff",
    thinkSoft: "#f4f1ff",
    tool: "#0d7d8c",
    toolSoft: "#eaf7f8",
    shadow: "0 1px 2px rgba(18,21,26,.05), 0 8px 24px rgba(18,21,26,.06)",
  },
  dark: {
    bg: "#0e1116",
    panel: "#151a21",
    panel2: "#11151b",
    line: "#252c36",
    line2: "#1e242c",
    ink: "#e8ecf2",
    ink2: "#a8b1bd",
    ink3: "#79838f",
    accent: "#8098f9",
    accentInk: "#a5b4fc",
    accentSoft: "#1b2237",
    ok: "#4ade80",
    okSoft: "#12261a",
    warn: "#fbbf24",
    warnSoft: "#2a1f0b",
    danger: "#f87171",
    dangerSoft: "#2b1414",
    think: "#a78bfa",
    thinkSoft: "#1e1b33",
    tool: "#4dd0c4",
    toolSoft: "#0f2426",
    shadow: "0 1px 2px rgba(0,0,0,.4),0 8px 24px rgba(0,0,0,.35)",
  },
};

/** 间距：4 的倍数（原型里 4/8/12/16/24 都有在用）。 */
export const spacing: Readonly<Record<"xs" | "sm" | "md" | "lg" | "xl", number>> = {
  xs: 4,
  sm: 8,
  md: 12,
  lg: 16,
  xl: 24,
};

/** 圆角：原型的圆角集中在 8px（输入框/行内元素）、12px（卡片/面板）、999px（胶囊）。 */
export const radius: Readonly<Record<"sm" | "md" | "lg", number>> = {
  sm: 8,
  md: 12,
  lg: 999,
};

/** 字号：原型正文是 14px，其余取值聚在 11/13/17/22，对应到五档。 */
export const fontSize: Readonly<Record<"xs" | "sm" | "md" | "lg" | "xl", number>> = {
  xs: 11,
  sm: 13,
  md: 14,
  lg: 17,
  xl: 22,
};

/** 字族：直接取原型的 `--sans` / `--mono` 两串（RNW 把它当 CSS `font-family` 用）。 */
export const fontFamily: { readonly sans: string; readonly mono: string } = {
  sans: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "PingFang SC", "Microsoft YaHei", sans-serif',
  mono: 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace',
};

/**
 * 断点（像素）：可用宽度不小于 `wide` 时用宽屏形态。
 *
 * 目录页、目录详情页与对话页共用**同一个**阈值（`design.md` D7/D10）；此前这三处各写了一次
 * 字面量 `720`，本 token 是它的唯一来源。
 */
export const breakpoints: Readonly<Record<"wide", number>> = {
  wide: 720,
};

/**
 * 字重档：四档，逐项取自原型用到的 `font-weight` ——
 *
 * - `regular` 400：正文（原型缺省字重）；
 * - `value` 500：等宽**值**（原型 `.drow .dpath`、`.dirctx .pt`、`.dv.mono` 三处，
 *   见 `prototypes/acp-remote-pwa.html:169/217/530`）；
 * - `medium` 600：标签与标题（原型里最常用的一档）；
 * - `bold` 700：强调（原型 `.fst` 与 markdown 的 `<strong>`）。
 *
 * `value` 这个名字取自它唯一的使用场景（等宽值行）：CSS 里 500 的常规词 `medium` 已被既有的
 * 600 档占用，而改名会打断 `ShellView` 等已合入的消费方，故既有档位保持不动。
 */
export const fontWeight: Readonly<
  Record<"regular" | "value" | "medium" | "bold", NonNullable<ReactNative.TextStyle["fontWeight"]>>
> = {
  regular: 400,
  value: 500,
  medium: 600,
  bold: 700,
};

/** 层级：原型 `header.top` 30 / `.scrim` 35 / `.drawer` 40 / `.toast` 60。 */
export const zIndex: Readonly<Record<"topBar" | "scrim" | "drawer" | "toast", number>> = {
  topBar: 30,
  scrim: 35,
  drawer: 40,
  toast: 60,
};

/**
 * 动效时长（毫秒）：逐项取自原型的 `transition` / `animation` 声明，四档各有出处——
 *
 * - `.15s`（折叠箭头 `.caret` 与差异区 `.sh .car` 的 `transform`）→ `fast`；
 * - `.2s`（toast 提示、诊断抽屉 `.dx` 的 `transform`、遮罩 `.scrim` 的 `opacity`）→ `base`；
 * - `.22s`（抽屉 `.drawer` 的 `transform`）→ `drawer`；
 * - `1.6s`（运行中圆点 `ring`、消息闪烁 `flashring`/`flashbg`、呼吸动画 `pulse`）→ `pulse`。
 *
 * 原型里还有 `.3s`（`.ctxbar i` 的宽度）、`.4s`（`stroke-dashoffset`）与 `.7s`（`spin`）三档，
 * 本仓库暂无对应动效，因此不为它们立 token。
 */
export const motion: Readonly<Record<"fast" | "base" | "drawer" | "pulse", number>> = {
  fast: 150,
  base: 200,
  drawer: 220,
  pulse: 1600,
};

/**
 * `prefers-reduced-motion: reduce` 的媒体查询串（唯一位置）。
 *
 * RNW 0.21 的 `StyleSheet.create` **不支持**把 `@media` 当样式键（它只对 `placeholderTextColor`、
 * `pointerEvents`、`scrollbarWidth` 做特例编译，其余键按属性名直通），因此「减少动效」只能走
 * 媒体查询的 JS 面：由 `src/components/shell/reduced-motion.ts` 读出取值，消费方据此换用不带时长的
 * 静态样式类。
 */
export const REDUCED_MOTION_QUERY = "(prefers-reduced-motion: reduce)";

/**
 * 网页专属样式属性的**唯一**断言点。
 *
 * `react-native` 的公开类型里没有 `scrollbarWidth` / `scrollbarColor` / `transition*` 与
 * `position: "fixed"` 这些取值，但它们都是标准 CSS：RNW 的 `StyleSheet.create` 会把它们编译成
 * **同一张同源样式表**里的原子类（`scrollbarWidth` 是 RNW 的显式支持项，其余走属性直通），
 * 因此不写内联 `style=`、不放宽 CSP 基线、也不需要新增 CSS 文件。把断言收敛在这一处，
 * 其余文件只消费编译后的类名。
 */
export function webStyle(style: Readonly<Record<string, string | number>>): ReactNative.ViewStyle {
  return style as unknown as ReactNative.ViewStyle;
}

/** 一套主题：颜色 + 尺寸 + 尺度 + 深浅色标记。 */
export interface Theme {
  readonly colors: Palette;
  readonly spacing: typeof spacing;
  readonly radius: typeof radius;
  readonly fontSize: typeof fontSize;
  readonly fontFamily: typeof fontFamily;
  readonly breakpoints: typeof breakpoints;
  readonly fontWeight: typeof fontWeight;
  readonly zIndex: typeof zIndex;
  readonly motion: typeof motion;
  readonly dark: boolean;
}

/**
 * 两套主题在模块加载时各建一次。
 *
 * 为什么要常量而不是每次渲染现拼：`useTheme()` 的返回值会进 `style` 依赖，重复渲染时
 * 若每次返回新对象，调用方的样式表与记忆化全部失效——这里保持**引用稳定**。
 *
 * 四类尺度是**同一份引用**（不是每套主题各抄一份）：它们是尺度而不是颜色，深浅色下取值相同，
 * 各写一份就制造了第二处定义。
 */
const LIGHT_THEME: Theme = {
  colors: palettes.light,
  spacing,
  radius,
  fontSize,
  fontFamily,
  breakpoints,
  fontWeight,
  zIndex,
  motion,
  dark: false,
};

const DARK_THEME: Theme = {
  colors: palettes.dark,
  spacing,
  radius,
  fontSize,
  fontFamily,
  breakpoints,
  fontWeight,
  zIndex,
  motion,
  dark: true,
};

// ── 主题偏好（D2：`system | light | dark`，缺省跟随系统）────────────────────────

/** 主题偏好：跟随系统 / 白天 / 黑夜。 */
export type ThemePreference = "system" | "light" | "dark";

/** 三档取值的封闭清单（顺序即界面上的呈现顺序）。 */
export const THEME_PREFERENCES: readonly ThemePreference[] = ["system", "light", "dark"];

/** 每档在界面上的文案（原型控制台的三档标签）。 */
export const THEME_PREFERENCE_LABELS: Readonly<Record<ThemePreference, string>> = {
  system: "跟随系统",
  light: "白天",
  dark: "黑夜",
};

/**
 * 主题偏好的持久化键。
 *
 * 它是 `docs/FRONTEND_DESIGN.md` §7 允许持久化的**客户端 UI 偏好**，因此落在**自己的键**上：
 * 与设备身份（`src/platform/` 的密钥存储）、会话正文与缓存摘要（IndexedDB）互不覆盖，
 * 且不参与任何数据判定（D2）。储存的只是三档取值之一，没有任何秘密。
 */
export const THEME_PREFERENCE_STORAGE_KEY = "acpr-theme-preference";

/** 存储面：只用到「按一个键读/写一段文本」这两个动作（`Storage` 的结构子集）。 */
export interface ThemePreferenceStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

/** 存储里的取值 → 偏好：缺失与非法取值一律退回「跟随系统」（旧用户没有这个键）。 */
export function parseThemePreference(raw: string | null | undefined): ThemePreference {
  if (raw === "light" || raw === "dark" || raw === "system") return raw;
  return "system";
}

/** 偏好 + 系统深浅色 → 实际生效的深浅色。 */
export function resolveColorScheme(preference: ThemePreference, systemDark: boolean): "light" | "dark" {
  if (preference === "system") return systemDark ? "dark" : "light";
  return preference;
}

/** 主题偏好的读写与订阅面（`useThemePreference` 的订阅源）。 */
export interface ThemePreferenceStore {
  get(): ThemePreference;
  set(next: ThemePreference): void;
  subscribe(listener: () => void): () => void;
}

/**
 * 建一个偏好仓库。
 *
 * `storage` 由调用方给出（生产是当前页面的存储，测试是替身）：这样「显式取值跨刷新保留」
 * 可以断言成「换一个仓库实例、读同一份存储」，而不是靠模块级的全局桩
 * （与 `design.md` D3 拒绝全局打桩同一条理由）。
 */
export function createThemePreferenceStore(storage: ThemePreferenceStorage): ThemePreferenceStore {
  let cached: ThemePreference | null = null;
  const listeners = new Set<() => void>();
  return {
    get(): ThemePreference {
      if (cached === null) {
        let raw: string | null = null;
        try {
          raw = storage.getItem(THEME_PREFERENCE_STORAGE_KEY);
        } catch {
          // 浏览器在隐私/配额形态下会**抛错**而不是返回 null：读回「跟随系统」即可。
          raw = null;
        }
        cached = parseThemePreference(raw);
      }
      return cached;
    },
    set(next: ThemePreference): void {
      if (cached === next) return;
      cached = next;
      try {
        storage.setItem(THEME_PREFERENCE_STORAGE_KEY, next);
      } catch {
        // 写不进去（隐私/配额）：偏好只在本次会话内生效，页面照常工作。
      }
      for (const listener of [...listeners]) listener();
    },
    subscribe(listener: () => void): () => void {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

/**
 * 页面自己的存储面；没有 `localStorage`（node / 组件测试）或读不到它（某些隐私设置下读这个属性
 * 本身就会抛错）时退回一份内存存储——那里不持久化，取值仍恒定为「跟随系统」。
 */
function pageStorage(): ThemePreferenceStorage {
  try {
    const storage = globalThis.localStorage;
    if (storage !== undefined) return storage;
  } catch {
    // 落到下面的内存存储。
  }
  let value: string | null = null;
  return {
    getItem: () => value,
    setItem: (_key, next) => {
      value = next;
    },
  };
}

/** 进程内唯一的偏好仓库：页面、外壳与全部主题消费方共用同一份订阅源。 */
export const themePreferenceStore: ThemePreferenceStore = createThemePreferenceStore(pageStorage());

/**
 * 当前主题偏好（订阅式读取；写入用 `themePreferenceStore.set()`）。
 *
 * 本页是**客户端渲染**（`app.json` 的 `web.output: "single"`），没有服务端渲染路径；
 * 第三个参数只服务 `renderToStaticMarkup` 的组件测试，取值与客户端一致。
 */
export function useThemePreference(): ThemePreference {
  const subscribe = useCallback((listener: () => void) => themePreferenceStore.subscribe(listener), []);
  const snapshot = useCallback(() => themePreferenceStore.get(), []);
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}

/**
 * 当前主题：按主题偏好解析（跟随系统时读 `useColorScheme()`）。
 *
 * 返回契约与本节之前完全一致（仍是两套 `Theme` 常量之一），因此既有 22 个组件的 `style`
 * 依赖不受影响。
 */
export function useTheme(): Theme {
  const preference = useThemePreference();
  return resolveColorScheme(preference, useColorScheme() === "dark") === "dark" ? DARK_THEME : LIGHT_THEME;
}

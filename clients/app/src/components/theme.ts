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
 * `useTheme()` 只跟随系统深浅色（D9）：产品内的主题开关是 `docs/FRONTEND_DESIGN.md` §11 的
 * 暂缓决策，本次不引入。
 */

import type * as ReactNative from "react-native";

// @ts-expect-error `react-native-web` 不发布类型声明；把它的运行时值绑到 `react-native` 的公开类型上。
import * as ReactNativeWeb from "react-native-web";

/**
 * `dataSet` 是 react-native-web 的转发表成员（`data-directory-alias` 这类 `data-*` 属性由它
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

/** 调色板的键：与原型 `:root` 的 CSS 变量一一对应。 */
export type ColorToken =
  | "bg"
  | "panel"
  | "panel2"
  | "line"
  | "line2"
  | "ink"
  | "ink2"
  | "ink3"
  | "accent"
  | "accentInk"
  | "accentSoft"
  | "ok"
  | "okSoft"
  | "warn"
  | "warnSoft"
  | "danger"
  | "dangerSoft"
  | "think"
  | "thinkSoft"
  | "tool"
  | "toolSoft"
  | "shadow";

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

/** 一套主题：颜色 + 尺寸 + 深浅色标记。 */
export interface Theme {
  readonly colors: Palette;
  readonly spacing: typeof spacing;
  readonly radius: typeof radius;
  readonly fontSize: typeof fontSize;
  readonly fontFamily: typeof fontFamily;
  readonly dark: boolean;
}

/**
 * 两套主题在模块加载时各建一次。
 *
 * 为什么要常量而不是每次渲染现拼：`useTheme()` 的返回值会进 `style` 依赖，重复渲染时
 * 若每次返回新对象，调用方的样式表与记忆化全部失效——这里保持**引用稳定**。
 */
const LIGHT_THEME: Theme = {
  colors: palettes.light,
  spacing,
  radius,
  fontSize,
  fontFamily,
  dark: false,
};

const DARK_THEME: Theme = {
  colors: palettes.dark,
  spacing,
  radius,
  fontSize,
  fontFamily,
  dark: true,
};

/** 随系统深浅色（`useColorScheme()`）返回当前主题。 */
export function useTheme(): Theme {
  return useColorScheme() === "dark" ? DARK_THEME : LIGHT_THEME;
}

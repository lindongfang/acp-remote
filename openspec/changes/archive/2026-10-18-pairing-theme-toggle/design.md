# Design

## Context

配对页由 `app/pair.tsx` 的 `PairingRouteBody` 承载；`/node-link/pair` 复用同一个主体。共享外壳在配对路径隐藏顶栏（`ShellView` 的 `topBarVisible`），因此顶栏里的主题按钮不可用。主题偏好由 `src/components/theme.ts` 的 `themePreferenceStore` 统一持有，`useTheme()` 按偏好解析当前调色板；`PairingPanel` 已经用 `useTheme()` 建好浅/深两套样式。

## Goals / Non-Goals

**Goals:**

- 在配对页提供与共享外壳一致的明暗切换，不改共享外壳的顶栏例外。
- 复用同一份 `themePreferenceStore`，刷新后保持；不新增存储键或第二份状态。
- 按钮在桌面/窄屏都浮在右上角，不遮挡状态内容。

**Non-Goals:**

- 不在配对页提供三态选择器；“跟随系统”仍由共享外壳的三态控件选择。
- 不恢复顶栏、页头、返回入口或帮助入口。
- 不改 `theme.ts` 的偏好语义或任何协议。

## Decisions

### D1：独立的页内按钮，复用共享偏好

新增 `PairingThemeToggle` 组件：

- `useTheme()` 取 `dark` 与调色板；`useThemePreference()` 只用于诊断锚点（`dataSet.themePreference`）。
- 点击执行 `themePreferenceStore.set(theme.dark ? "light" : "dark")`，与共享外壳的按钮同一路径。
- 不把 `theme` 放进 `ClientStore`/`ClientState`，不新增 `localStorage` 键；`useTheme` 的订阅会让页面与按钮一起重渲染。

### D2：浮在页面右上角，不占布局

- `webStyle({ position: "fixed", top: 18, right: 18 })`（600px 以下 `12px`），`zIndex` 取共享外壳顶栏层。
- 36×36、圆角 10、`border` + `panel` 背景 + `shadow`，逐项取原型 `.pairtheme`
  （`prototypes/acp-remote-pwa.html:485-495`）；太阳/月亮沿用与 `ShellView` 相同的图形（圆形 + 咬缺）。
- 图标颜色取原型 `.pairtheme[data-ic="orange"]` 的亮橙（白天 `#d9480f` / 黑夜 `#ffa94d`）。这是原型里
  唯一不来自 CSS 变量（也就是不来自 `palettes`）的主题色，因此只在 `PairingThemeToggle` 里按深浅色各给一次，
  不新增调色板 token。
- 按钮 `aria-label` 与共享外壳一致：“当前：白天，点击切换” / “当前：黑夜，点击切换”。
- `data-action="toggle-theme"`、`data-pairing-theme-toggle="true"`，供 e2e 与组件测试定位。

### D3：放在页面骨架而不是面板

`PairingRouteBody` 的页面 `View` 同时包住 `RuntimeUnavailable` 与 `PairingPanel`；按钮放在这一层，因此：

- `/pair` 与 `/node-link/pair` 都有；
- 运行时未装配完成时也能切主题；
- 面板内部的失败页/输入页不需要各自重复按钮。

### D4：测试

- 组件测试：浅色/深色下分别渲染，断言 `aria-label`、`data-action`、`data-theme-preference` 与太阳/月亮图形。
- 浏览器 e2e：在 `/pair` 点击按钮，断言 `aria-label` 从白天翻到黑夜（或反向）、页面调色板跟随、`localStorage` 的共享键更新；刷新后保持。
- 现有配对 e2e（S0、lookup、二维码路径）不应因按钮出现而失败。

## Risks / Trade-offs

- **固定按钮遮挡**：右上角可能压住长文案或窄屏内容。缓解：12px 偏移、36×36、页面内容居中且顶部留白；e2e 用 320px 宽确认不遮挡关键输入格。
- **移动端标题栏差异**：移动原型把按钮放在标题栏右侧，PWA 没有配对页标题栏，因此统一用浮动按钮；这是对移动原型的合理适配，不恢复页头。
- **主题闪烁**：按钮与页面都订阅 `useTheme()`，切换时同一次渲染更新；没有第二份状态，不会出现按钮和页面不同步。

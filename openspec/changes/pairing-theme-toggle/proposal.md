# Proposal

## Why

原型给配对页加了一个明暗切换按钮：桌面端浮在右上角，移动端放在配对页自己的标题栏。实现里的配对页仍刻意不提供页内主题切换（共享外壳在配对路径隐藏顶栏），用户进入 `/pair` 后无法切换明暗，只能回目录页操作。这与原型和“配对页完整复用当前主题”的产品口径不一致。

## What Changes

- 在 `/pair` 与 `/node-link/pair` 页面增加一个浮动的明暗切换按钮（桌面/窄屏都在右上角），不绘制页头、不恢复全局顶栏。
- 按钮复用共享的 `themePreferenceStore` / `useTheme` / `useThemePreference`：点击在白天/黑夜之间切换并持久化，MUST NOT 建立第二份主题状态。
- 页面调色板继续完全来自 `useTheme()` 的当前主题；按钮自身也用同一套调色板。
- 更新 `pwa-web-client` 规格、`docs/FRONTEND_DESIGN.md` 与 `prototypes/README.md` 中“实现不提供页内主题切换”的过时说明。
- 增加组件测试与浏览器 e2e：按钮存在、点击后标签/主题变化、偏好持久化。

## Capabilities

### Modified Capabilities

- `pwa-web-client`：配对入口允许页内明暗切换控件，但必须复用共享偏好。

### New Capabilities

无。

## 非目标

- 不在配对页提供三态主题选择器；“跟随系统 / 白天 / 黑夜”的三态选择仍在共享外壳。
- 不改变主题偏好的存储键或语义。
- 不恢复配对页的全局顶栏、页头或返回入口。
- 不改 native 客户端的行为。

## 成功判据

- 打开 `/pair` 或 `/node-link/pair` 时能看到右上角的明暗切换按钮；点击后页面立即切换主题，刷新后保持。
- 按钮使用与共享外壳相同的主题偏好，不出现两套主题状态。
- 配对页仍不呈现全局顶栏、设备胶囊、页头或返回入口。
- `npm run typecheck`、`npm run test`、`node e2e/run-e2e.mjs` 通过。

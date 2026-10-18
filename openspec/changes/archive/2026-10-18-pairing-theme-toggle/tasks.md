# Tasks

## 1. WP1：配对页明暗切换

- [x] 1.1 新增 `clients/app/src/components/PairingThemeToggle.tsx`：复用 `themePreferenceStore`/`useTheme`/`useThemePreference`，固定右上角，太阳/月亮图形与共享外壳一致，`data-action="toggle-theme"`。
- [x] 1.2 在 `clients/app/src/components/index.ts` 导出，并在 `app/pair.tsx` 的页面骨架里渲染；`RuntimeUnavailable` 与 `PairingPanel` 两个分支都可见。
- [x] 1.3 更新 `PairingPanel.tsx`/`pair.tsx` 的模块注释：移除“本页不提供主题切换”的过时说明，写清复用共享偏好。
- [x] 1.4 增加组件测试：浅/深两态渲染、`aria-label`、`data-action`、图形；浏览器 e2e：`/pair` 点击切换、标签翻转、刷新保持，且原有配对用例不回归。
- [x] 1.5 更新 `docs/FRONTEND_DESIGN.md` 与 `prototypes/README.md` 中“实现不提供页内主题切换”的表述。
- [x] 1.6 运行 `cd clients/app && npm run typecheck && npm run test`、`node e2e/run-e2e.mjs`、`npm run check:docs`、`npm run check:openspec`；全绿后提交分支与 PR。

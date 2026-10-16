# Tasks

## 1. 复现与定位

- [x] 1.1 用**带真实 CSP 的来源**复现样式失效：断言 379 字符那条 `<style>` 的 `sheet === null`、`document.styleSheets.length === 0`、以及至少一处关键元素的计算样式与预期不符。完成判据：复现方式与输出写进报告（不得只跑 `clients/app` 的门禁就宣布无问题——它不带这套 CSP）。
- [x] 1.2 判定修复落点（前端构建产物 / 托管注入 / 样式形态），并在 design.md 写清取舍与备选。完成判据：design.md 的 Decision 与 Risks 可被 reviewer 逐条核对。

## 2. 修复与回归

- [x] 2.1 在前端侧改到「既定 CSP 下样式生效」。完成判据：在带该 CSP 的静态面上断言关键样式真的生效（计算样式等于预期）；把样式改回内联时该断言必须变红。
- [x] 2.2 补一条 CI 可见的回归：让「样式生效」这条断言进入 `npm run check` 或前端 e2e 的检查集（不能只留在一次性脚本里）。完成判据：`cd clients/app && npm run check` 与 e2e 全绿，且该断言在 2.1 被回退时变红。

## 3. 文档

- [x] 3.1 如实登记与 `docs/FRONTEND_DESIGN.md` / `README.md` 的差异（尤其是「同源托管下前端可用」这一条的实绩）。完成判据：`npm run check:docs`、`npm run check:drift` 全绿。

/**
 * 目录页路由（空壳，WP7 实现）。
 *
 * `docs/FRONTEND_DESIGN.md` §9 与原型的路由口径是 `#/dirs`（目录为主角）。
 * 这里只声明路由存在与稳定的 `data-route` 选择器（TP4 的浏览器用例按稳定选择器编写），
 * 页面内容由 WP7 填充。
 */
import type { ReactElement } from "react";


export default function DirectoriesRoute(): ReactElement {
  return <div data-route="dirs" />;
}

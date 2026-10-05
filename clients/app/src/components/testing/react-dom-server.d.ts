/**
 * `react-dom/server` 的最小类型声明。
 *
 * v1 只交付 Web/PWA，`package.json` 的依赖由 WP5a 冻结且不允许在本轮追加
 * （`plan.md` Shared File Ownership：`clients/app/package.json` 只在既有 dependencies 上追加，
 * 而追加一个 `@types/react-dom` 属于新增依赖）。组件层的渲染断言只需要一个纯函数：
 * 把元素树渲染成 HTML 字符串。因此这里声明它的**实际形状**而不是整个 ReactDOM 类型面。
 */
declare module "react-dom/server" {
  /** 把元素树渲染成静态 HTML（服务端渲染用的那一半）。 */
  export function renderToStaticMarkup(element: unknown): string;
}

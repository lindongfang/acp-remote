/**
 * `react-dom/client` 的最小类型声明。
 *
 * 与 `src/components/testing/react-dom-server.d.ts` 同一条理由：`clients/app/package.json`
 * 的依赖被冻结，追加 `@types/react-dom` 属于新增依赖，不在本工作包的授权范围内。
 * 因此这里只声明 e2e 用到的那一个函数，并让它返回**与 `Root.render` 同形状**的句柄。
 */
declare module "react-dom/client" {
  /** 一个已挂载的 React 根。 */
  export interface Root {
    render(element: unknown): void;
    unmount(): void;
  }

  /** 在容器上创建 React 根。 */
  export function createRoot(container: Element | DocumentFragment): Root;
}
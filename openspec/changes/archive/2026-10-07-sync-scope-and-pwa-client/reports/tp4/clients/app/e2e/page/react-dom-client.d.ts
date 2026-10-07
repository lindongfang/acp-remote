// 【交接副本】来源 worktree .worktrees/tp4 的 clients/app/e2e/page/react-dom-client.d.ts
// 交付提交 0a19d39b4c63b723c7e7d7edbaaf64e60cd82e83（分支 feat/tp4），基线 f6ea252。
// 用途：premerge 门禁按「相对变更目录」解析 test_delivery.artifacts 的可读性；本副本在权威交付物之外。
// 除顶部本 4 行说明外与来源逐字节相同（权威可运行副本在 worktree 内，由 PV3 执行）。
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
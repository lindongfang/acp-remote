/**
 * `expo-router` 的最小替身（只给本目录的用例用）。
 *
 * ## 为什么需要它
 *
 * `app/` 下的真实路由组件 `import { useRouter } from "expo-router"`。真实的
 * expo-router 入口依赖 Metro/Metro 运行时配置与 `react-native-web` 的打包管线，
 * 本工作包既没有新增依赖的授权，也无法在一条 esbuild 命令里复现那套管线。
 * esbuild 打包时把 `expo-router` 这个**裸说明符**指向本文件（见 `run-e2e.mjs` 的插件）。
 *
 * ## 它替掉了什么、没替掉什么
 *
 * 替掉：路由表查找、`push` / `back`、`useLocalSearchParams` 的参数来源、`usePathname` 的当前路径
 * （共享外壳据此判定是否呈现全局顶栏，见 `src/components/shell/Shell.tsx`）。
 * **没替掉**：组合根、连接状态机、`ClientStore`、全部 feature 判定与展示组件——
 * 它们一行都没有被替身碰到，仍然是 `app/` 与 `src/` 里的真实代码。
 *
 * 因此本目录能回答的是「页面在真实 DOM 里渲染成什么样、点了按钮发出什么字节」，
 * **不能**回答「真实 expo-router 的 URL 同步与历史栈行为」——那不在 TP4 的交付范围内，
 * 已在测试设计报告的「未验证项」里登记。
 *
 * ## 路由组件必须交给 React 当**子元素**渲染，不能直接调用
 *
 * `Stack` 早先写成 `return current.component()`：那是在 `Stack` 自己的 fiber 上**调用**路由
 * 组件函数，于是路由组件的 hooks 与 `Stack` 的 `useSyncExternalStore` 共用同一批槽位。
 * 一旦切到 hooks 序列不同的路由组件（目录页 ↔ 详情页），React 的更新分派器会拿上一轮的
 * `deps` 去比对新一轮的 effect，抛
 * `TypeError: Cannot read properties of undefined (reading 'length')` 并把整棵树卸载
 * （实测：点击目录项后容器 `innerHTML` 变空、`[data-route]` 一个都不剩）。
 * 因此这里 `createElement(current.component, { key: current.path })`：hooks 归路由组件自己，
 * 槽位不会串；`key` 取路径则让**同类型**路由之间切换（`/dir/alpha` → `/dir/beta`）强制重挂，
 * 上一屏的局部状态（页面上选中的 Agent）不会被带进下一屏——真实 expo-router 的栈里
 * 每屏都是一个新实例。
 */

import { createElement, useSyncExternalStore } from "react";
import type { ReactElement, ReactNode } from "react";

/** 一个路由条目：组件 + 该路由的参数。 */
export interface RouteEntry {
  readonly path: string;
  readonly component: () => ReactElement;
  readonly params: Readonly<Record<string, string>>;
}

const listeners = new Set<() => void>();
let version = 0;
let current: RouteEntry | null = null;
const registry = new Map<string, RouteEntry>();
const history: string[] = [];

function emit(): void {
  version += 1;
  for (const listener of [...listeners]) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function getVersion(): number {
  return version;
}

/**
 * 注册本场景可导航到的路由（`push` 按路径查表）。
 *
 * **整表替换**而不是累加：`registry` 是模块级的，累加会让上一条用例注册过的路径在本条里
 * 仍然可导航——那样「这条用例忘了给 `extraRoutes`」就不会红，而会静悄悄地点到一个本该
 * 不可达的页面。每条用例都调一次本函数（`support.tsx` 的 `mountScenario`），因此这里的
 * 替换语义就是「本场景能去哪些页」。
 */
export function registerRoutes(routes: readonly RouteEntry[]): void {
  registry.clear();
  for (const route of routes) registry.set(route.path, route);
}

/** 直接切到某个路由（用例的初始页面）。 */
export function mountRoute(route: RouteEntry): void {
  registry.set(route.path, route);
  current = route;
  // 历史栈从初始路由起算：`navigate` 只把**目标**路径压栈，若这里留空，第一次 `goBack()`
  // 弹完就无路可退（详情页的「返回目录」会变成一个没有反应的按钮）。
  history.length = 0;
  history.push(route.path);
  emit();
}

/** 导航到已注册的路径；未注册则忽略（真实 expo-router 会渲染未匹配路由，这里不需要）。 */
export function navigate(path: string): void {
  const next = registry.get(path);
  if (next === undefined) return;
  current = next;
  history.push(path);
  emit();
}

/** 返回上一条路径；栈空时不动（真实实现会离开应用，本用例用不到）。 */
export function goBack(): void {
  history.pop();
  const previous = history[history.length - 1];
  const entry = previous === undefined ? null : (registry.get(previous) ?? null);
  if (entry === null) return;
  current = entry;
  emit();
}

/** 当前路由路径；用例用它断言导航确实发生了。 */
export function currentPath(): string | null {
  return current?.path ?? null;
}

/** 路由出口：`app/_layout.tsx` 渲染的 `<Stack />`。 */
export function Stack(_props: { readonly screenOptions?: unknown; readonly children?: ReactNode }): ReactElement | null {
  useSyncExternalStore(subscribe, getVersion, getVersion);
  if (current === null) return null;
  // 作为**元素**交给 React（见文件头）：`key` 取路径，同类型路由之间切换时强制重挂。
  return createElement(current.component, { key: current.path });
}

/** `useRouter()`：只提供页面用到的方法。 */
export function useRouter(): {
  push: (path: string) => void;
  back: () => void;
  replace: (path: string) => void;
} {
  return { push: navigate, back: goBack, replace: navigate };
}

/** `useLocalSearchParams()`：取当前路由参数。 */
export function useLocalSearchParams<T extends Record<string, string>>(): T {
  useSyncExternalStore(subscribe, getVersion, getVersion);
  return (current?.params ?? {}) as T;
}

/**
 * `usePathname()`：当前路由的路径。
 *
 * 真实实现从链接状态里取地址栏的 pathname（查询串与片段都不进路径），这里取替身自己的当前条目
 * ——两者在「同一个 URL 路径对应同一个取值」上一致；尚未挂任何路由时按根路径 `"/"` 处理。
 * 订阅方式与 `useLocalSearchParams` 相同：`navigate`/`goBack` 会让用它判定的组件重新渲染。
 */
export function usePathname(): string {
  useSyncExternalStore(subscribe, getVersion, getVersion);
  return current?.path ?? "/";
}
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
 * 替掉：路由表查找、`push` / `back`、`useLocalSearchParams` 的参数来源。
 * **没替掉**：组合根、连接状态机、`ClientStore`、全部 feature 判定与展示组件——
 * 它们一行都没有被替身碰到，仍然是 `app/` 与 `src/` 里的真实代码。
 *
 * 因此本目录能回答的是「页面在真实 DOM 里渲染成什么样、点了按钮发出什么字节」，
 * **不能**回答「真实 expo-router 的 URL 同步与历史栈行为」——那不在 TP4 的交付范围内，
 * 已在测试设计报告的「未验证项」里登记。
 */

import { useSyncExternalStore } from "react";
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

/** 注册可导航到的路由（`push` 按路径查表）。 */
export function registerRoutes(routes: readonly RouteEntry[]): void {
  for (const route of routes) registry.set(route.path, route);
}

/** 直接切到某个路由（用例的初始页面）。 */
export function mountRoute(route: RouteEntry): void {
  registry.set(route.path, route);
  current = route;
  history.length = 0;
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
  return current.component();
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
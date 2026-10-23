import { useEffect } from "react";

/** 已批准后停留在这里的过渡时长（design D4 / 原型 S1）。 */
export const APPROVED_REDIRECT_MS = 1_600;

interface PairingConnectionState {
  readonly host: unknown | null;
  readonly connection: { readonly state: string };
}

interface PairingRouter {
  replace(href: "/"): void;
}

export interface PairingNavigationOptions {
  /** 本页路由声明的期望类型。 */
  readonly kind: "sync" | "node-link";
  /** 当前显示的确实是本页，而不是被 Stack 保留在后台的实例。 */
  readonly isCurrentRoute: boolean;
  readonly state: PairingConnectionState | null;
  /** 本次配对已成为「已批准 + sync」过渡。 */
  readonly approvedSync: boolean;
  readonly router: PairingRouter;
  readonly approvedRedirectMs?: number;
}

/**
 * 配对页的两条导航效应：已配对过的设备直接回目录，以及批准后延迟进入目录。
 *
 * 放在组件层而不是页面里，页面只负责提供路由与状态；离开本页（后台实例）后不得响应
 * store 更新覆盖当前导航，身份变化或设备撤销后也不得据旧主机记录跳回目录。
 */
export function usePairingNavigation({
  kind,
  isCurrentRoute,
  state,
  approvedSync,
  router,
  approvedRedirectMs = APPROVED_REDIRECT_MS,
}: PairingNavigationOptions): void {
  useEffect(() => {
    // Stack 会保留后台页面；离开配对页后不能再响应 store 更新并覆盖当前导航。
    if (!isCurrentRoute || kind !== "sync") return;
    if (state === null || state.host === null) return;
    if (state.connection.state === "identity_changed" || state.connection.state === "revoked") return;
    if (approvedSync) return;
    router.replace("/");
  }, [isCurrentRoute, kind, state, approvedSync, router]);

  useEffect(() => {
    if (!isCurrentRoute || !approvedSync) return;
    const timer = setTimeout(() => {
      // 目录页就是根路由：`app/index.tsx` 匹配 `/`（`DIRS_ROUTE.path === "/"`）。
      router.replace("/");
    }, approvedRedirectMs);
    return () => {
      clearTimeout(timer);
    };
  }, [isCurrentRoute, approvedSync, router, approvedRedirectMs]);
}

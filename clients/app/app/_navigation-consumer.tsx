import { usePathname, useRouter } from "expo-router";
import { useEffect, useRef } from "react";

import { useClientState, useClientStore } from "../src/features/runtime";

/** 根布局把 feature 层的一次性创建导航意图接到 Expo Router。 */
export function NavigationConsumer(): null {
  const router = useRouter();
  const pathname = usePathname();
  const store = useClientStore();
  const state = useClientState();
  const previousPathname = useRef(pathname);

  useEffect(() => {
    if (previousPathname.current === pathname) return;
    previousPathname.current = pathname;
    store?.noteRouteChanged();
  }, [pathname, store]);

  useEffect(() => {
    const target = state?.navigationTarget;
    if (store === null || target == null || target.reason === "deleted") return;
    const claimed = store.takeNavigationTarget();
    if (claimed?.kind !== "session") return;
    store.closeCreateSheet();
    router.push(`/session/${encodeURIComponent(claimed.sessionId)}`);
  }, [router, state?.navigationTarget, store]);

  return null;
}

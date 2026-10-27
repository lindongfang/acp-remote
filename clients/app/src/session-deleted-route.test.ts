import { createElement } from "react";
import type { ReactElement } from "react";
import { act, create } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ClientStore } from "./features/client-store";
import { ClientStoreProvider } from "./features/runtime";
import {
  ONLINE_SESSION_ID,
  eventMessage,
  onlineStore,
  resources,
  session,
} from "./features/testing/fixtures";

const routeHarness = vi.hoisted(() => {
  const replace = vi.fn();
  const push = vi.fn();
  return {
    sessionId: "",
    pathname: "",
    router: {
      replace,
      push,
      back: vi.fn(),
      canGoBack: vi.fn(() => false),
    },
    showToast: vi.fn(),
  };
});

vi.mock("expo-router", () => ({
  useLocalSearchParams: () => ({ id: routeHarness.sessionId }),
  usePathname: () => routeHarness.pathname,
  useRouter: () => routeHarness.router,
}));

vi.mock("./components/theme", () => ({
  View: "div",
  StyleSheet: {
    create: <T>(styles: T): T => styles,
  },
  breakpoints: { wide: 900 },
  palettes: {
    light: { bg: "#fff" },
    dark: { bg: "#111" },
  },
  useTheme: () => ({ dark: false, colors: { bg: "#fff" } }),
  useWindowDimensions: () => ({ width: 1200 }),
}));

vi.mock("./components", () => ({
  ConversationStream: () => null,
  CreateSessionSheet: () => null,
  DiagnosticsDrawer: () => null,
  RuntimeUnavailable: () => null,
}));

vi.mock("./components/shell/toast", () => ({
  toastStore: { show: routeHarness.showToast },
}));

vi.mock("./features/session-file-download", () => ({
  currentSessionFileDownloader: () => null,
}));

import { NavigationConsumer } from "../app/_navigation-consumer";
import SessionRoute from "../app/session/[id]";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let rendered: ReactTestRenderer | null = null;

function RootLayoutHarness({ store }: { readonly store: ClientStore }): ReactElement {
  return createElement(
    ClientStoreProvider,
    {
      store,
      children: [
        createElement(NavigationConsumer, { key: "navigation" }),
        createElement(SessionRoute, { key: "session" }),
      ],
    },
  );
}

async function renderHarness(store: ClientStore): Promise<ReactTestRenderer> {
  let renderer: ReactTestRenderer | null = null;
  await act(async () => {
    renderer = create(createElement(RootLayoutHarness, { store }));
  });
  if (renderer === null) throw new Error("session route harness did not mount");
  rendered = renderer;
  return renderer;
}

function visibleText(renderer: ReactTestRenderer): string {
  return JSON.stringify(renderer.toJSON());
}

function deletion(eventId: string) {
  return eventMessage({
    eventType: "session.deleted",
    eventId,
    sessionId: ONLINE_SESSION_ID,
    globalSequence: "11",
    view: { sessionId: ONLINE_SESSION_ID },
  });
}

beforeEach(() => {
  routeHarness.router.replace.mockClear();
  routeHarness.router.push.mockClear();
  routeHarness.showToast.mockClear();
  routeHarness.sessionId = "";
  routeHarness.pathname = "";
});

afterEach(async () => {
  if (rendered !== null) {
    await act(async () => {
      rendered?.unmount();
    });
    rendered = null;
  }
});

describe("session route deletion", () => {
  it("mounts the root consumer and route together, then navigates a current tombstone home exactly once", async () => {
    const { store } = onlineStore({
      resources: resources({
        sessions: [session({ id: ONLINE_SESSION_ID, state: "idle", localActivity: "active" })],
      }),
    });
    store.setActiveConversationSession(ONLINE_SESSION_ID);
    routeHarness.sessionId = ONLINE_SESSION_ID;
    routeHarness.pathname = `/session/${ONLINE_SESSION_ID}`;

    const renderer = await renderHarness(store);
    expect(visibleText(renderer)).not.toContain("可能已被删除");

    await act(async () => {
      store.apply({
        kind: "event",
        event: deletion("dddddddd-0000-4000-8000-000000000010"),
      });
    });

    expect(visibleText(renderer)).not.toContain('role="alert"');
    expect(visibleText(renderer)).not.toContain("可能已被删除");
    expect(routeHarness.router.replace).toHaveBeenCalledOnce();
    expect(routeHarness.router.replace).toHaveBeenCalledWith("/");
    expect(routeHarness.router.push).not.toHaveBeenCalled();
    expect(routeHarness.showToast).not.toHaveBeenCalled();
    expect(store.state.navigationTarget).toBeNull();

    await act(async () => {
      store.apply({
        kind: "event",
        event: deletion("dddddddd-0000-4000-8000-000000000011"),
      });
    });

    expect(routeHarness.router.replace).toHaveBeenCalledOnce();
    expect(store.state.navigationTarget).toBeNull();
  });

  it("keeps the human-readable recovery alert for an unknown or not-yet-synced resource", async () => {
    const unknownSessionId = "eeeeeeee-0000-4000-8000-000000000001";
    const { store } = onlineStore({
      resources: resources({
        sessions: [session({ id: ONLINE_SESSION_ID, state: "idle", localActivity: "active" })],
      }),
    });
    routeHarness.sessionId = unknownSessionId;
    routeHarness.pathname = `/session/${unknownSessionId}`;

    const renderer = await renderHarness(store);
    expect(visibleText(renderer)).toContain("可能已被删除");
    expect(routeHarness.router.replace).not.toHaveBeenCalled();
    expect(routeHarness.showToast).not.toHaveBeenCalled();
  });
});

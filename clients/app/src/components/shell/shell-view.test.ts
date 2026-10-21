/**
 * 共享外壳的呈现（`spec` 的「共享外壳承担连接状态与全局入口」，`design.md` D1）。
 *
 * 渲染真实呈现层、给**显式输入**：因此「根路由呈现顶栏」「配对路由不呈现全局顶栏」「明文连接
 * 呈现横幅与徽标」「阻断态呈现对应短词」四条都是可观察行为上的断言，而不是对源码文本的检查。
 *
 * 「配对路由不呈现顶栏」同时断言外壳本体与路由内容**仍在**——只断言「没有顶栏」在「整页什么都没
 * 渲染」时也会绿，那样就证明不了是判据在起作用。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { buildConnectionProblem } from "../../features/connection-problem";
import type { ConnectionProblemModel } from "../../features/connection-problem";
import type { ThemePreference } from "../theme";
import { buildShellModel } from "./shell-model";
import type { ShellModel } from "./shell-model";
import { ShellView } from "./ShellView";
import type { ShellViewProps } from "./ShellView";

/** 所有回调都留空：本文件的断言只看呈现，交互由浏览器用例负责。 */
const NOOP_HANDLERS = {
  onToggleTheme: () => {},
  onSelectTheme: () => {},
  onOpenHelp: () => {},
  onCloseHelp: () => {},
  onOpenHosts: () => {},
  onRepair: () => {},
  onReload: () => {},
} as const;

/** 用例要覆盖的入参；其余按缺省补齐。 */
interface ShellRenderInput {
  readonly model: ShellModel;
  readonly themePreference?: ThemePreference;
  readonly themeDark?: boolean;
  readonly reducedMotion?: boolean;
  readonly helpOpen?: boolean;
  readonly toastMessage?: string | null;
  readonly problem?: ConnectionProblemModel | null;
}

function renderShell(input: ShellRenderInput): string {
  const props: ShellViewProps = {
    model: input.model,
    themePreference: input.themePreference ?? "system",
    themeDark: input.themeDark ?? false,
    reducedMotion: input.reducedMotion ?? false,
    helpOpen: input.helpOpen ?? false,
    toastMessage: input.toastMessage ?? null,
    problem: input.problem ?? null,
    ...NOOP_HANDLERS,
  };
  return renderToStaticMarkup(createElement(ShellView, props, createElement("main", null, "路由内容")));
}

/** 固定时刻的问题模型：五个变体共用同一份关闭记录，只有连接状态不同。 */
function problemFor(state: "disconnected" | "reconnecting" | "revoked" | "incompatible" | "identity_changed" | "replaced"): ConnectionProblemModel {
  const problem = buildConnectionProblem({
    connection: state,
    hasAuthenticated: true,
    lastClose: { code: 4500, reason: "snapshot_validation_failed", atMs: Date.parse("2026-10-01T00:00:08.000Z") },
    reconnect: { attempts: 3, nextRetryAtMs: Date.parse("2026-10-01T00:00:16.000Z") },
    nowMs: Date.parse("2026-10-01T00:00:08.000Z"),
  });
  if (problem === null) throw new Error("这些状态都应产生连接问题模型");
  return problem;
}

/** 在线模型：弹层的覆盖关系与顶栏是否可见都要在同一个页面上看。 */
const ONLINE_MODEL = buildShellModel({
  pathname: "/",
  connectionState: "online",
  hostAddress: "https://pc.example.ts.net",
  secureContext: true,
});

/** 某个属性片段在渲染结果里出现了几次（用于「只有一处」这类断言）。 */
function countOccurrences(html: string, fragment: string): number {
  return html.split(fragment).length - 1;
}

describe("顶栏：根路由呈现、配对路由不呈现", () => {
  it("根路由呈现顶栏与设备胶囊，路由内容仍在外壳里", () => {
    const html = renderShell({
      model: buildShellModel({
        pathname: "/",
        connectionState: "online",
        hostAddress: "https://pc.example.ts.net",
        secureContext: true,
      }),
    });
    expect(html).toContain('data-shell-top-bar="visible"');
    expect(html).toContain('data-device-capsule="true"');
    expect(html).toContain('data-shell-content="true"');
    expect(html).toContain("ACP Remote");
    expect(html).toContain("路由内容");
  });

  it("配对路由不呈现顶栏（外壳本体与路由内容仍在）", () => {
    const html = renderShell({
      model: buildShellModel({
        pathname: "/pair",
        connectionState: "online",
        hostAddress: "https://pc.example.ts.net",
        secureContext: true,
      }),
    });
    expect(html).not.toContain("data-shell-top-bar");
    expect(html).not.toContain("data-device-capsule");
    expect(html).toContain('data-shell="root"');
    expect(html).toContain('data-shell-content="true"');
    expect(html).toContain("路由内容");
  });

  it("连接状态只由胶囊表达：不在别处呈现第二个状态徽标", () => {
    const html = renderShell({
      model: buildShellModel({
        pathname: "/",
        connectionState: "reconnecting",
        hostAddress: null,
        secureContext: true,
      }),
    });
    // 胶囊只出现一次，且状态只挂在它上面：整个外壳里没有 `data-connection-state` 这类第二处锚点。
    expect(countOccurrences(html, 'data-device-capsule="true"')).toBe(1);
    expect(html).not.toContain("data-connection-state");
  });
});

describe("按视口填满：只有对话路由", () => {
  it("对话路由 contentFill 为 true，目录页/详情页/主机页/配对入口为 false", () => {
    const shellOf = (pathname: string): ShellModel =>
      buildShellModel({ pathname, connectionState: "online", hostAddress: null, secureContext: true });

    expect(shellOf("/session/aaaaaaaa-0000-4000-8000-000000000001").contentFill).toBe(true);
    // 查询串、片段与尾斜杠不改变判定。
    expect(shellOf("/session/abc/?x=1#frag").contentFill).toBe(true);
    for (const pathname of ["/", "/dir/work-api", "/hosts", "/pair", "/node-link/pair", "/session", "/session/"]) {
      expect(shellOf(pathname).contentFill, pathname).toBe(false);
    }
  });

  it("呈现层只在对话路由给根与内容区不同的装配（其余路由保持文档流）", () => {
    const sessionHtml = renderShell({
      model: buildShellModel({
        pathname: "/session/abc",
        connectionState: "online",
        hostAddress: null,
        secureContext: true,
      }),
    });
    const directoryHtml = renderShell({
      model: buildShellModel({
        pathname: "/",
        connectionState: "online",
        hostAddress: null,
        secureContext: true,
      }),
    });

    expect(sessionHtml).toContain('data-shell-fill="true"');
    expect(sessionHtml).toContain('data-shell-content-fill="true"');
    expect(directoryHtml).toContain('data-shell-fill="false"');
    expect(directoryHtml).toContain('data-shell-content-fill="false"');
    // 反例：把 contentFill 套到所有路由 → 两次渲染的根/内容容器样式完全相同（下面两条会红）。
    expect(sessionHtml.match(/<[^>]*data-shell="root"[^>]*>/)?.[0]).not.toEqual(
      directoryHtml.match(/<[^>]*data-shell="root"[^>]*>/)?.[0],
    );
    expect(sessionHtml.match(/<[^>]*data-shell-content="true"[^>]*>/)?.[0]).not.toEqual(
      directoryHtml.match(/<[^>]*data-shell-content="true"[^>]*>/)?.[0],
    );
  });
});

describe("设备胶囊：在线/离线两档", () => {
  it("在线时不出现状态短词，档位是正常", () => {
    const html = renderShell({
      model: buildShellModel({
        pathname: "/",
        connectionState: "online",
        hostAddress: "https://pc.example.ts.net",
        secureContext: true,
      }),
    });
    expect(html).toContain('data-capsule-tone="ok"');
    expect(html).toContain('data-capsule-availability="online"');
    expect(html).toContain("在线");
    expect(html).not.toContain("data-capsule-short-word");
  });

  it("阻断态不追加原因短词，也不追加离线文案：原因交给连接问题弹层", () => {
    const html = renderShell({
      model: buildShellModel({
        pathname: "/",
        connectionState: "revoked",
        hostAddress: null,
        secureContext: true,
      }),
    });
    expect(html).toContain('data-capsule-tone="danger"');
    expect(html).toContain('data-capsule-availability="offline"');
    expect(html).not.toContain("data-capsule-short-word");
    for (const word of ["已被移除", "密钥已变化", "版本不兼容", "已在别处接管", "正在重连"]) {
      expect(html, word).not.toContain(word);
    }
  });

  it("断开与重连中都是离线档，只靠圆点档位表达、不追加文案", () => {
    for (const connectionState of ["disconnected", "reconnecting"] as const) {
      const html = renderShell({
        model: buildShellModel({
          pathname: "/",
          connectionState,
          hostAddress: null,
          secureContext: true,
        }),
      });
      expect(html, connectionState).toContain('data-capsule-tone="danger"');
      expect(html, connectionState).not.toContain("data-capsule-short-word");
      expect(html, connectionState).not.toContain("正在重连");
    }
  });

  it("尚未认证的握手阶段只给中性圆点，不添加在线/离线文案", () => {
    const html = renderShell({
      model: buildShellModel({
        pathname: "/hosts",
        connectionState: "authenticating",
        hostAddress: null,
        secureContext: true,
      }),
    });
    expect(html).toContain('data-capsule-tone="warn"');
    expect(html).toContain('data-capsule-availability="unknown"');
    expect(html).not.toContain("data-capsule-short-word");
    expect(html).not.toContain(">在线<");
    expect(html).not.toContain(">离线<");
  });
});

describe("明文连接：横幅与徽标", () => {
  it("未加密托管时顶部呈现横幅、顶栏呈现徽标，且不把它说成安全连接", () => {
    const html = renderShell({
      model: buildShellModel({
        pathname: "/",
        connectionState: "online",
        hostAddress: null,
        secureContext: false,
      }),
    });
    expect(html).toContain('data-dev-banner="visible"');
    expect(html).toContain('data-dev-badge="visible"');
    expect(html).toContain("开发模式");
    expect(html).toContain("连接未加密");
    expect(html).not.toContain("安全连接");
  });

  it("加密连接下两者都不出现", () => {
    const html = renderShell({
      model: buildShellModel({
        pathname: "/",
        connectionState: "online",
        hostAddress: null,
        secureContext: true,
      }),
    });
    expect(html).not.toContain("data-dev-banner");
    expect(html).not.toContain("data-dev-badge");
  });
});

describe("帮助抽屉与提示", () => {
  it("抽屉收起时标注 closed，展开时标注 open 并给出三态主题控件（只有一项选中）", () => {
    const model = buildShellModel({
      pathname: "/",
      connectionState: "online",
      hostAddress: null,
      secureContext: true,
    });
    const closed = renderShell({ model });
    expect(closed).toContain('data-help-drawer="closed"');
    expect(closed).toContain('data-help-scrim="closed"');

    const open = renderShell({ model, helpOpen: true, themePreference: "dark" });
    expect(open).toContain('data-help-drawer="open"');
    expect(open).toContain('data-help-scrim="open"');
    for (const preference of ["system", "light", "dark"]) {
      expect(open).toContain(`data-theme-choice="${preference}"`);
    }
    expect(open).toContain("跟随系统");
    expect(countOccurrences(open, 'data-theme-choice-state="on"')).toBe(1);
  });

  it("没有提示时不渲染提示条；有提示时渲染它的文本", () => {
    const model = buildShellModel({
      pathname: "/",
      connectionState: "online",
      hostAddress: null,
      secureContext: true,
    });
    expect(renderShell({ model })).not.toContain("data-toast");

    const withToast = renderShell({ model, toastMessage: "主题已切换为「黑夜」" });
    expect(withToast).toContain('data-toast="visible"');
    expect(withToast).toContain("主题已切换为「黑夜」");
  });
});

describe("连接问题弹层：五种变体与唯一动作", () => {
  const VARIANTS = [
    { state: "disconnected", kind: "offline", title: "连接已断开", action: null },
    { state: "reconnecting", kind: "offline", title: "正在重连", action: null },
    { state: "revoked", kind: "revoked", title: "设备已被撤销", action: "重新配对" },
    { state: "incompatible", kind: "incompatible", title: "版本或能力不兼容", action: "重新加载" },
    { state: "identity_changed", kind: "identity_changed", title: "主机身份已变化", action: "重新配对" },
    { state: "replaced", kind: "replaced", title: "已被其他标签页顶替", action: "重新加载" },
  ] as const;

  it("每个变体呈现标题、码位与恰好一个动作（普通离线没有按钮）", () => {
    for (const variant of VARIANTS) {
      const html = renderShell({ model: ONLINE_MODEL, problem: problemFor(variant.state) });
      expect(html, variant.state).toContain(`data-connection-problem="${variant.kind}"`);
      expect(html, variant.state).toContain('role="dialog"');
      expect(html, variant.state).toContain('aria-modal="true"');
      expect(html, variant.state).toContain(variant.title);
      expect(html, variant.state).toContain('data-problem-close-code="4500"');
      if (variant.action === null) {
        expect(html, variant.state).not.toContain("data-problem-action=");
      } else {
        expect(countOccurrences(html, "data-problem-action="), variant.state).toBe(1);
        expect(html, variant.state).toContain(variant.action);
      }
    }
  });

  it("阻断变体给出协议错误码，普通离线不给错误码", () => {
    const expected = {
      disconnected: null,
      reconnecting: null,
      revoked: "auth.device_revoked",
      incompatible: "protocol.version_unsupported",
      identity_changed: "auth.host_mismatch",
      replaced: "connection.replaced",
    } as const;
    for (const variant of VARIANTS) {
      const html = renderShell({ model: ONLINE_MODEL, problem: problemFor(variant.state) });
      const code = expected[variant.state];
      if (code === null) {
        expect(html, variant.state).not.toContain("data-problem-error-code");
      } else {
        expect(html, variant.state).toContain(`data-problem-error-code="${code}"`);
      }
    }
  });

  it("动作与状态一一对应：repair 只给重新配对，reload 只给重新加载", () => {
    for (const state of ["revoked", "identity_changed"] as const) {
      const html = renderShell({ model: ONLINE_MODEL, problem: problemFor(state) });
      expect(html, state).toContain('data-problem-action="repair"');
      expect(html, state).not.toContain('data-problem-action="reload"');
    }
    for (const state of ["incompatible", "replaced"] as const) {
      const html = renderShell({ model: ONLINE_MODEL, problem: problemFor(state) });
      expect(html, state).toContain('data-problem-action="reload"');
      expect(html, state).not.toContain('data-problem-action="repair"');
    }
  });

  it("普通离线呈现重连进度；阻断态不呈现自动重连", () => {
    const offline = renderShell({ model: ONLINE_MODEL, problem: problemFor("reconnecting") });
    expect(offline).toContain('data-problem-reconnect="true"');
    expect(offline).toContain('data-problem-reconnect-attempts="3"');
    expect(offline).toContain("已重试 3 次");
    expect(offline).toContain("下次重试");

    for (const state of ["revoked", "incompatible", "identity_changed", "replaced"] as const) {
      const html = renderShell({ model: ONLINE_MODEL, problem: problemFor(state) });
      expect(html, state).not.toContain("data-problem-reconnect");
    }
  });

  it("技术详情默认收起但始终在 DOM 里，且不含原始关闭原因以外的秘密字段", () => {
    const html = renderShell({ model: ONLINE_MODEL, problem: problemFor("revoked") });
    expect(html).toContain('data-action="toggle-problem-detail"');
    expect(html).toContain('data-problem-detail-open="false"');
    expect(html).toContain('data-problem-detail="true"');
    expect(html).toContain("技术详情");
    expect(html).toContain("error=auth.device_revoked");
    expect(html).toContain("reason=snapshot_validation_failed");
    expect(html).not.toContain("pairingSecret");
  });

  it("弹层不可关闭：没有关闭按钮、遮罩或 dismiss 动作", () => {
    const html = renderShell({ model: ONLINE_MODEL, problem: problemFor("incompatible") });
    for (const fragment of ["close-problem", "dismiss-problem", "problem-dismiss"]) {
      expect(html, fragment).not.toContain(fragment);
    }
    // 帮助抽屉自己的关闭入口与遮罩不随弹层增加。
    expect(countOccurrences(html, 'data-help-scrim="closed"')).toBe(1);
  });

  it("弹层盖在内容区之上但顶栏仍可见，且内容仍在 DOM 里", () => {
    const html = renderShell({ model: ONLINE_MODEL, problem: problemFor("revoked") });
    expect(html).toContain('data-shell-top-bar="visible"');
    expect(html).toContain("路由内容");
    expect(html.indexOf("路由内容")).toBeLessThan(html.indexOf('data-connection-problem-overlay="true"'));
  });

  it("没有问题时不渲染弹层", () => {
    const html = renderShell({ model: ONLINE_MODEL });
    expect(html).not.toContain("data-connection-problem");
    expect(html).not.toContain("data-problem-action");
  });
});

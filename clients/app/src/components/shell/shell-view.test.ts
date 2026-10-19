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
} as const;

/** 用例要覆盖的入参；其余按缺省补齐。 */
interface ShellRenderInput {
  readonly model: ShellModel;
  readonly themePreference?: ThemePreference;
  readonly themeDark?: boolean;
  readonly reducedMotion?: boolean;
  readonly helpOpen?: boolean;
  readonly toastMessage?: string | null;
}

function renderShell(input: ShellRenderInput): string {
  const props: ShellViewProps = {
    model: input.model,
    themePreference: input.themePreference ?? "system",
    themeDark: input.themeDark ?? false,
    reducedMotion: input.reducedMotion ?? false,
    helpOpen: input.helpOpen ?? false,
    toastMessage: input.toastMessage ?? null,
    ...NOOP_HANDLERS,
  };
  return renderToStaticMarkup(createElement(ShellView, props, createElement("main", null, "路由内容")));
}

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

describe("设备胶囊：三档色与异常短词", () => {
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
    expect(html).not.toContain("data-capsule-short-word");
  });

  it("阻断态呈现阻断档与对应短词", () => {
    const html = renderShell({
      model: buildShellModel({
        pathname: "/",
        connectionState: "revoked",
        hostAddress: null,
        secureContext: true,
      }),
    });
    expect(html).toContain('data-capsule-tone="danger"');
    expect(html).toContain('data-capsule-short-word="true"');
    expect(html).toContain("已被移除");
  });

  it("离线态按阻断档呈现红色，并给出「离线」短词", () => {
    const html = renderShell({
      model: buildShellModel({
        pathname: "/",
        connectionState: "disconnected",
        hostAddress: null,
        secureContext: true,
      }),
    });
    expect(html).toContain('data-capsule-tone="danger"');
    expect(html).toContain("离线");
  });

  it("需要注意档呈现自己的短词", () => {
    const html = renderShell({
      model: buildShellModel({
        pathname: "/",
        connectionState: "reconnecting",
        hostAddress: null,
        secureContext: true,
      }),
    });
    expect(html).toContain('data-capsule-tone="warn"');
    expect(html).toContain("正在重连");
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

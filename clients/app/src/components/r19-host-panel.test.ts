/**
 * R19：Agent 目录与连接状态分两层呈现（渲染层 + 仓库层）。
 *
 * 逐条 MUST NOT 的负向用例：
 *
 * - **断连时仍列出已配置 Agent**：注入只有事件、没有快照的相反情形对照——
 *   真正要证伪的是「因为缺少连接事件而变为空列表」，因此用**重连中**这个
 *   非「已断开」的连接态，让 HTML 里出现「已断开」即可判定为把缺席当成了断开；
 * - **状态未知不等于已断开**：覆盖层缺席时每行是 `unknown`；
 * - **MUST NOT 以会话活跃程度推断**：会话全部 `running` 而覆盖层缺席 → 仍然 `unknown`
 *   （构造输入里根本没有连接事件，这条断言直接对准那条禁止）；
 * - **同名 Agent 以标识区分**：两个同名 Agent 是两行，且两个标识都在。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { HostConnectionPanel } from "./HostConnectionPanel";
import { UNKNOWN_CONNECTION_LABEL } from "../domain";
import { buildHostPanelModel } from "../features/host-panel-model";
import type { HostPanelModel } from "../features/host-panel-model";
import { eventMessage, resources, session, storeWith } from "../features/testing/fixtures";

/** 渲染主机面板。 */
function renderPanel(model: HostPanelModel): string {
  return renderToStaticMarkup(createElement(HostConnectionPanel, { model }));
}

/** 两个同名 Agent 的快照资源。 */
const AGENTS = [
  { agentId: "codex-a", displayName: "codex", default: true },
  { agentId: "codex-b", displayName: "codex", default: false },
];

describe("R19 断连时仍列出已配置 Agent", () => {
  it("连接处于重连中且没有任何覆盖层：列表非空，每行状态未知", () => {
    const model = buildHostPanelModel({
      resources: resources({ agents: AGENTS }),
      overlay: [],
      connection: "reconnecting",
      blocking: null,
      host: null,
    });

    const html = renderPanel(model);

    expect(model.agents).toHaveLength(2);
    // 反例：若因缺少连接事件而渲染成空列表，这里会是 0 项，断言变红。
    expect(html.match(/data-agent-row=/g)).toHaveLength(2);
    expect(html).toContain('data-connection-state="reconnecting"');
    // 断连时标注列表来自上次同步。
    expect(html).toContain("当前未连接");
  });

  it("覆盖层缺席：呈现状态未知，并说明为什么（而不是呈现为已断开）", () => {
    const model = buildHostPanelModel({
      resources: resources({ agents: AGENTS }),
      overlay: [],
      connection: "replaying",
      blocking: null,
      host: null,
    });

    const html = renderPanel(model);

    for (const agent of model.agents) {
      expect(agent.connection).toBe("unknown");
      expect(agent.connectionLabel).toBe(UNKNOWN_CONNECTION_LABEL);
    }
    expect(html).toContain('data-unknown-state-note="true"');
    // 反例：若把缺席当作已断开，这里会出现「已断开」的 Agent 行，断言变红。
    expect(html).not.toContain('data-agent-connection-label="disconnected"');
    expect(html).not.toContain(">已断开<");
    expect(html).toContain(UNKNOWN_CONNECTION_LABEL);
    expect(html.match(/data-agent-connection="unknown"/g)).toHaveLength(2);
  });

  it("仓库层：会话全部运行中也不得被推断成「已连接」", () => {
    const { store } = storeWith({
      resources: resources({
        agents: AGENTS,
        sessions: [
          session({ id: "aaaaaaaa-0000-4000-8000-000000000001", state: "running" }),
          session({ id: "aaaaaaaa-0000-4000-8000-000000000002", state: "running" }),
        ],
      }),
      scopes: ["session.list"],
    });

    const panel = store.hostPanel();

    // 反例：若按会话活跃度推断，这里会变成 connected，断言变红。
    expect(panel.agents.map((agent) => agent.connection)).toEqual(["unknown", "unknown"]);
    expect(panel.overlayPresent).toBe(false);
  });

  it("对照：收到 agent.connected 覆盖层后，该 Agent 才是已连接", () => {
    const { store } = storeWith({ resources: resources({ agents: AGENTS }), scopes: ["session.list"] });
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "agent.connected",
        view: { agentId: "codex-a", state: "connected" },
      }),
    });

    const panel = store.hostPanel();

    expect(panel.overlayPresent).toBe(true);
    // 覆盖层只覆盖它提到的那一个 Agent，另一个仍是未知。
    expect(panel.agents.map((agent) => agent.connection)).toEqual(["connected", "unknown"]);
  });
});

describe("R19 同名 Agent 以标识区分", () => {
  it("两个同名 Agent 是两行，两个标识都渲染出来", () => {
    const model = buildHostPanelModel({
      resources: resources({ agents: AGENTS }),
      overlay: [],
      connection: "replaying",
      blocking: null,
      host: null,
    });

    const html = renderPanel(model);

    expect(html.match(/data-agent-display-name="codex"/g)).toHaveLength(2);
    expect(html).toContain('data-agent-row="codex-a"');
    expect(html).toContain('data-agent-row="codex-b"');
    // 反例：若按展示名合并成一项，这里会只剩一行，断言变红。
    expect(html.match(/<li data-agent-row=/g)).toHaveLength(2);
  });

  it("覆盖层按标识索引：同名 Agent 的状态互不覆盖", () => {
    const model = buildHostPanelModel({
      resources: resources({ agents: AGENTS }),
      overlay: [
        { agentId: "codex-a", state: "disconnected", reason: "进程退出" },
        { agentId: "codex-b", state: "connected", reason: null },
      ],
      connection: "online",
      blocking: null,
      host: null,
    });

    expect(model.agents.map((agent) => agent.connection)).toEqual(["disconnected", "connected"]);
    expect(model.agents[0]?.disconnectReason).toBe("进程退出");
  });
});

describe("R19 未配对时不编造主机身份", () => {
  it("host 为 null 时明确说明未配对，而不是显示一个空地址", () => {
    const model = buildHostPanelModel({
      resources: resources({ agents: AGENTS }),
      overlay: [],
      connection: "unpaired",
      blocking: null,
      host: null,
    });

    const html = renderPanel(model);

    expect(html).toContain('data-host-identity="unpaired"');
    expect(html).not.toContain("data-host-identity=\"paired\"");
  });

  it("已配对时展示地址与两个标识", () => {
    const model = buildHostPanelModel({
      resources: resources({ agents: AGENTS }),
      overlay: [],
      connection: "online",
      blocking: null,
      host: { address: "https://work-pc.example.ts.net", hostId: "bdb2ec20-f98c-4d87-b789-e540d527ef87", deviceId: "2ae1c07c" },
    });

    const html = renderPanel(model);

    expect(html).toContain('data-host-identity="paired"');
    expect(html).toContain("https://work-pc.example.ts.net");
  });
});

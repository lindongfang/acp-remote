import { describe, expect, it, vi } from "vitest";
import { sessionCapacityPanel } from "../src/management/session-capacity";
import { parseSessionCapacity, parseSessionCommand } from "../src/management/session-protocol";
import { managementRequestErrorMessage } from "../src/management/error-messages";
import { parseManagementFrame } from "../src/management/protocol";

const snapshot = {
  limit: 50, occupied: 50, reserved: 2, closedPending: 1, automaticRecycling: false,
} as const;

describe("session capacity wire facts", () => {
  it("accepts only an empty authenticated command payload", () => {
    const command = { v: 1, type: "command", requestId: "AAAAAAAAAAAAAAAAAAAAAA",
      action: "session.capacity.inspect", payload: {} };
    expect(parseSessionCommand(JSON.stringify(command))).toEqual(command);
    expect(() => parseSessionCommand(JSON.stringify({ ...command, payload: { limit: 51 } }))).toThrow();
  });
  it("retains reservations and uncoordinated closures including upgrade overflow", () => {
    expect(parseSessionCapacity(snapshot)).toEqual(snapshot);
    expect(parseSessionCapacity({ ...snapshot, occupied: 60 }).occupied).toBe(60);
    expect(parseSessionCapacity({ ...snapshot, automaticRecycling: true }).automaticRecycling).toBe(true);
  });
  it.each([
    { ...snapshot, automaticRecycling: "true" },
    { ...snapshot, automaticRecycling: null },
    { ...snapshot, occupied: -1 },
    { ...snapshot, reserved: 2.5 },
    { ...snapshot, occupied: "50" },
    { ...snapshot, occupied: 0 },
    { ...snapshot, limit: 0 },
    { ...snapshot, secret: "private" },
    { ...snapshot, closedPending: 0x1_0000_0000 },
  ])("rejects unsupported or inconsistent resource facts %#", (value) => {
    expect(() => parseSessionCapacity(value)).toThrow();
  });
  it.each(["session_capacity_full", "session_tree_unverified"] as const)(
    "preserves %s from the server and explains the permitted next step", (code) => {
      const response = { v: 1, type: "result", requestId: "AAAAAAAAAAAAAAAAAAAAAA",
        ok: false, result: null, error: { code } };
      expect(parseManagementFrame(JSON.stringify(response))).toEqual(response);
      expect(managementRequestErrorMessage(code)).toContain(
        code === "session_capacity_full" ? "关闭未确认时仍占用名额" : "主执行树归属");
    },
  );
});

class FakeElement {
  className = "";
  textContent = "";
  type = "";
  disabled = false;
  readonly children: FakeElement[] = [];
  readonly attributes = new Map<string, string>();
  readonly events = new Map<string, () => void>();
  constructor(readonly tag: string) {}
  append(...values: FakeElement[]): void { this.children.push(...values); }
  setAttribute(name: string, value: string): void { this.attributes.set(name, value); }
  addEventListener(name: string, listener: () => void): void { this.events.set(name, listener); }
}
const owner = { createElement: (tag: string) => new FakeElement(tag) } as unknown as Document;

describe("capacity panel", () => {
  it("renders node resource facts and one explicitly initiated refresh", () => {
    const refresh = vi.fn();
    const panel = sessionCapacityPanel(owner, snapshot, true, refresh) as unknown as FakeElement;
    const [heading, facts, help, button] = panel.children;
    expect(heading?.textContent).toBe("原生会话名额");
    expect(facts?.textContent).toContain("已占用 50 / 50");
    expect(facts?.textContent).toContain("关闭待协调 1");
    expect(facts?.attributes.get("aria-live")).toBe("polite");
    expect(help?.textContent).toContain("自动回收尚未启用");
    expect(help?.textContent).toContain("原生会话释放");
    expect(refresh).not.toHaveBeenCalled();
    button?.events.get("click")?.();
    expect(button?.disabled).toBe(true);
    expect(refresh).toHaveBeenCalledWith(button);
  });
  it("does not invent counts before reading, and disables disconnected refresh", () => {
    const panel = sessionCapacityPanel(owner, null, false, vi.fn()) as unknown as FakeElement;
    expect(panel.children[1]?.textContent).toContain("尚未读取");
    expect(panel.children[2]?.textContent).toContain("自动回收状态尚未读取");
    expect(panel.children[3]?.disabled).toBe(true);
  });
  it("shows enabled request policy without treating a request as released capacity", () => {
    const panel = sessionCapacityPanel(owner, { ...snapshot, automaticRecycling: true },
      true, vi.fn()) as unknown as FakeElement;
    expect(panel.children[1]?.textContent).toContain("已占用 50 / 50");
    expect(panel.children[2]?.textContent).toContain("请求 Harness 关闭主会话");
    expect(panel.children[2]?.textContent).toContain("发起请求不代表已经关闭");
    expect(panel.children[2]?.textContent).not.toContain("尚未启用");
  });
});

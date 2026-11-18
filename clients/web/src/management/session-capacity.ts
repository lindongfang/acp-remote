import type { SessionCapacity } from "./session-protocol";

export function sessionCapacityPanel(
  owner: Document,
  capacity: SessionCapacity | null,
  connected: boolean,
  refresh: (button: HTMLButtonElement) => void,
): HTMLElement {
  const section = owner.createElement("section");
  section.className = "session-composer";
  const heading = owner.createElement("h3");
  heading.className = "section-title";
  heading.textContent = "原生会话名额";
  const facts = owner.createElement("p");
  facts.setAttribute("role", "status");
  facts.setAttribute("aria-live", "polite");
  facts.textContent = capacity === null
    ? "尚未读取节点名额，请刷新。"
    : `节点快照：已占用 ${capacity.occupied} / ${capacity.limit}；` +
      `其中创建预留 ${capacity.reserved}，关闭待协调 ${capacity.closedPending}。`;
  const help = owner.createElement("p");
  help.className = "field-help";
  const policy = capacity === null
    ? "自动回收状态尚未读取。"
    : capacity.automaticRecycling
      ? "节点会根据已知活动，在空闲超时或名额不足时请求 Harness 关闭主会话。"
      : "自动回收尚未启用。";
  help.textContent = policy + "发起请求不代表已经关闭；预留及关闭结果未协调的会话继续占额。" +
    "名额不足时可使用下方“原生会话释放”，再刷新名额。页面及缓存读取不会保活。";
  const button = owner.createElement("button");
  button.type = "button";
  button.className = "secondary-button";
  button.textContent = "刷新会话名额";
  button.disabled = !connected;
  button.addEventListener("click", () => {
    button.disabled = true;
    refresh(button);
  });
  section.append(heading, facts, help, button);
  return section;
}

/**
 * 工作流门禁：要求「会改文件」的子 Agent 必须在隔离工作区里运行。
 *
 * 为什么需要它（这条规则不能只写在文档里）：
 *   `task.isolation.enabled: true` 只是让 `isolated` 字段出现在 task 参数里，**它不改变默认值**。
 *   默认不传等于不隔离，子 Agent 会用父会话的 cwd，直接改写主检出——实测中一次漏传就让探针文件
 *   出现在主检出。而按 `docs/WORKFLOW.md` §4，工作包必须先在隔离工作区产出 patch，再由主 Agent
 *   串行 `git apply`，否则既没有 `git apply --check` 这道校验，也会与开发者的在途修改相撞。
 *
 * 豁免：只读角色不写文件，为它们复制 15M 工作区只是浪费。判据是角色本身能否写文件，
 * 不是「这次有没有写」——`sonic` 有全量工具，因此不豁免。
 *
 * 类型说明：本地没有 `@oh-my-pi/pi-coding-agent` 的包（omp 是独立安装的宿主），
 * 因此这里用结构化的最小类型描述我们用到的两个 API，而不是 import 宿主的类型。
 */

/** 只读角色：不提供写文件的工具，允许不加 isolated。静态词表用 Record。 */
const READ_ONLY_AGENTS: Record<string, true> = {
  reviewer: true,
  "security-reviewer": true,
  scout: true,
};

const READ_ONLY_NAMES = Object.keys(READ_ONLY_AGENTS).join(" / ");

/** 我们用到的 `tool_call` 事件子集。 */
interface TaskToolCallEvent {
  toolName: string;
  input: unknown;
}

/** 我们用到的 `ExtensionAPI` 子集。 */
interface MinimalExtensionApi {
  on(event: "tool_call", handler: (event: TaskToolCallEvent) => unknown): void;
}

interface TaskItem {
  agent?: unknown;
  isolated?: unknown;
}

export default function (pi: MinimalExtensionApi): void {
  pi.on("tool_call", (event) => {
    if (event.toolName !== "task") return;
    const input = event.input as { tasks?: TaskItem[] } | undefined;
    const items = Array.isArray(input?.tasks) ? input.tasks : [];
    if (items.length === 0) return;

    const offenders = items
      .map((item, index) => ({
        index,
        agent: typeof item?.agent === "string" ? item.agent : "(default)",
        isolated: item?.isolated,
      }))
      .filter((entry) => READ_ONLY_AGENTS[entry.agent] !== true && entry.isolated !== true);

    if (offenders.length === 0) return;

    const named = offenders.map((o) => `#${o.index} (agent: ${o.agent})`).join(", ");
    return {
      block: true,
      reason:
        `task 调用被工作流门禁拦截：${named} 没有 isolated: true。`
        + "本仓库要求会写文件的工作包在隔离工作区运行，改动以 <id>.patch 交回、由主 Agent 串行 git apply"
        + "（判据见 docs/WORKFLOW.md §4、配置见 .omp/config.yml 的 task.isolation）。"
        + `只读角色（${READ_ONLY_NAMES}）不需要该字段。`,
    };
  });
}

/**
 * 对话页的**布局**与**左栏**浏览器级验证（宽屏三栏、独立滚动、窄屏三档、紧凑会话行、两个目录入口）。
 *
 * ## 为什么需要真实外壳
 *
 * 三栏的高度与滚动由共享外壳的 `contentFill`（`100dvh`）决定：只有把它挂上，中栏的
 * `ScrollView` 才拿到一个有限高度、滚动才发生在列内而不是文档上。`support.tsx` 的
 * `HarnessLayout` 因此与 `app/_layout.tsx` 同构（`Shell > Stack`），本检查集依赖这一点——
 * 只挂 `Stack` 时 `scrollHeight > clientHeight` 永远不会成立，下面的独立滚动断言会直接超时红。
 *
 * ## 判别力（把实现反着写，哪一条会红）
 *
 * - **三栏并排**：断言三栏矩形都非空且按 左 < 中 < 右 排列；退回单栏/纵向堆叠时顺序断言红。
 * - **输入区在中栏**：断言输入区位于 `data-conversation-column="timeline"` 内、且**不**在
 *   时间线滚动容器内（它吸在中栏底部，不随时间线滚走）。把输入区挪回三栏之外时第一条红。
 * - **独立滚动**：把中栏滚到底，左右两栏的 `scrollTop` 必须保持 0；退回整页滚动时中栏自己的
 *   `scrollTop` 不会变（等待超时红），左右栏也不会各自滚动。
 * - **窄屏没有「全部」**：宽度降到断点以下后，切换控件的取值集合必须恰好是
 *   `timeline / sessions / files`，且当前只呈现中栏。多出「全部」或三栏仍并列都会红。
 * - **紧凑行**：左栏每一行是 `compact` 形态，只有圆点/标题/状态词/时间；Agent 名与来源标注
 *   不得出现在行内。把目录页的五字段行原样搬过来时这两条负向断言红。
 * - **两个目录入口**：`‹ 目录` 回目录列表（路径 `/`），`全部 ›` 进本目录详情（路径
 *   `/dir/<id>`）。两个入口若指向同一处，其中一条路径断言红。
 */

import {
  DIRECTORY_ID_ALPHA,
  DIRS_ROUTE,
  SESSION_ALPHA,
  WORKSPACES_WITH_PATHS,
  dirRoute,
  readMessage,
  readPage,
  sessionRoute,
  sessionSeries,
  sessionSummary,
  seriesSessionId,
} from "../fixtures";
import {
  click,
  expectEqual,
  expectThat,
  query,
  queryAll,
  textOf,
  waitFor,
} from "../support";
import type { Check } from "../support";
import type { SessionSummary } from "../../../src/protocol";
import { currentPath } from "../router-stub";
import { overrideViewportWidth } from "./r15-directory-detail";
import { sessionScenario, withSessionPage } from "./conversation-timeline";

/** 让中栏真的能滚起来的一串 Agent 消息（每条都足够长，12 条即可超过 600px 的可用高度）。 */
const LONG_MESSAGE_LINE = "这一段用来把时间线撑长，让中栏出现自己的滚动区间。";

function longMessages(): readonly Record<string, unknown>[] {
  const text = Array.from({ length: 24 }, (_, line) => `第 ${String(line)} 行：${LONG_MESSAGE_LINE}`).join("\n");
  return [
    readMessage({
      messageId: "b0000000-0000-4000-8000-000000000001",
      sessionId: SESSION_ALPHA,
      role: "user",
      text: "先看下这个项目怎么跑起来",
      createdAt: "2026-10-01T00:09:00.000Z",
    }),
    ...Array.from({ length: 12 }, (_, index) =>
      readMessage({
        messageId: `b0000000-0000-4000-8000-00000000${String(index + 10).padStart(4, "0")}`,
        sessionId: SESSION_ALPHA,
        role: "agent",
        text,
        createdAt: `2026-10-01T00:${String(10 + index).padStart(2, "0")}:00.000Z`,
      }),
    ),
  ];
}

const LAST_LONG_MESSAGE = "b0000000-0000-4000-8000-000000000021";

/**
 * 18 条同目录会话：够左栏分页（15 / 18），且「等你回复」那条的时间足够新、一定落在第一批
 * ——紧凑行的状态词与圆点都要在它身上读。当前会话的时间最新，因此也一定在第一批里。
 */
const SIBLING_WAITING = seriesSessionId("cccccccc", 0);

function sidebarSessions(): readonly SessionSummary[] {
  const times = Array.from({ length: 17 }, (_, index) =>
    new Date(Date.UTC(2026, 9, 1, 0, 20 - index)).toISOString(),
  );
  const titles = ["登录流程", ...Array.from({ length: 16 }, (_, index) => `任务 ${String(index + 2)}`)];
  const states: readonly SessionSummary["state"][] = [
    "pending",
    ...Array.from({ length: 16 }, () => "idle" as const),
  ];
  return [
    ...sessionSeries({ directoryId: DIRECTORY_ID_ALPHA, idPrefix: "cccccccc", titles, at: times, states }),
    // 当前会话的时间最新：它必须落在第一批 15 条里，否则「选中项」那条断言等的是一个
    // 尚未分页出来的行。
    sessionSummary({
      id: SESSION_ALPHA,
      directoryId: DIRECTORY_ID_ALPHA,
      title: "部署脚本",
      state: "running",
      updatedAt: "2026-10-01T00:30:00.000Z",
    }),
  ];
}

/** 三栏的滚动容器各一个；缺任何一个都直接抛红。 */
function columnScroller(container: HTMLElement, column: "sessions" | "timeline" | "files"): HTMLElement {
  const found = query(container, `[data-conversation-scroll="${column}"]`);
  if (!(found instanceof HTMLElement)) {
    throw new Error(`找不到 ${column} 栏的滚动容器`);
  }
  return found;
}

const NARROW_OPTIONS = ["timeline", "sessions", "files"] as const;

/** 切换控件的取值与文案（按 DOM 顺序）。 */
function segmentOptions(container: HTMLElement): { readonly values: readonly string[]; readonly labels: readonly string[] } {
  const group = query(container, '[data-segment="conversation-secondary-view"]');
  const options = queryAll(group, "[data-segment-option]");
  return {
    values: options.map((node) => node.getAttribute("data-segment-option") ?? ""),
    labels: options.map((node) => textOf(node)),
  };
}

/** 失败时的祖先高度链：定位「哪一层没有把视口高度传下来」。 */
function heightChain(element: Element): string {
  const parts: string[] = [];
  let node: Element | null = element;
  while (node !== null && node !== document.body) {
    const style = getComputedStyle(node);
    parts.push(
      `${node.tagName.toLowerCase()}${node.getAttribute("data-conversation-column") === null ? "" : `[col=${node.getAttribute("data-conversation-column") ?? ""}]`}` +
        ` h=${style.height} ch=${String((node as HTMLElement).clientHeight)} display=${style.display} flex=${style.flexGrow}/${style.flexShrink}/${style.flexBasis}`,
    );
    node = node.parentElement;
  }
  return parts.join(" <- ");
}

const checks: readonly Check[] = [
  {
    name: "CP-11 宽屏三栏：左中右并排、输入区在中栏、时间线独立滚动不牵动左右栏",
    async run() {
      // 浏览器默认视口（800×600）已高于 `breakpoints.wide`（720），因此这里就是宽屏分支。
      await withSessionPage(
        sessionScenario({
          workspaces: WORKSPACES_WITH_PATHS,
          sessions: sidebarSessions(),
          readResult: (sessionId) => readPage({ sessionId, messages: longMessages() }),
        }),
        (container) =>
          query(container, `[data-message-id="${LAST_LONG_MESSAGE}"]`) !== null &&
          query(container, '[data-conversation-scroll="sessions"]') !== null &&
          query(container, '[data-conversation-scroll="files"]') !== null,
        "三栏与最后一条消息渲染",
        async (mounted) => {
          const container = mounted.container;

          // 外壳按视口高度固定：这是「三栏各自滚动」的前提，缺了它下面的滚动断言会超时。
          expectThat(
            query(container, '[data-shell-content-fill="true"]') !== null,
            "对话路由必须让共享外壳进入按视口高度固定的形态",
          );

          const sessions = columnScroller(container, "sessions");
          const timeline = columnScroller(container, "timeline");
          const files = columnScroller(container, "files");
          for (const [name, scroller] of [
            ["sessions", sessions],
            ["timeline", timeline],
            ["files", files],
          ] as const) {
            expectThat(
              getComputedStyle(scroller).display !== "none",
              `${name} 栏必须可见（不是 display:none）`,
            );
            expectThat(
              scroller.clientHeight > 0,
              `${name} 栏必须有自己的可视高度（${heightChain(scroller)}）`,
            );
          }

          // 从左到右的真实位置：不是只有三个节点，而是它们确实并排。
          const sessionsRect = sessions.getBoundingClientRect();
          const timelineRect = timeline.getBoundingClientRect();
          const filesRect = files.getBoundingClientRect();
          expectThat(
            sessionsRect.left < timelineRect.left && timelineRect.right <= filesRect.left,
            `三栏必须并排：sessions=${String(sessionsRect.left)}，timeline=${String(timelineRect.left)}，files=${String(filesRect.left)}`,
          );

          // 宽屏直接呈现三栏，不增加顶部次级视图切换。
          expectEqual(segmentOptions(container).values, [], "宽屏不显示次级视图切换");

          // 输入区在中栏里，但不在时间线的滚动容器里（吸底，不随时间线滚走）。
          const timelineColumn = query(container, '[data-conversation-column="timeline"]');
          expectThat(timelineColumn !== null, "必须有中栏");
          expectThat(
            query(timelineColumn, "[data-composer]") !== null,
            "输入区必须落在中栏内",
          );
          expectEqual(
            query(timeline, "[data-composer]"),
            null,
            "输入区不得位于时间线滚动容器内",
          );
          // 中栏的装配顺序：会话头 → 时间线滚动区 → 输入区。
          const header = query(timelineColumn, "[data-session-header]");
          const composerDock = query(timelineColumn, "[data-composer-dock]");
          expectThat(header !== null && composerDock !== null, "中栏必须有会话头与输入区");
          expectThat(
            (header.compareDocumentPosition(timeline) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0,
            "时间线滚动区必须排在会话头之后",
          );
          expectThat(
            (timeline.compareDocumentPosition(composerDock) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0,
            "输入区必须排在时间线滚动区之后",
          );

          // 独立滚动：中栏真的能滚；滚动它不改变左右栏。
          await waitFor(
            () => timeline.scrollHeight > timeline.clientHeight,
            `中栏时间线必须超过可视高度（scrollHeight=${String(timeline.scrollHeight)}, clientHeight=${String(timeline.clientHeight)}）`,
          );
          const before = { sessions: sessions.scrollTop, files: files.scrollTop };
          timeline.scrollTop = Math.min(240, timeline.scrollHeight - timeline.clientHeight);
          timeline.dispatchEvent(new Event("scroll"));
          await waitFor(() => timeline.scrollTop > 0, "中栏滚动位置必须真的离开顶部");
          expectEqual(sessions.scrollTop, before.sessions, "滚动中栏不得改变左栏的滚动位置");
          expectEqual(files.scrollTop, before.files, "滚动中栏不得改变右栏的滚动位置");
        },
      );
    },
  },
  {
    name: "CP-12 左栏紧凑行只有圆点/标题/状态词/时间；「‹ 目录」回列表；分页 15 / 18",
    async run() {
      await withSessionPage(
        sessionScenario({
          workspaces: WORKSPACES_WITH_PATHS,
          sessions: sidebarSessions(),
          // 三个真实路径都必须注册：`‹ 目录` → `/`、`全部 ›` → `/dir/<id>`、再回到会话页。
          extraRoutes: [DIRS_ROUTE, dirRoute(DIRECTORY_ID_ALPHA), sessionRoute(SESSION_ALPHA)],
        }),
        (container) => query(container, "[data-conversation-sidebar]") !== null,
        "左栏渲染",
        async (mounted) => {
          const container = mounted.container;

          // 拖动柄支持键盘调整，避免只有鼠标用户能改变栏宽。
          for (const side of ["left", "right"] as const) {
            const resizer = query(container, `[data-column-resizer="${side}"]`);
            expectThat(resizer !== null, "宽屏提供左右两道宽度调整柄");
            const before = Number(resizer?.getAttribute("aria-valuenow"));
            resizer?.dispatchEvent(new KeyboardEvent("keydown", {
              key: side === "left" ? "ArrowLeft" : "ArrowRight", bubbles: true,
            }));
            await waitFor(() => Number(resizer?.getAttribute("aria-valuenow")) < before, "键盘缩窄侧栏");
          }

          // 分页：18 条里先呈现 15 条，「已显示 N / M」与继续加载同时在场。
          expectEqual(
            query(container, "[data-sidebar-shown]")?.getAttribute("data-sidebar-shown"),
            "15",
            "左栏先呈现一批",
          );
          expectEqual(
            query(container, "[data-sidebar-total]")?.getAttribute("data-sidebar-total"),
            "18",
            "左栏的分母是本目录全部会话",
          );
          const pager = query(container, '[data-sidebar-pager="true"]');
          expectEqual(pager?.getAttribute("data-sidebar-page"), "1", "初始第一页");
          expectEqual(pager?.getAttribute("data-sidebar-page-count"), "2", "18 条会话分两页");
          expectEqual(query(container, '[data-action="load-more-sidebar"]'), null, "不再呈现加载更多");
          click(query(pager, '[data-action="page-next"]'), "左栏下一页");
          await waitFor(
            () => query(container, '[data-sidebar-pager]')?.getAttribute("data-sidebar-page") === "2",
            "左栏切换到第二页",
          );
          expectEqual(queryAll(container, '[data-sidebar-session]').length, 3, "第二页只呈现剩余三条");
          click(query(container, '[data-sidebar-pager] [data-action="page-prev"]'), "左栏上一页");
          await waitFor(
            () => query(container, '[data-sidebar-pager]')?.getAttribute("data-sidebar-page") === "1",
            "返回第一页",
          );

          // 紧凑行的字段集合：圆点 / 标题 / 状态词 / 时间；Agent 名与来源标注不出现。
          const waitingRow = query(container, `[data-sidebar-session="${SIBLING_WAITING}"]`);
          expectThat(waitingRow !== null, "等你回复的那条同目录会话必须出现在左栏");
          expectThat(
            query(waitingRow, '[data-session-row-variant="compact"]') !== null,
            "左栏必须使用紧凑行形态",
          );
          // 状态圆点与状态词：同一个 `data-session-state` 取值出现两次——圆点没有文本，
          // 状态词是「等你回复」。少了圆点（或把它缩成一个纯色块以外的表达）时集合会变。
          expectEqual(
            queryAll(waitingRow, '[data-session-state="pending"]').map((node) => textOf(node)),
            [""],
            "紧凑行必须有状态圆点与状态词",
          );
          expectEqual(
            textOf(query(waitingRow, "[data-session-row-title]")),
            "登录流程",
            "紧凑行必须有标题",
          );
          const time = query(waitingRow, "[data-session-updated-at]");
          expectEqual(
            time?.getAttribute("data-session-updated-at"),
            "2026-10-01T00:20:00.000Z",
            "紧凑行的时间必须来自会话自身的时间",
          );
          expectThat(textOf(time).length > 0, "紧凑行必须有相对时间文案");
          // 负向：紧凑形态不再有 Agent 名与来源标注（那两列只属于目录页的五字段行）。
          expectEqual(query(waitingRow, "[data-session-agent]"), null, "紧凑行不得呈现 Agent 名");
          expectEqual(query(waitingRow, "[data-origin-offline]"), null, "紧凑行不得呈现来源标注");
          expectThat(!textOf(waitingRow).includes("codex"), "紧凑行不得出现 Agent 名文本");

          // 当前会话被标记为选中。
          expectEqual(
            query(container, `[data-sidebar-session="${SESSION_ALPHA}"]`)?.getAttribute(
              "data-sidebar-session-selected",
            ),
            "true",
            "当前会话在左栏被标记为选中",
          );

          // 两个目录入口是分开的控件。
          const backToList = query(container, '[data-action="open-directory-list"]');
          expectThat(textOf(backToList).includes("‹ 目录"), "「‹ 目录」必须回到目录列表");
          expectEqual(
            query(container, '[data-action="open-directory"]'),
            null,
            "对话页左栏只保留返回目录列表的入口，不重复目录详情入口",
          );
          click(query(container, '[data-action="open-directory-list"]'), "回到目录列表");
          await waitFor(() => currentPath() === "/", "点击「‹ 目录」后回到目录列表");
          await waitFor(
            () => query(container, '[data-directory-list="directories"]') !== null,
            "目录列表渲染",
          );
        },
      );
    },
  },
  {
    name: "CP-13 窄屏只提供对话/会话/变更文件三档，没有「全部」；一次只呈现一栏",
    async run() {
      const restore = overrideViewportWidth(600);
      try {
        await withSessionPage(
          sessionScenario({
            workspaces: WORKSPACES_WITH_PATHS,
            sessions: [
              sessionSummary({ id: SESSION_ALPHA, directoryId: DIRECTORY_ID_ALPHA, state: "running" }),
            ],
          }),
          (container) => query(container, '[data-segment="conversation-secondary-view"]') !== null,
          "窄屏的次级视图切换控件",
          async (mounted) => {
            const container = mounted.container;
            const options = segmentOptions(container);
            expectEqual(options.values, NARROW_OPTIONS, "窄屏的取值里不得出现「全部」");
            expectEqual(options.labels, ["对话", "会话", "文件"], "窄屏的三档文案");

            // 缺省是「对话」：只呈现中栏，左右两栏被隐藏（但仍在 DOM 里，切回去即可见）。
            expectEqual(
              query(container, '[data-segment-option="timeline"]')?.getAttribute("data-segment-selected"),
              "true",
              "窄屏缺省选中「对话」",
            );
            const timelineColumn = query(container, '[data-conversation-column="timeline"]');
            const sessionsColumn = query(container, '[data-conversation-column="sessions"]');
            const filesColumn = query(container, '[data-conversation-column="files"]');
            expectThat(timelineColumn !== null, "中栏必须存在");
            expectThat(sessionsColumn !== null, "左栏必须仍在 DOM 里");
            expectThat(filesColumn !== null, "右栏必须仍在 DOM 里");
            expectThat(
              getComputedStyle(timelineColumn).display !== "none",
              "窄屏缺省必须呈现中栏",
            );
            expectEqual(getComputedStyle(sessionsColumn).display, "none", "窄屏左栏必须隐藏");
            expectEqual(getComputedStyle(filesColumn).display, "none", "窄屏右栏必须隐藏");
            expectThat(query(timelineColumn, "[data-composer]") !== null, "窄屏的输入区仍在中栏内");
          },
        );
      } finally {
        restore();
      }
    },
  },
];

export const CONVERSATION_LAYOUT_CHECKS = checks;

/**
 * 文件栏两来源、中间只读查看与关闭恢复的浏览器级验证。
 *
 * ## 假主机提供的是什么
 *
 * 这里用页内假主机（`../fake-host.ts`）应答四条真实查询，客户端走的是**生产**的 Sync 客户端、
 * `ClientStore` 与页面组件；被替换的只有传输与磁盘。因此这些检查证明的是：
 *
 * - 两个来源的判定与渲染路径（默认全部文件、目录树、忽略开关、Git 状态、根外链接）；
 * - 会话改动只在时间线呈现（结构化差异 + 诊断入口），文件栏没有会话变更清单；
 * - 中间查看区的互斥替换、关闭恢复、窄屏返回与跨断点保留；
 * - 不可信内容（脚本/远端图片/SVG）不成为活动内容，图片只从 Blob URL 加载；
 * - 下载经平台端口完成、可取消且 Blob URL 可撤销。
 *
 * 真实 Daemon 上的同一组行为由 `crates/app/src/daemon/sync_ingress_tests.rs` 与 Main E2E 证明；
 * 本文件的证据只在「假主机 + 真实组件树」这一层，不替代真实链路证据。
 */

import { encodeBase64Url } from "../../../src/sync-client";
import {
  AGENTS,
  DIRECTORY_ID_ALPHA,
  SESSION_ALPHA,
  WORKSPACES,
  historyItem,
  readMessage,
  readPage,
  sessionSummary,
  wireEvent,
} from "../fixtures";
import {
  CheckFailure,
  click,
  expectEqual,
  expectThat,
  query,
  queryAll,
  textOf,
  waitFor,
} from "../support";
import type { Check, ScenarioOptions } from "../support";
import { overrideViewportWidth } from "./r15-directory-detail";
import { commandBody, sessionScenario, withSessionPage } from "./conversation-timeline";

/** 一个固定的改动事件视图（带结构化差异行）。 */
function changeView(input: {
  readonly changeId: string;
  readonly path: string;
  readonly kind?: string;
  readonly added?: string;
  readonly deleted?: string;
  readonly diffLines?: readonly { readonly kind: string; readonly text: string }[];
  readonly summary?: string;
}): Record<string, unknown> {
  return {
    changeId: input.changeId,
    kind: input.kind ?? "update",
    displayPath: input.path,
    summary: input.summary ?? "调整入口",
    ...(input.added === undefined ? {} : { addedLines: input.added }),
    ...(input.deleted === undefined ? {} : { deletedLines: input.deleted }),
    outsideWorkspace: false,
    ...(input.diffLines === undefined ? {} : { diffLines: input.diffLines }),
  };
}

const TEXT_FILE = [
  "# 说明",
  "",
  "<script>window.__pwned = true</script>",
  '<img src="https://evil.example/tracker.png" onerror="window.__pwned = true">',
  "<svg onload=\"window.__pwned = true\"></svg>",
  "",
  "普通文本。",
].join("\n");

/** 1×1 PNG（实际 magic 是 PNG，因此会走 Blob 图片预览）。 */
const PNG_BYTES = Uint8Array.from(
  atob("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="),
  (char) => char.charCodeAt(0),
);

/** 一次性二进制字节：4 KiB 的递增模式。 */
const BINARY_BYTES = Uint8Array.from({ length: 4096 }, (_, index) => index % 251);

/** 往对话输入区写一段草稿（走原生 setter，React 的受控值才会更新）。 */
function setComposerDraft(container: HTMLElement, value: string): void {
  const node = query(container, "[data-composer-input]");
  if (!(node instanceof HTMLTextAreaElement)) throw new CheckFailure("对话输入区必须是 textarea");
  const native = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value");
  native?.set?.call(node, value);
  node.dispatchEvent(new Event("input", { bubbles: true }));
}

function bytesOf(path: string): { readonly bytes: Uint8Array; readonly kind: string } {
  if (path === "README.md") {
    // 足够长以证明查看器自身可滚动（长文本/长 diff/多次记录都不能被裁掉）。
    const bytes = new TextEncoder().encode(TEXT_FILE.repeat(80));
    return { bytes, kind: "text" };
  }
  if (path === "src/app.ts") {
    return { bytes: new TextEncoder().encode("export const app = 42;\n"), kind: "text" };
  }
  if (path === "evil.svg") {
    return { bytes: new TextEncoder().encode('<svg xmlns="http://www.w3.org/2000/svg" onload="window.__pwned=true"></svg>'), kind: "text" };
  }
  if (path === "assets/pixel.png") return { bytes: PNG_BYTES, kind: "image" };
  if (path === "bundle.bin") return { bytes: BINARY_BYTES, kind: "binary" };
  return { bytes: new Uint8Array(0), kind: "binary" };
}

/** 假主机的四条查询应答：数据是脚本化的，但形状必须过客户端的严格解码。 */
function filesResult(command: string, _sessionId: string, payload: Record<string, unknown>): Record<string, unknown> {
  switch (command) {
    case "session.files.list": {
      const path = typeof payload["path"] === "string" ? payload["path"] : "";
      const hideIgnored = payload["hideIgnored"] === true;
      if (path === "") {
        const entries = [
          { path: "README.md", name: "README.md", kind: "file", openable: true },
          { path: "src", name: "src", kind: "directory", openable: true },
          { path: "assets", name: "assets", kind: "directory", openable: true },
          { path: "bundle.bin", name: "bundle.bin", kind: "file", openable: true },
          { path: "evil.svg", name: "evil.svg", kind: "file", openable: true },
          { path: "escape", name: "escape", kind: "link", openable: false, restricted: "outside_root" },
          ...(hideIgnored ? [] : [{ path: "debug.log", name: "debug.log", kind: "file", openable: true }]),
        ];
        return { path, gitState: "available", entries };
      }
      if (path === "src") {
        return {
          path,
          gitState: "available",
          entries: [{ path: "src/app.ts", name: "app.ts", kind: "file", openable: true }],
        };
      }
      if (path === "assets") {
        return {
          path,
          gitState: "available",
          entries: [{ path: "assets/pixel.png", name: "pixel.png", kind: "file", openable: true }],
        };
      }
      return { path, gitState: "available", entries: [] };
    }
    case "session.files.read": {
      const path = typeof payload["path"] === "string" ? payload["path"] : "";
      const offset = Number(typeof payload["offset"] === "string" ? payload["offset"] : "0");
      const { bytes, kind } = bytesOf(path);
      const chunkSize = 32 * 1024;
      const chunk = bytes.slice(offset, offset + chunkSize);
      return {
        path,
        kind,
        sizeBytes: String(bytes.length),
        version: "version-1",
        offset: String(offset),
        dataBase64: encodeBase64Url(chunk),
        eof: offset + chunk.length >= bytes.length,
        previewTruncated: false,
      };
    }
    case "session.git.status":
      return {
        gitState: "available",
        entries: [
          { path: "src/app.ts", status: "modified", conflicted: false, binary: false },
          { path: "src/new.ts", status: "added", conflicted: false, binary: false },
          { path: "assets/pixel.png", status: "untracked", conflicted: false, binary: true },
          { path: "src/conflict.ts", status: "modified", conflicted: true, binary: false },
          { path: "src/old.ts", status: "deleted", conflicted: false, binary: false },
        ],
      };
    case "session.git.diff": {
      const path = typeof payload["path"] === "string" ? payload["path"] : "";
      if (path === "assets/pixel.png") {
        return { path, base: "head", kind: "binary", truncated: false };
      }
      if (path === "src/conflict.ts") {
        return {
          path,
          base: "head",
          kind: "conflict",
          diffLines: ["<<<<<<< HEAD", "const a = 1;", "=======", "const a = 2;", ">>>>>>> work"],
          truncated: false,
        };
      }
      return {
        path,
        base: "head",
        kind: "text",
        diffLines: ["@@ -1,2 +1,2 @@", " const a = 1;", "-const b = 2;", "+const b = 3;"],
        truncated: false,
      };
    }
    default:
      return {};
  }
}

/** 分页场景：第一页只给一个文件并带 `next`，第二页再给一个。 */
function pagedFilesResult(command: string, sessionId: string, payload: Record<string, unknown>): Record<string, unknown> {
  if (command !== "session.files.list") return filesResult(command, sessionId, payload);
  const path = typeof payload["path"] === "string" ? payload["path"] : "";
  if (path !== "") return filesResult(command, sessionId, payload);
  if (payload["after"] === "page-2") {
    return {
      path,
      gitState: "available",
      entries: [{ path: "second.txt", name: "second.txt", kind: "file", openable: true }],
    };
  }
  return {
    path,
    gitState: "available",
    entries: [{ path: "first.txt", name: "first.txt", kind: "file", openable: true }],
    next: "page-2",
  };
}

/** 非 Git 目录：状态查询是成功结果里的显式状态。 */
function nonRepositoryResult(command: string, sessionId: string, payload: Record<string, unknown>): Record<string, unknown> {
  if (command === "session.git.status") return { gitState: "not_repository", entries: [] };
  return filesResult(command, sessionId, payload);
}

/** 一条带文件路径的 `file.changed` 事件。 */
function changedEvent(sequence: number, changeId: string, path: string, extra: Record<string, unknown> = {}): Record<string, unknown> {
  return wireEvent({
    globalSequence: sequence,
    eventId: `eeeeeeee-0000-4000-8000-${String(sequence).padStart(12, "0")}`,
    sessionId: SESSION_ALPHA,
    eventType: "file.changed",
    view: changeView({ changeId, path, ...extra }),
    createdAt: `2026-10-01T00:00:${String(sequence % 60).padStart(2, "0")}.000Z`,
  });
}

function sessionOptions(overrides: Partial<ScenarioOptions> = {}): ScenarioOptions {
  return sessionScenario({
    workspaces: WORKSPACES,
    sessions: [sessionSummary({ id: SESSION_ALPHA, directoryId: DIRECTORY_ID_ALPHA, state: "running" })],
    agents: AGENTS,
    filesResult,
    ...overrides,
  });
}

/** 等右栏出现指定来源。 */
async function selectSource(container: HTMLElement, source: "all" | "git"): Promise<void> {
  click(query(container, `[data-action="select-file-source"][data-file-source-option="${source}"]`), `切到 ${source}`);
  await waitFor(
    () => query(container, `[data-file-source-option="${source}"][data-file-source-selected="true"]`) !== null,
    `${source} 来源被选中`,
  );
}

const checks: readonly Check[] = [
  {
    name: "SF-1 文件栏默认「全部文件」，只有两个来源；手动刷新只发对应目录查询",
    async run() {
      await withSessionPage(
        sessionOptions(),
        (container) => query(container, '[data-files-panel="true"]') !== null,
        "文件栏渲染",
        async (mounted) => {
          const container = mounted.container;
          expectEqual(textOf(query(container, "[data-files-panel-title]")), "文件", "右栏标题");
          const labels = queryAll(container, "[data-file-source-option]").map(textOf);
          expectEqual(labels, ["全部文件", "Git 变更"], "两个来源的顺序与文案");
          expectEqual(
            query(container, '[data-file-source-option="all"]')?.getAttribute("data-file-source-selected"),
            "true",
            "首次进入默认全部文件",
          );
          expectEqual(query(container, '[data-file-source-option="session"]'), null, "不得再出现会话变更来源");
          await waitFor(() => query(container, '[data-file-entry="README.md"]') !== null, "默认来源首次加载目录");
          await waitFor(() => query(container, '[data-action="refresh-files"][data-refresh-disabled="false"]') !== null,
            "目录加载完成后刷新可点击");
          const sentBefore = mounted.host?.sent.length ?? 0;
          mounted.host?.holdCommand("session.files.list");
          click(query(container, '[data-action="refresh-files"]'), "刷新全部文件");
          await waitFor(() => commandBody(mounted.host?.sent.slice(sentBefore) ?? [], "session.files.list") !== null,
            "点击刷新真正发出目录查询");
          const body = commandBody(mounted.host?.sent.slice(sentBefore) ?? [], "session.files.list");
          await waitFor(() => query(container, '[data-refresh-feedback="loading"]') !== null, "刷新按钮忙碌高亮");
          expectEqual(query(container, '[data-action="refresh-files"]')?.getAttribute("aria-disabled"), "true", "刷新期间禁重复点击");
          expectThat(query(container, '[data-file-entry="README.md"]') !== null, "刷新期间保留旧列表");
          mounted.host?.releaseHeldCommand("session.files.list");
          mounted.host?.completeHeldCommand("session.files.list", { result: filesResult("session.files.list", SESSION_ALPHA, { path: "", hideIgnored: true }) });
          await waitFor(() => query(container, '[data-refresh-feedback="success"]') !== null, "全部成功显示勾号");
          const visibleBefore = mounted.host?.sent.length ?? 0;
          mounted.host?.holdCommand("session.files.list");
          document.dispatchEvent(new Event("visibilitychange"));
          await waitFor(() => commandBody(mounted.host?.sent.slice(visibleBefore) ?? [], "session.files.list") !== null,
            "平台前台恢复事件接线触发文件刷新");
          mounted.host?.releaseHeldCommand("session.files.list");
          mounted.host?.completeHeldCommand("session.files.list", { result: filesResult("session.files.list", SESSION_ALPHA, { path: "", hideIgnored: true }) });
          expectEqual(body?.["payload"], { path: "", hideIgnored: true }, "刷新只读根目录，不重载聊天消息");
          expectEqual(
            commandBody(mounted.host?.sent.slice(sentBefore) ?? [], "session.read"),
            null,
            "文件栏不再派发会话变更专属的历史刷新",
          );
        },
      );
    },
  },
  {
    name: "SF-2 全部文件：目录树逐层展开、忽略项默认可见且可隐藏、根外链接可见不可打开",
    async run() {
      await withSessionPage(
        sessionOptions(),
        (container) => query(container, '[data-files-panel="true"]') !== null,
        "文件栏渲染",
        async (mounted) => {
          const container = mounted.container;
          await selectSource(container, "all");
          await waitFor(() => query(container, '[data-file-entry="README.md"]') !== null, "根层加载");

          // 默认隐藏被忽略项；关闭「隐藏忽略文件」后才显示。`.git` 永不出现（主机不会下发，界面也不得虚构）。
          expectEqual(query(container, '[data-file-entry="debug.log"]'), null, "忽略项默认隐藏");
          // `.git` 的说明文字可以出现；可点击的 `.git` 条目本身不能出现。
          expectEqual(query(container, '[data-file-entry=".git"]'), null, "不得出现 .git 条目");
          expectThat(
            queryAll(container, "[data-file-entry]").every((node) => textOf(node).trim() !== ".git"),
            "任何文件项都不得渲染 .git 名称",
          );

          // 根外链接：条目在，但没有打开入口，并说明不能打开。
          const escape = query(container, '[data-file-entry="escape"]');
          expectThat(escape !== null, "根外链接必须可见");
          expectEqual(escape?.getAttribute("data-file-entry-openable"), "false", "根外链接不可打开");
          expectEqual(query(escape, '[data-action="open-file"]'), null, "根外链接不得有打开按钮");
          expectThat(textOf(escape).includes("根外链接"), "根外链接必须说明不能打开");

          // 展开子目录：第一次展开触发第一页。
          click(query(container, '[data-file-directory-row="src"]'), "展开 src");
          await waitFor(() => query(container, '[data-file-entry="src/app.ts"]') !== null, "子目录加载");
          expectEqual(
            query(container, '[data-file-directory-row="src"]')?.getAttribute("data-file-directory-expanded"),
            "true",
            "目录标记为展开",
          );

          // 忽略开关：关闭隐藏后显示被忽略项；再打开后重新隐藏。
          click(query(container, '[data-action="toggle-hide-ignored"]'), "显示忽略文件");
          await waitFor(() => query(container, '[data-file-entry="debug.log"]') !== null, "忽略项显示");
          click(query(container, '[data-action="toggle-hide-ignored"]'), "隐藏忽略文件");
          await waitFor(() => query(container, '[data-file-entry="debug.log"]') === null, "忽略项重新隐藏");
        },
      );
    },
  },
  {
    name: "SF-3 全部文件分页：加载更多后追加，部分结果不冒充完整列表",
    async run() {
      await withSessionPage(
        sessionOptions({ filesResult: pagedFilesResult }),
        (container) => query(container, '[data-files-panel="true"]') !== null,
        "文件栏渲染",
        async (mounted) => {
          const container = mounted.container;
          await selectSource(container, "all");
          await waitFor(() => query(container, '[data-file-entry="first.txt"]') !== null, "第一页加载");
          expectEqual(query(container, '[data-file-entry="second.txt"]'), null, "第二页尚未加载");
          click(query(container, '[data-action="load-more-directory"]'), "加载更多");
          await waitFor(() => query(container, '[data-file-entry="second.txt"]') !== null, "第二页追加");
          expectThat(query(container, '[data-file-entry="first.txt"]') !== null, "加载更多不得替换已有条目");
        },
      );
    },
  },
  {
    name: "SF-4 Git 变更：按文件展示状态与冲突/二进制，非仓库显式说明",
    async run() {
      await withSessionPage(
        sessionOptions(),
        (container) => query(container, '[data-files-panel="true"]') !== null,
        "文件栏渲染",
        async (mounted) => {
          const container = mounted.container;
          await selectSource(container, "git");
          await waitFor(() => query(container, '[data-git-entry="src/app.ts"]') !== null, "Git 状态加载");
          expectEqual(query(container, "[data-git-scope-notice]"), null, "Git 顶部不显示额外范围说明");
          expectEqual(
            query(container, '[data-git-entry="src/app.ts"]')?.getAttribute("data-git-status"),
            "modified",
            "已暂存后再修改的文件按 modified 列出",
          );
          expectThat(
            textOf(query(container, '[data-git-entry="src/conflict.ts"]')).includes("冲突"),
            "冲突标识必须可见",
          );
          expectThat(
            textOf(query(container, '[data-git-entry="assets/pixel.png"]')).includes("二进制"),
            "二进制标识必须可见",
          );
          click(query(container, '[data-action="open-git-file"][data-file-path="src/app.ts"]'), "打开 Git 差异");
          await waitFor(() => query(container, '[data-file-view-content="git_diff"]') !== null, "差异查看器");
          await waitFor(() => query(container, '[data-git-change-marker]') !== null, "原生文件容器右侧显示改动标记");
          expectEqual(
            queryAll(container, "[data-comparison-horizontal-scroll]").length,
            2,
            "固定双栏各有独立横向正文滚动区域",
          );
          expectEqual(
            queryAll(container, '[data-comparison-side="after"][data-comparison-changed="true"]').length,
            1,
            "新增行逐行呈现",
          );
          expectEqual(
            queryAll(container, '[data-comparison-side="before"][data-comparison-changed="true"]').length,
            1,
            "删除行逐行呈现",
          );
          click(query(container, '[data-action="close-file-view"]'), "关闭文件查看");
          await waitFor(() => query(container, "[data-file-view]") === null, "关闭后回到清单");

          // 删除的文件同样能打开相对 HEAD 的差异，并呈现删除行。
          click(query(container, '[data-action="open-git-file"][data-file-path="src/old.ts"]'), "打开已删除文件的差异");
          await waitFor(() => query(container, '[data-file-view-content="git_diff"]') !== null, "删除文件差异查看器");
          expectThat(
            queryAll(container, '[data-comparison-side="before"][data-comparison-changed="true"]').length >= 1,
            "删除文件必须呈现删除行",
          );
          click(query(container, '[data-action="close-file-view"]'), "关闭删除文件差异");
          await waitFor(() => query(container, "[data-file-view]") === null, "删除文件差异关闭");
        },
      );

      await withSessionPage(
        sessionOptions({ filesResult: nonRepositoryResult }),
        (container) => query(container, '[data-files-panel="true"]') !== null,
        "文件栏渲染",
        async (mounted) => {
          const container = mounted.container;
          await selectSource(container, "git");
          await waitFor(() => query(container, '[data-git-state="not_repository"]') !== null, "非仓库说明");
          expectThat(
            query(container, '[data-git-state="not_repository"]')?.getAttribute("aria-label") === "此目录不是 Git 仓库",
            "非仓库保留明确状态标签，不显示说明段落",
          );
          expectEqual(
            query(container, '[data-file-source-option="git"]')?.getAttribute("data-file-source-selected"),
            "true",
            "非仓库时选项保留",
          );
        },
      );
    },
  },
  {
    name: "SF-5 会话改动只在时间线：文件栏无会话清单，结构化差异与诊断入口仍可用",
    async run() {
      const rawDiff = '{"update":{"path":"src/app.ts","diff":"+const first = 1;"}}';
      await withSessionPage(
        sessionOptions({
          events: [
            wireEvent({
              globalSequence: 21,
              eventId: "eeeeeeee-0000-4000-8000-000000000021",
              sessionId: SESSION_ALPHA,
              eventType: "file.changed",
              view: changeView({
                changeId: "cccccccc-0000-4000-8000-000000000021",
                path: "src/app.ts",
                added: "1",
                deleted: "0",
                diffLines: [{ kind: "added", text: "const first = 1;" }],
              }),
              acp: { mediaType: "application/json", rawJson: rawDiff, byteLength: "48", sha256: "a".repeat(43) },
            }),
            wireEvent({
              globalSequence: 22,
              eventId: "eeeeeeee-0000-4000-8000-000000000022",
              sessionId: SESSION_ALPHA,
              eventType: "file.changed",
              view: changeView({
                changeId: "cccccccc-0000-4000-8000-000000000022",
                path: "src/app.ts",
                added: "0",
                deleted: "1",
                diffLines: [{ kind: "deleted", text: "const first = 1;" }],
              }),
            }),
          ],
          readResult: (sessionId) =>
            readPage({
              sessionId,
              history: [
                historyItem({
                  eventId: "eeeeeeee-0000-4000-8000-000000000020",
                  eventType: "file.changed",
                  sessionSequence: 20,
                  createdAt: "2026-10-01T00:00:20.000Z",
                  view: changeView({
                    changeId: "cccccccc-0000-4000-8000-000000000020",
                    path: "src/history.ts",
                    added: "1",
                    deleted: "0",
                  }),
                }),
              ],
              historyHasEarlier: true,
            }),
        }),
        (container) =>
          queryAll(container, '[data-timeline-card="file-changed"]').length === 3 &&
          query(container, '[data-action="open-file-change-diagnostics"]') !== null,
        "时间线文件改动渲染",
        async (mounted) => {
          const container = mounted.container;
          // 文件栏没有会话变更清单或记录选择器。
          expectEqual(queryAll(container, '[data-conversation-column="files"] [data-session-file]').length, 0, "文件栏无会话文件行");
          expectEqual(query(container, '[data-action="select-file-change"]'), null, "不得再有会话来源联动入口");
          expectEqual(query(container, '[data-file-view-content="session_changes"]'), null, "不得打开会话变更查看器");

          // 实时两条与历史一条都在时间线；差异行只来自结构化来源。
          expectEqual(
            queryAll(container, '[data-timeline-card="file-changed"]').map((node) => node.getAttribute("data-change-path")),
            ["src/app.ts", "src/app.ts", "src/history.ts"],
            "实时与历史的改动按时间呈现",
          );
          expectEqual(queryAll(container, '[data-timeline-card="file-changed"] [data-diff-region="lines"]').length, 2, "有差异行的两条逐行呈现");
          expectEqual(queryAll(container, '[data-timeline-card="file-changed"] [data-diff-not-delivered="true"]').length, 1, "缺差异时明确说明");
          expectThat(
            textOf(query(container, '[data-timeline-card="file-changed"] [data-change-added-lines]')).includes("+1"),
            "真实统计保留",
          );

          // 诊断入口：原文以单块不可信文本呈现，不解析成差异行。
          click(query(container, '[data-action="open-file-change-diagnostics"]'), "展开 ACP 原文诊断");
          await waitFor(() => query(container, '[data-raw-json="available"]') !== null, "原文诊断展开");
          expectThat(textOf(query(container, '[data-raw-json="available"]')).includes("const first = 1;"), "原文整串呈现");
          expectEqual(queryAll(container, '[data-timeline-card="file-changed"] [data-diff-region="lines"]').length, 2, "原文不得变成额外差异行");
          expectEqual(query(container, "[data-file-tabs]"), null, "不得出现多文件页签");
        },
      );
    },
  },
  {
    name: "SF-6 中间只读查看：文本按纯文本、图片只用 Blob URL，HTML/SVG 不执行",
    async run() {
      await withSessionPage(
        sessionOptions(),
        (container) => query(container, '[data-files-panel="true"]') !== null,
        "文件栏渲染",
        async (mounted) => {
          const container = mounted.container;
          await selectSource(container, "all");
          await waitFor(() => query(container, '[data-file-entry="README.md"]') !== null, "根层加载");

          click(query(container, '[data-action="open-file"][data-file-path="README.md"]'), "打开文本");
          await waitFor(() => query(container, '[data-file-view="ready"]') !== null, "文本预览就绪");
          expectEqual(
            query(container, "[data-file-view]")?.getAttribute("data-file-view-path"),
            "README.md",
            "查看器明确显示路径",
          );
          expectEqual(
            query(container, "[data-file-view-source-label]")?.getAttribute("data-file-view-source-label"),
            "all",
            "查看器明确显示来源",
          );
          const text = query(container, "[data-file-preview-text]");
          expectThat(text?.textContent?.includes("<script>window.__pwned") === true, "脚本按文本逐字呈现");
          expectThat(Reflect.get(window, "__pwned") === undefined, "脚本不得执行");
          expectEqual(query(container, "[data-file-preview-text] script"), null, "不得产生 script 元素");
          expectEqual(query(container, "[data-file-preview-text] img"), null, "不得产生远端图片元素");
          const scroller = query(container, '[data-file-view-scroll="true"]');
          expectThat(scroller instanceof HTMLElement, "查看器必须有独立滚动容器");
          if (scroller instanceof HTMLElement) {
            expectThat(scroller.scrollHeight > scroller.clientHeight, "长文本必须能滚动");
            scroller.scrollTop = scroller.scrollHeight;
            expectThat(scroller.scrollTop > 0, "长文本能滚到底");
          }

          click(query(container, '[data-action="close-file-view"]'), "关闭文本");
          await waitFor(() => query(container, "[data-file-view]") === null, "文本查看器关闭");

          click(query(container, '[data-file-directory-row="assets"]'), "展开 assets");
          await waitFor(() => query(container, '[data-file-entry="assets/pixel.png"]') !== null, "图片条目");
          click(query(container, '[data-action="open-file"][data-file-path="assets/pixel.png"]'), "打开图片");
          await waitFor(() => query(container, '[data-file-preview-image="true"]') !== null, "图片以 Blob URL 呈现");
          const src = query(container, 'img[data-file-preview-image="true"]')?.getAttribute("src") ?? "";
          expectThat(src.startsWith("blob:"), `图片地址必须是同源 Blob URL，实际 ${src}`);
          click(query(container, '[data-action="close-file-view"]'), "关闭图片");
          await waitFor(() => query(container, "[data-file-view]") === null, "图片查看器关闭");

          click(query(container, '[data-action="open-file"][data-file-path="evil.svg"]'), "打开 SVG");
          await waitFor(() => query(container, '[data-file-preview-text]') !== null, "SVG 按文本呈现");
          expectEqual(query(container, "[data-file-preview-text] svg"), null, "SVG 不得成为活动元素");
          expectThat(Reflect.get(window, "__pwned") === undefined, "SVG 事件不得执行");
        },
      );
    },
  },
  {
    name: "SF-7 A→B 直接替换、关闭后草稿/滚动/展开状态与焦点恢复",
    async run() {
      const messages = Array.from({ length: 30 }, (_, index) =>
        readMessage({
          messageId: `ffffffff-0000-4000-8000-${String(index).padStart(12, "0")}`,
          sessionId: SESSION_ALPHA,
          role: "agent",
          text: `第 ${String(index + 1)} 条消息`,
          createdAt: "2026-10-01T00:00:00.000Z",
        }),
      );
      await withSessionPage(
        sessionOptions({
          events: [
            wiredToolEvent(),
            changedEvent(41, "cccccccc-0000-4000-8000-000000000041", "src/app.ts", {
              diffLines: [{ kind: "added", text: "const a = 1;" }],
              added: "1",
              deleted: "0",
            }),
          ],
          readResult: (sessionId) => readPage({ sessionId, messages }),
        }),
        (container) => query(container, "[data-timeline]") !== null,
        "时间线渲染",
        async (mounted) => {
          const container = mounted.container;
          // 草稿与展开状态：打开文件前后必须保持。
          setComposerDraft(container, "还没发送的草稿");

          const toggle = query(container, '[data-action="toggle-timeline-entry"]');
          expectThat(toggle !== null, "工具条目可展开");
          click(toggle, "展开工具条目");
          await waitFor(() => query(container, '[data-entry-expanded="true"]') !== null, "条目已展开");

          const timeline = query(container, '[data-conversation-scroll="timeline"]');
          expectThat(timeline !== null, "时间线滚动容器");
          timeline.scrollTop = 120;
          // 只改 scrollTop 不会触发 React 的滚动状态；真实用户滚动会派发 scroll 事件。
          // 不派发时页面仍按「跟随底部」处理，下一次内容变化会把位置归零。
          timeline.dispatchEvent(new Event("scroll"));
          await waitFor(
            () => timeline.getAttribute("data-scroll-follow") === "false",
            "时间线进入上读状态",
          );
          const beforeScroll = timeline.scrollTop;

          await selectSource(container, "all");
          await waitFor(() => query(container, '[data-file-entry="README.md"]') !== null, "根层加载");
          click(query(container, '[data-action="open-file"][data-file-path="README.md"]'), "打开 A");
          await waitFor(() => query(container, '[data-file-view-path-label="README.md"]') !== null, "A 打开");

          // A → B 直接替换，不加页签。
          await selectSource(container, "all");
          click(query(container, '[data-file-directory-row="src"]'), "展开 src");
          await waitFor(() => query(container, '[data-file-entry="src/app.ts"]') !== null, "src 子目录加载");
          click(query(container, '[data-action="open-file"][data-file-path="src/app.ts"]'), "打开 B");
          await waitFor(() => query(container, '[data-file-view-path-label="src/app.ts"]') !== null, "B 替换 A");
          expectEqual(query(container, "[data-file-tabs]"), null, "不得出现多文件页签");

          click(query(container, '[data-action="close-file-view"]'), "关闭文件查看");
          await waitFor(() => query(container, "[data-file-view]") === null, "关闭后回对话");
          const inputAfter = query(container, "[data-composer-input]");
          expectThat(inputAfter instanceof HTMLTextAreaElement, "输入区仍在 DOM");
          expectEqual(inputAfter.value, "还没发送的草稿", "草稿必须保留");
          expectThat(query(container, '[data-entry-expanded="true"]') !== null, "条目展开状态必须保留");
          const timelineAfter = query(container, '[data-conversation-scroll="timeline"]');
          // 恢复发生在关闭后的被动 effect 里，必须等它落地再断言，而不是抢在 effect 之前读。
          await waitFor(
            () => timelineAfter?.scrollTop === beforeScroll,
            "关闭后恢复原滚动位置（不强制滚到底）",
          );
          expectEqual(timelineAfter?.scrollTop, beforeScroll, "关闭后恢复原滚动位置（不强制滚到底）");
          // 关闭后焦点回到触发文件行。
          await waitFor(
            () => document.activeElement?.getAttribute("data-file-row") !== null,
            "焦点回到文件行",
          );

          // Git 来源同样在关闭后把焦点还给触发行。
          await selectSource(container, "git");
          await waitFor(() => query(container, '[data-git-entry="src/app.ts"]') !== null, "Git 行加载");
          const gitDirectory = query(container, '[data-git-changes] [data-file-directory-row="src/"]');
          if (gitDirectory?.getAttribute("aria-expanded") !== "true") {
            click(gitDirectory, "展开 Git 文件所在目录");
            await waitFor(() => gitDirectory?.getAttribute("aria-expanded") === "true", "Git 文件行实际可见");
          }
          click(query(container, '[data-action="open-git-file"][data-file-path="src/app.ts"]'), "从 Git 打开");
          await waitFor(() => query(container, '[data-file-view-content="git_diff"]') !== null, "Git 差异查看器");
          click(query(container, '[data-action="close-file-view"]'), "关闭 Git 差异");
          await waitFor(
            () => document.activeElement?.getAttribute("data-file-row") === "git:src/app.ts",
            "焦点回到 Git 触发行",
          );
        },
      );
    },
  },
  {
    name: "SF-8 窄屏打开文件后关闭返回原文件列表与滚动位置；跨断点保留选中文件",
    async run() {
      const restore = overrideViewportWidth(600);
      try {
        await withSessionPage(
          sessionOptions(),
          (container) => query(container, '[data-segment="conversation-secondary-view"]') !== null,
          "窄屏切换控件",
          async (mounted) => {
            const container = mounted.container;
            click(query(container, '[data-segment-option="files"]'), "切到文件栏");
            await waitFor(
              () => getComputedStyle(query(container, '[data-conversation-column="files"]') ?? container).display !== "none",
              "文件栏可见",
            );
            await waitFor(() => query(container, '[data-file-entry="README.md"]') !== null, "根层加载");
            const filesScroll = query(container, '[data-conversation-scroll="files"]');
            expectThat(filesScroll !== null, "文件栏滚动容器");
            filesScroll.scrollTop = 50;
            const before = filesScroll.scrollTop;

            click(
              query(container, '[data-action="open-file"][data-file-path="README.md"]'),
              "从全部文件打开",
            );
            await waitFor(() => query(container, "[data-file-view]") !== null, "窄屏查看器");
            expectEqual(
              getComputedStyle(query(container, '[data-conversation-column="files"]') ?? container).display,
              "none",
              "窄屏查看文件时文件栏不占主体",
            );

            // 跨断点：变宽后仍显示同一个文件与来源。
            const widen = overrideViewportWidth(1200);
            try {
              await waitFor(
                () => getComputedStyle(query(container, '[data-conversation-column="files"]') ?? container).display !== "none",
                "变宽后三栏恢复",
              );
              expectThat(query(container, "[data-file-view]") !== null, "跨断点保留当前选中文件");
            } finally {
              widen();
            }
            // 变回窄屏再关闭：返回文件列表而不是对话顶部。
            await waitFor(
              () => getComputedStyle(query(container, '[data-conversation-column="files"]') ?? container).display === "none",
              "回到窄屏",
            );
            click(query(container, '[data-action="close-file-view"]'), "关闭文件查看");
            await waitFor(
              () => query(container, '[data-segment-option="files"]')?.getAttribute("data-segment-selected") === "true",
              "关闭后返回文件栏",
            );
            const after = query(container, '[data-conversation-scroll="files"]');
            expectEqual(after?.scrollTop, before, "关闭后恢复文件栏滚动位置");
          },
        );
      } finally {
        restore();
      }
    },
  },
  {
    name: "SF-9 下载与取消：多块二进制按字节核对，取消后不留下未完成内容",
    async run() {
      await withSessionPage(
        sessionOptions(),
        (container) => query(container, '[data-files-panel="true"]') !== null,
        "文件栏渲染",
        async (mounted) => {
          const container = mounted.container;
          await selectSource(container, "all");
          await waitFor(() => query(container, '[data-file-entry="bundle.bin"]') !== null, "根层加载");
          click(query(container, '[data-action="open-file"][data-file-path="bundle.bin"]'), "打开二进制");
          await waitFor(() => query(container, '[data-action="download-file"]') !== null, "下载入口");

          click(query(container, '[data-action="download-file"]'), "开始下载");
          await waitFor(() => query(container, '[data-download-state="saved"]') !== null, "下载完成");
          const anchor = query(container, "a[data-download-artifact]");
          const url = anchor?.getAttribute("href") ?? "";
          expectThat(url.startsWith("blob:"), "下载交付物是 Blob URL");
          // 页面 CSP 的 `connect-src 'self'` 不允许在页面内 fetch(blob:)；字节一致性由
          // `crates/app/src/daemon/sync_ingress_tests.rs` 的真实 daemon 用例逐字节证明。
          expectEqual(
            query(container, "[data-download-saved]")?.getAttribute("data-download-saved"),
            String(BINARY_BYTES.length),
            "下载写入的字节数",
          );
          click(query(container, '[data-action="release-download"]'), "释放下载资源");

          // 取消：从此刻起扣住读块，点下载后取消，状态必须停在 cancelled。
          const host = mounted.host;
          if (host === null) throw new CheckFailure("场景未挂载假主机");
          host.holdCommand("session.files.read");
          click(query(container, '[data-action="download-again"]'), "再次下载");
          await waitFor(() => query(container, '[data-download-state="downloading"]') !== null, "下载中");
          click(query(container, '[data-action="cancel-download"]'), "取消下载");
          await waitFor(() => query(container, '[data-download-state="cancelled"]') !== null, "下载已取消");
          host.releaseHeldCommand("session.files.read");
          expectThat(query(container, '[data-download-state="saved"]') === null, "取消后不得变成已保存");
        },
      );
    },
  },
  {
    name: "SF-10 关闭按钮可聚焦且键盘可触发（加载态也可关闭）",
    async run() {
      await withSessionPage(
        sessionOptions(),
        (container) => query(container, '[data-files-panel="true"]') !== null,
        "文件栏渲染",
        async (mounted) => {
          const container = mounted.container;
          await selectSource(container, "all");
          await waitFor(() => query(container, '[data-file-entry="README.md"]') !== null, "根层加载");
          click(query(container, '[data-action="open-file"][data-file-path="README.md"]'), "打开文本");
          await waitFor(() => query(container, '[data-file-view="ready"]') !== null, "查看器就绪");
          const close = query(container, '[data-action="close-file-view"]');
          expectThat(close !== null, "关闭按钮");
          expectEqual(close?.getAttribute("aria-label"), "关闭文件查看", "关闭按钮名称明确");
          expectThat(Number(close?.getAttribute("tabindex") ?? "-1") >= 0, "关闭按钮可聚焦");
          (close as HTMLElement | null)?.focus();
          close?.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", code: "Enter", keyCode: 13, which: 13, bubbles: true }));
          await waitFor(() => query(container, "[data-file-view]") === null, "键盘关闭");
        },
      );
    },
  },
];

/** 一条工具调用事件（用来钉住「条目展开状态不被文件视图清理」）。 */
function wiredToolEvent(): Record<string, unknown> {
  return wireEvent({
    globalSequence: 40,
    eventId: "eeeeeeee-0000-4000-8000-000000000040",
    sessionId: SESSION_ALPHA,
    eventType: "tool.call.completed",
    view: {
      toolCallId: "dddddddd-0000-4000-8000-000000000040",
      turnId: "dddddddd-0000-4000-8000-000000000041",
      title: "读取部署脚本",
      state: "completed",
    },
  });
}

export const SESSION_FILES_CHECKS: readonly Check[] = checks;

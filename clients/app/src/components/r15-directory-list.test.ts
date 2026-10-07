/**
 * R15：目录页渲染层的负向断言。
 *
 * 每条 MUST NOT 都写成「构造违规输入 → 断言渲染结果不含违规内容」：
 *
 * - 目录项只显示展示名与别名 → 快照资源里的 `path`/`root` 不得出现在 HTML 里；
 * - 同名目录不以路径消歧 → 两个同名目录必须各自带别名，且没有任何路径片段；
 * - 两种空态可区分 → 两段文案与 `data-empty-state` 都不同；
 * - 断连仍可渲染并标注来源 → 断连时目录行仍在，且标注「来自上次同步」；
 * - 目录详情里「别名失效」与「目录存在但无会话」是两种呈现 → 两种文案互不冒充。
 *
 * 判别力：断言的是**渲染后的 HTML 字符串**而不是模型字段——模型里没有路径字段并不能
 * 保证组件不会去渲染别的东西（例如从 alias 派生一个「父目录」标签）。
 *
 * 本文件用 `createElement` 而非 JSX：`vitest.config.ts` 的 `include` 只收 `*.test.ts`，
 * 组件本身仍是 `.tsx`（在 `src/components/` 下照常被类型检查与 web 打包）。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { DirectoryDetail } from "./DirectoryDetail";
import { DirectoryList } from "./DirectoryList";
import { DIRECTORY_EMPTY_COPY, buildDirectoryPageModel } from "../features/directory-model";
import type { DirectoryDetailModel, DirectoryPageModel } from "../features/directory-model";
import { buildCreateSessionEntryModel } from "../features/create-session-model";
import { WORKSPACES, WORKSPACES_WITH_PATHS, resources, session, storeWith } from "../features/testing/fixtures";

const SESSIONS = [
  session({ id: "aaaaaaaa-0000-4000-8000-000000000001", state: "running" }),
  session({ id: "aaaaaaaa-0000-4000-8000-000000000002", state: "waiting_permission" }),
  session({ id: "aaaaaaaa-0000-4000-8000-000000000003", state: "idle", alias: "personal-api" }),
];

/** 渲染目录页。点击回调在渲染测试里不会被触发。 */
function renderDirectoryList(model: DirectoryPageModel): string {
  return renderToStaticMarkup(
    createElement(DirectoryList, {
      model,
      onOpenDirectory: () => undefined,
    }),
  );
}

/**
 * 渲染目录详情。模型由 `ClientStore.directoryDetail` 取（与页面同一条取数路径），
 * 因此这里断言的是用户真正看到的那句话，而不是模型字段。
 */
function renderDirectoryDetail(model: DirectoryDetailModel): string {
  return renderToStaticMarkup(
    createElement(DirectoryDetail, {
      model,
      createEntry: buildCreateSessionEntryModel({
        directoryAlias: "work-api",
        scopes: ["session.create"],
        agents: [],
        record: null,
        connectionOnline: true,
      }),
      onOpenSession: () => undefined,
      onCreateSession: () => undefined,
      canCreate: true,
    }),
  );
}

describe("R15 目录项不暴露路径", () => {
  it("带路径字段的目录项渲染后不含任何路径信息", () => {
    const html = renderDirectoryList(
      buildDirectoryPageModel({
        resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES_WITH_PATHS }),
        connection: "online",
        lastSyncedAtMs: 1_700_000_000_000,
      }),
    );

    // 反例：若组件把 path/root 当作附加信息渲染，这里会出现 `D:\work`，断言变红。
    expect(html).not.toContain("D:\\work");
    expect(html).not.toContain("D:\\personal");
    expect(html).not.toContain("path=");
    expect(html).not.toContain("root=");
    // 展示名与别名仍然必须在（否则「只显示展示名与别名」会被实现成「什么都不显示」）。
    expect(html).toContain("work-api");
    expect(html).toContain("personal-api");
  });

  it("同名目录各自带别名，且没有任何路径片段用于消歧", () => {
    const html = renderDirectoryList(
      buildDirectoryPageModel({
        resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
        connection: "online",
        lastSyncedAtMs: null,
      }),
    );

    // 两个同名目录是两项，别名各自出现。
    expect(html.match(/data-directory-display-name="api"/g)).toHaveLength(2);
    expect(html).toContain(">work-api<");
    expect(html).toContain(">personal-api<");
    // 反例：若用「父目录/末段」消歧，这里会出现反斜杠或盘符，断言变红。
    expect(html).not.toContain("\\");
    expect(html).not.toMatch(/[A-Za-z]:[\\/]/);
  });

  it("目录项渲染出三项汇总（会话数/运行中/待处理）", () => {
    const html = renderDirectoryList(
      buildDirectoryPageModel({
        resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
        connection: "online",
        lastSyncedAtMs: null,
      }),
    );

    expect(html).toContain('data-directory-count="total"');
    expect(html).toContain('data-directory-count="active"');
    expect(html).toContain('data-directory-count="pending"');
  });
});

describe("R15 断连时目录仍可渲染", () => {
  it("断连时目录行照常渲染，并明确标注内容来自上次同步", () => {
    const model = buildDirectoryPageModel({
      resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
      connection: "disconnected",
      lastSyncedAtMs: 1_700_000_000_000,
    });
    const html = renderDirectoryList(model);

    expect(model.kind === "loading" ? null : model.contentOrigin).toBe("last_sync");
    expect(html).toContain('data-content-origin="last_sync"');
    expect(html).toContain("内容来自上次同步");
    // 反例：断连时不渲染目录项，这里会是空列表，断言变红。
    expect(html).toContain('data-directory-alias="work-api"');
  });

  it("对照：在线时标注为本次同步（证明上一条不是因为标注写死）", () => {
    const html = renderDirectoryList(
      buildDirectoryPageModel({
        resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
        connection: "online",
        lastSyncedAtMs: null,
      }),
    );
    expect(html).toContain('data-content-origin="live"');
  });
});

describe("R15 两种空态可区分", () => {
  it("目录存在但没有会话：呈现该空态的文案与指引", () => {
    const html = renderDirectoryList(
      buildDirectoryPageModel({
        resources: resources({ sessions: [], workspaces: WORKSPACES }),
        connection: "online",
        lastSyncedAtMs: null,
      }),
    );
    expect(html).toContain('data-empty-state="directories_without_sessions"');
    expect(html).toContain(DIRECTORY_EMPTY_COPY.directories_without_sessions.title);
    expect(html).toContain(DIRECTORY_EMPTY_COPY.directories_without_sessions.guidance);
  });

  it("一个目录都没有：呈现另一种空态，且与上一种在标题/指引上都不同", () => {
    const html = renderDirectoryList(
      buildDirectoryPageModel({
        resources: resources({ sessions: [], workspaces: [] }),
        connection: "online",
        lastSyncedAtMs: null,
      }),
    );

    expect(html).toContain('data-empty-state="no_directories"');
    expect(html).toContain(DIRECTORY_EMPTY_COPY.no_directories.title);
    // 反例：若两种空态共用一句话，这里会同时出现上一种的指引，断言变红。
    expect(html).not.toContain(DIRECTORY_EMPTY_COPY.directories_without_sessions.guidance);
    expect(DIRECTORY_EMPTY_COPY.no_directories.title).not.toBe(DIRECTORY_EMPTY_COPY.directories_without_sessions.title);
    expect(DIRECTORY_EMPTY_COPY.no_directories.guidance).not.toBe(DIRECTORY_EMPTY_COPY.directories_without_sessions.guidance);
  });

  it("从未同步过：呈现 loading，不冒充任何一种空态", () => {
    const html = renderDirectoryList(
      buildDirectoryPageModel({ resources: null, connection: "online", lastSyncedAtMs: null }),
    );
    // 反例：若无快照时渲染「本机没有任何目录」，这里会出现 data-empty-state，断言变红。
    expect(html).toContain('data-directory-state="loading"');
    expect(html).not.toContain("data-empty-state");
  });
});

describe("R15 目录详情：链接失效与「目录存在但无会话」是两种呈现", () => {
  it("目录存在但没有任何会话：渲染「还没有会话」，不渲染「链接已失效」的告警", () => {
    // 生产路径：模型经 ClientStore 取出（与页面同一路径），不直调构建函数。
    const { store } = storeWith({ resources: resources({ sessions: [], workspaces: WORKSPACES }) });
    const html = renderDirectoryDetail(store.directoryDetail("work-api"));

    expect(html).toContain('data-directory-sessions="empty"');
    expect(html).toContain("这个目录里还没有会话");
    // 反例：若详情模型把「有目录但无会话」当成别名失效，这里渲染的是告警，断言变红。
    expect(html).not.toContain('data-directory-detail-state="unknown_directory"');
    expect(html).not.toContain("可能已被删除");
  });

  it("对照：别名不在快照里才是「链接已失效」——两种呈现互不冒充", () => {
    const { store } = storeWith({ resources: resources({ sessions: [], workspaces: WORKSPACES }) });
    const html = renderDirectoryDetail(store.directoryDetail("not-there"));

    expect(html).toContain('data-directory-detail-state="unknown_directory"');
    expect(html).not.toContain('data-directory-sessions="empty"');
  });
});

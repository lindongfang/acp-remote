import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { SessionFilesPanel } from "./SessionFilesPanel";
import type { SessionFilesPanelModel } from "../features/session-files";
import { onlineStore, ONLINE_SESSION_ID, resources, session } from "../features/testing/fixtures";

function base(): SessionFilesPanelModel {
  const { store } = onlineStore({ resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID })] }) });
  return store.sessionFilesPage(ONLINE_SESSION_ID);
}

describe("文件栏的目录与状态展示", () => {
  it("全部文件只压缩已完整加载的单分支目录", () => {
    const model = base();
    const directories = new Map(model.all.directories);
    directories.set("", { path: "", entries: [{ path: "clients", name: "clients", kind: "directory", openable: true }],
      next: null, gitState: "available", loading: false, error: null });
    directories.set("clients", { path: "clients", entries: [{ path: "clients/app", name: "app", kind: "directory", openable: true }],
      next: null, gitState: "available", loading: false, error: null });
    const render = () => renderToStaticMarkup(createElement(SessionFilesPanel, { model: {
      ...model, selectedSource: "all", allNotice: null, all: { ...model.all, directories },
    } }));
    expect(render()).toContain('data-file-directory-row="clients/app"');
    const cached = directories.get("clients");
    if (cached === undefined) throw new Error("missing cached directory");
    directories.set("clients", { ...cached, next: "more" });
    expect(render()).toContain('data-file-directory-row="clients"');
    expect(render()).not.toContain('data-file-directory-row="clients/app"');
  });

  it("Git 同名文件保留各自目录、完整打开路径、增删标记与工作目录范围说明", () => {
    const model = base();
    const html = renderToStaticMarkup(createElement(SessionFilesPanel, { model: {
      ...model, selectedSource: "git", gitNotice: null,
      git: { ...model.git, gitState: "available", entries: [
        { path: "src/ui/a.ts", status: "modified", conflicted: false, binary: false },
        { path: "tests/a.ts", status: "deleted", conflicted: false, binary: false },
        { path: "src/new.ts", status: "untracked", conflicted: false, binary: false },
      ] },
    }, onOpenFile: () => {} }));
    for (const path of ["src/", "src/ui/", "tests/"]) expect(html).toContain(`data-file-directory-row="${path}"`);
    expect(html).toContain('data-file-icon="folder"');
    expect(html).toContain('data-file-icon="file"');
    expect(html).toContain('data-file-path="src/ui/a.ts"');
    expect(html.match(/data-file-change-marker="positive"/g)).toHaveLength(2);
    expect(html.match(/data-file-change-marker="negative"/g)).toHaveLength(1);
    expect(html).not.toContain('>src/ui/a.ts<');
    // 范围说明：整个会话工作目录，可能包含人工与其他会话的改动。
    expect(html).toContain('data-git-scope-notice="true"');
    expect(html).toContain("整个会话工作目录");
    expect(html).toContain("人工或其他会话");
  });

  it("只有全部文件与 Git 变更两个来源，没有会话变更选项或专属记录入口", () => {
    const model = base();
    const html = renderToStaticMarkup(createElement(SessionFilesPanel, { model }));
    const options = [...html.matchAll(/data-file-source-option="([^"]+)"/g)].map((match) => match[1]);
    expect(options).toEqual(["all", "git"]);
    expect(html).toContain("全部文件");
    expect(html).toContain("Git 变更");
    expect(html).not.toContain("会话变更");
    expect(html).not.toContain('data-file-source-option="session"');
    expect(html).not.toContain("data-session-file=");
    expect(html).not.toContain("data-session-change-record=");
    expect(html).not.toContain('data-action="select-file-change"');
    expect(html).toContain('data-file-source-option="all" data-file-source-selected="true"');
  });
});

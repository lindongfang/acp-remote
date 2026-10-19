import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { SessionFilesPanel } from "./SessionFilesPanel";
import type { SessionFilesPanelModel } from "../features/session-files";
import type { SessionChangeRecord } from "../domain/session-files";
import { onlineStore, ONLINE_SESSION_ID, resources, session } from "../features/testing/fixtures";

function base(): SessionFilesPanelModel {
  const { store } = onlineStore({ resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID })] }) });
  return store.sessionFilesPage(ONLINE_SESSION_ID);
}

function record(kind: string, id: string): SessionChangeRecord {
  return { eventId: id, changeId: id, kind, displayPath: "src/ui/a.ts", summary: kind,
    addedLines: null, deletedLines: null, diff: null, sessionSequence: id, createdAt: "2026-10-19T00:00:00Z",
    diagnostics: { eventId: id, eventType: "file.changed", createdAt: "2026-10-19T00:00:00Z",
      sessionSequence: id, globalSequence: null, view: {} } };
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
  it("Git 同名文件保留各自目录、完整打开路径和增删标记", () => {
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
  });

  it("选中早期记录不会将最新删除文件的红色标记变成绿色", () => {
    const model = base();
    const html = renderToStaticMarkup(createElement(SessionFilesPanel, { model: {
      ...model, selectedSource: "session", sessionChanges: {
        ...model.sessionChanges, fileCount: 1, files: [{ key: "path:src/ui/a.ts", path: "src/ui/a.ts",
          displayPath: "src/ui/a.ts", outsideWorkspace: false, readable: true, unreadableReason: null,
          records: [record("update", "1"), record("delete", "2")] }],
      },
    }, selectedChangeId: "1" }));
    expect(html).toContain('data-file-directory-row="src/ui/"');
    expect(html).toContain('data-file-change-marker="negative"');
    expect(html).not.toContain('data-file-change-marker="positive"');
    expect(html).toContain('data-session-change-record="1"');
    expect(html).toContain('data-session-change-record="2"');
  });
});

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { SessionFilesPanel, toggleFileDirectoryChain, visibleExpandedDirectories } from "./SessionFilesPanel";
import type { SessionFilesPanelModel } from "../features/session-files";
import { onlineStore, ONLINE_SESSION_ID, resources, session } from "../features/testing/fixtures";

function base(): SessionFilesPanelModel {
  const { store } = onlineStore({ resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID })] }) });
  return store.sessionFilesPage(ONLINE_SESSION_ID);
}

describe("file refresh feedback", () => {
  it("keeps every merged ancestor open when refreshing breaks the compact chain", () => {
    const expanded = toggleFileDirectoryChain(new Set(), ["src", "src/lib"]);
    expect([...expanded]).toEqual(["src", "src/lib"]);
    const model = base();
    const directory = (path: string, children: readonly string[], loading = false) => ({
      path, entries: children.map(child => ({ path: child, name: child.split("/").pop() ?? child, kind: "directory" as const, openable: true })),
      next: null, gitState: "available" as const, loading, error: null,
    });
    for (const loading of [false, true]) {
      const directories = new Map(model.all.directories);
      directories.set("", directory("", ["src"]));
      directories.set("src", directory("src", ["src/lib", "src/other"], loading));
      directories.set("src/lib", directory("src/lib", []));
      expect(visibleExpandedDirectories({ ...model, all: { ...model.all, directories } }, expanded)).toEqual(["src", "src/lib"]);
    }
  });

  it("collapses the complete compact chain and hidden descendants in one update", () => {
    const expanded = new Set(["src", "src/lib", "src/lib/deep", "src/other", "src-other"]);
    expect([...toggleFileDirectoryChain(expanded, ["src", "src/lib"])]).toEqual(["src-other"]);
    expect([...expanded]).toContain("src/lib/deep");
    expect([...toggleFileDirectoryChain(new Set(["src/lib", "src/lib/deep"]), ["src", "src/lib"])]).toEqual([]);
  });

  it("marks refresh busy and disables repeated clicks without loading text", () => {
    const html = renderToStaticMarkup(createElement(SessionFilesPanel, {
      model: { ...base(), refreshState: { kind: "loading" } }, onRefresh: () => {},
    }));
    expect(html).toContain('data-refresh-feedback="loading"');
    expect(html).toContain('data-refresh-disabled="true"');
    expect(html).toContain('aria-busy="true"');
    expect(html).not.toContain("正在加载");
  });

  it("registers merged ancestors and excludes descendants of collapsed parents", () => {
    const model = base();
    const directories = new Map(model.all.directories);
    const directory = (path: string, children: readonly string[]) => ({
      path, entries: children.map(child => ({ path: child, name: child.split("/").pop() ?? child, kind: "directory" as const, openable: true })),
      next: null, gitState: "available" as const, loading: false, error: null,
    });
    directories.set("", directory("", ["src"]));
    directories.set("src", directory("src", ["src/app"]));
    directories.set("src/app", directory("src/app", ["src/app/a", "src/app/b"]));
    const tree = { ...model, all: { ...model.all, directories } };
    expect(visibleExpandedDirectories(tree, new Set(["src/app", "src/app/a"]))).toEqual(["src", "src/app", "src/app/a"]);
    expect(visibleExpandedDirectories(tree, new Set(["src/app/a"]))).toEqual([]);
    expect(visibleExpandedDirectories(tree, new Set())).toEqual([]);
  });
});

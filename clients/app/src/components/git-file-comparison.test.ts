import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import { GitFileComparison } from "./GitFileComparison";
import * as theme from "./theme";
import type { GitDiffView } from "../domain/session-files";

const diff: GitDiffView={path:"a.ts",base:"head",kind:"text",diffLines:[],unavailable:null,truncated:false,
  comparison:{before:"same\nold\n",after:"same\nnew\n"}};
afterEach(()=>vi.restoreAllMocks());
describe("Git two-column file view",()=>{
  it("shows complete before/after texts in two columns on desktop",()=>{
    vi.spyOn(theme,"useWindowDimensions").mockReturnValue({width:1280,height:800,scale:1,fontScale:1});
    const html=renderToStaticMarkup(createElement(GitFileComparison,{diff}));
    expect(html).toContain('data-git-comparison="full"');
    expect(html).toContain("grid-template-columns:minmax(0,1fr) minmax(0,1fr)");
    expect(html).toContain('data-comparison-fixed-panes="true"');
    expect(html).toContain('data-comparison-horizontal-scroll="before"');
    expect(html).toContain('data-comparison-horizontal-scroll="after"');
    expect(html).toContain('data-comparison-pane="before"');
    expect(html).toContain('data-comparison-pane="after"');
    expect(html).toContain("修改前");expect(html).toContain("修改后");
    expect(html).toContain("old");expect(html).toContain("new");
    expect(html).toContain('data-comparison-changed="true"');
    expect(html).toContain("white-space:pre");
    expect(html).not.toContain("white-space:pre-wrap");
    expect(html).toContain("overflow-x:auto");
    expect(html).not.toContain('data-git-change-overview="true"');
    expect(html).toContain("position:sticky");
  });
  it("labels older-host fragments as partial and escapes untrusted content",()=>{
    const partial: GitDiffView={path:"a.ts",base:"head",kind:"text",unavailable:null,truncated:false,diffLines:["@@ -1 +1 @@","-<script>old</script>","+new"]};
    const html=renderToStaticMarkup(createElement(GitFileComparison,{diff:partial}));
    expect(html).toContain('data-git-comparison="partial"');
    expect(html).toContain("当前仅提供局部对比");
    expect(html).toContain("&lt;script&gt;");expect(html).not.toContain("<script>");
  });
  it("does not add a second scrollbar inside the comparison panes",()=>{
    const html=renderToStaticMarkup(createElement(GitFileComparison,{diff:{...diff,comparison:{before:"same\n",after:"same\n"}}}));
    expect(html).not.toContain('data-git-change-overview="true"');
    expect(html).not.toContain('data-git-change-marker=');
  });
});

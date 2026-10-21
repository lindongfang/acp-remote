import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { FileContentViewer } from "./FileContentViewer";
import type { FileContentState } from "../features/session-files";
import { ONLINE_SESSION_ID } from "../features/testing/fixtures";

function textContent(path: string, text: string): Exclude<FileContentState, { kind: "conversation" }> {
  const bytes = new TextEncoder().encode(text);
  return {
    kind: "file", status: "ready", generation: 1, stale: false,
    context: { sessionId: ONLINE_SESSION_ID, source: "all", path },
    content: { kind: "preview", path, fileKind: "text", sizeBytes: String(bytes.length),
      version: "v1", offset: String(bytes.length), bytes, eof: true, previewTruncated: false },
  };
}

describe("文件打开后的原型展示与安全边界", () => {
  it("使用紧凑 SVG 关闭按钮、独立行号和预格式正文", () => {
    const html = renderToStaticMarkup(createElement(FileContentViewer, {
      content: textContent("src/a.ts", "const answer = 42;\n"), onClose: () => {},
    }));
    expect(html).toContain('aria-label="关闭文件查看"');
    expect(html).toMatch(/<svg[^>]*width="11"/);
    expect(html).toContain("关闭</button>");
    expect(html).toContain('data-file-line-numbers="1"');
    expect(html).toContain('data-file-preview-text="true"');
    expect(html).toContain("TypeScript");
    expect(html).toContain("只读预览");
    expect(html).toContain("font-size:12.5px");
    expect(html).toContain("white-space:pre");
  });

  it("Markdown、HTML 与 SVG 保持原文，不执行内容或加载其中的远端资源", () => {
    for (const path of ["README.md", "page.html", "image.svg"]) {
      const html = renderToStaticMarkup(createElement(FileContentViewer, {
        content: textContent(path, '<script>alert(1)</script>\n<img src="https://example.com/private">'),
        onClose: () => {},
      }));
      expect(html).toContain("&lt;script&gt;");
      expect(html).not.toContain("<script>");
      expect(html).not.toContain('<img src="https://');
    }
  });
});

describe("file reading controls", () => {
  it("uses the same native file scrollbar for Git and regular files",()=>{
    const html=renderToStaticMarkup(createElement(FileContentViewer,{
      content:{kind:"file",status:"ready",generation:1,stale:false,context:{sessionId:ONLINE_SESSION_ID,source:"git",path:"a.ts"},
        content:{kind:"git_diff",diff:{path:"a.ts",base:"head",kind:"text",diffLines:[],unavailable:null,truncated:false,comparison:{before:"old\n",after:"new\n"}}}},
      onClose:()=>{},
    }));
    expect(html).toContain('data-file-view-scrollbar="native"');
    expect(html).not.toContain('data-git-integrated-scrollbar="true"');
    expect(html).not.toContain('role="scrollbar"');
    const regular=renderToStaticMarkup(createElement(FileContentViewer,{content:textContent("a.ts","same"),onClose:()=>{}}));
    expect(regular).toContain('data-file-view-scrollbar="native"');
    expect(html).not.toContain("scrollbar-width:none");
    expect(html).toContain('data-comparison-fixed-panes="true"');
    expect(html).toContain('data-git-change-markers="true"');
    expect(regular).not.toContain('data-git-change-markers="true"');
  });
  it("keeps downloads above the scroll body and scrolls long lines horizontally", () => {
    const html = renderToStaticMarkup(createElement(FileContentViewer, {
      content: textContent("src/a.ts", "const value = '" + "x".repeat(300) + "';"),
      onClose: () => {}, onDownload: () => {},
    }));
    const download = html.indexOf('data-action="download-file"');
    const scroll = html.indexOf('data-file-view-scroll="true"');
    expect(download).toBeGreaterThan(-1);
    expect(download).toBeLessThan(scroll);
    expect(html).toContain('data-file-word-wrap="false"');
    expect(html).not.toContain('data-action="toggle-file-wrap"');
    expect(html).toContain('white-space:pre');
    expect(html).not.toContain('white-space:pre-wrap');
    expect(html).toContain('overflow-x:auto');
  });
});

import { useMemo } from "react";
import type { ReactElement } from "react";
import type { GitDiffView } from "../domain/session-files";
import { comparePatch, compareTexts } from "../domain/text-comparison";
import type { ComparisonLine } from "../domain/text-comparison";
import { Text, View, fontFamily, useTheme, useWindowDimensions } from "./theme";

/** Equal-width panes stay fixed; only their text scrolls horizontally. */
export function GitFileComparison({ diff }: { readonly diff: GitDiffView }): ReactElement {
  const {colors}=useTheme();
  const {width}=useWindowDimensions();
  const narrow=width<640;
  const comparison=useMemo(()=>diff.comparison === undefined
    ? {rows:comparePatch(diff.diffLines),simplified:false}
    : compareTexts(diff.comparison.before,diff.comparison.after),[diff.comparison,diff.diffLines]);
  const cell=(entry:ComparisonLine | null,side:"before" | "after",index:number):ReactElement=>(
    <div key={index} data-comparison-side={side} data-comparison-changed={String(entry?.changed ?? false)} data-comparison-line-index={String(index)}
      style={{display:"flex",height:24,boxSizing:"border-box",minWidth:"100%",width:"max-content",
        background:entry?.changed ? (side === "before" ? colors.dangerSoft : colors.okSoft) : entry === null ? colors.panel2 : colors.panel}}>
      <span aria-hidden={true} style={{position:"sticky",left:0,zIndex:1,minWidth:"3.5em",padding:"0 8px",boxSizing:"border-box",textAlign:"right",color:colors.ink3,userSelect:"none",flexShrink:0,background:entry?.changed ? (side === "before" ? colors.dangerSoft : colors.okSoft) : colors.panel2}}>{entry?.number ?? ""}</span>
      <pre style={{font:"inherit",margin:0,padding:"0 10px",whiteSpace:"pre",overflowWrap:"normal",minWidth:"max-content",flex:1,
        color:entry?.changed ? (side === "before" ? colors.danger : colors.ok) : colors.ink}}>{entry?.text ?? ""}</pre>
    </div>
  );
  const pane=(side:"before" | "after"):ReactElement=>(
    <section data-comparison-pane={side} style={{minWidth:0,overflow:"hidden",borderRight:side === "before" && !narrow ? `1px solid ${colors.line}` : undefined}}>
      <div style={{height:40,boxSizing:"border-box",padding:"8px 12px",whiteSpace:"nowrap",overflow:"hidden",textOverflow:"ellipsis",background:colors.panel2,borderBottom:`1px solid ${colors.line}`,color:side === "before" ? colors.danger : colors.ok,fontSize:12,fontWeight:600}}>
        {side === "before" ? "修改前 · "+(diff.base === "empty" ? "空文件" : "HEAD") : "修改后 · 当前工作文件"}
      </div>
      <div data-comparison-horizontal-scroll={side} tabIndex={0} role="region" aria-label={side === "before" ? "修改前内容，可横向滚动" : "修改后内容，可横向滚动"} style={{overflowX:"auto",minWidth:0}}>
        <div style={{minWidth:"100%",width:"max-content"}}>{comparison.rows.map((row,index)=>cell(row[side],side,index))}</div>
      </div>
    </section>
  );
  return <View dataSet={{gitComparison:diff.comparison === undefined ? "partial" : "full"}} style={{gap:8}}>
    {diff.comparison === undefined ? <Text style={{fontSize:12,color:colors.ink3}}>当前仅提供局部对比，未取得两份完整内容（主机不支持、文件过大或读取失败）。</Text> : null}
    {comparison.simplified ? <Text style={{fontSize:12,color:colors.ink3}}>改动范围较大，采用整段对齐；两侧仍保留完整原文。</Text> : null}
    <div style={{display:"flex",minWidth:0}}>
      <div data-comparison-fixed-panes="true" style={{display:"grid",gridTemplateColumns:narrow ? "minmax(0,1fr)" : "minmax(0,1fr) minmax(0,1fr)",flex:1,minWidth:0,border:`1px solid ${colors.line}`,borderRadius:10,overflow:"hidden",fontFamily:fontFamily.mono,fontSize:12.5,lineHeight:"24px"}}>
        {pane("before")}{pane("after")}
        {comparison.rows.length === 0 ? <div style={{gridColumn:"1 / -1",padding:20,color:colors.ink3}}>没有可展示的文本行。</div> : null}
      </div>

    </div>
    {diff.comparison !== undefined ? <Text style={{fontSize:11,color:colors.ink3}}>{`文件末尾换行：修改前${diff.comparison.before.endsWith("\n") ? "有" : "无"}，修改后${diff.comparison.after.endsWith("\n") ? "有" : "无"}`}</Text> : null}
  </View>;
}

import { useLayoutEffect, useState } from "react";
import type { ReactElement } from "react";
import { useTheme } from "./theme";

export interface GitChangeMarker {
  readonly top:number;
  readonly height:number;
  readonly added:boolean;
  readonly deleted:boolean;
}

/** Merge measured changed rows, including changes at the same position in both panes. */
export function mergeChangeMarkers(input:readonly GitChangeMarker[]):readonly GitChangeMarker[] {
  const merged:GitChangeMarker[]=[];
  for (const marker of [...input].sort((a,b)=>a.top-b.top)) {
    const last=merged[merged.length-1];
    if (last !== undefined && marker.top<=last.top+last.height+1) {
      merged[merged.length-1]={top:last.top,height:Math.max(last.top+last.height,marker.top+marker.height)-last.top,added:last.added || marker.added,deleted:last.deleted || marker.deleted};
    } else merged.push(marker);
  }
  return merged;
}

/** A transparent content-edge overlay; the browser still owns the scrollbar. */
export function GitChangeMarkers({scrollHost,contentKey}:{readonly scrollHost:HTMLElement | null;readonly contentKey:unknown}):ReactElement {
  const {colors}=useTheme();
  const [layout,setLayout]=useState({height:1,gutter:12,markers:[] as readonly GitChangeMarker[]});
  useLayoutEffect(()=>{
    if (scrollHost === null) return;
    const measure=():void=>{
      const origin=scrollHost.getBoundingClientRect().top+scrollHost.clientTop;
      const markers=Array.from(scrollHost.querySelectorAll<HTMLElement>('[data-comparison-changed="true"]')).map(element=>{
        const rect=element.getBoundingClientRect();
        return {top:scrollHost.scrollTop+rect.top-origin,height:rect.height,added:element.dataset["comparisonSide"] === "after",deleted:element.dataset["comparisonSide"] === "before"};
      });
      setLayout({height:Math.max(1,scrollHost.scrollHeight),gutter:Math.max(12,scrollHost.offsetWidth-scrollHost.clientWidth),markers:mergeChangeMarkers(markers)});
    };
    measure();
    const observer=typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    observer?.observe(scrollHost);
    if (scrollHost.firstElementChild !== null) observer?.observe(scrollHost.firstElementChild);
    window.addEventListener("resize",measure);
    return ()=>{observer?.disconnect();window.removeEventListener("resize",measure);};
  },[scrollHost,contentKey]);
  return <div data-git-change-markers="true" role="navigation" aria-label="Git 改动标记" style={{position:"absolute",top:0,bottom:0,right:layout.gutter,width:8,pointerEvents:"none",zIndex:2}}>
    {layout.markers.map((marker,index)=>{
      const kind=marker.added && marker.deleted ? "modified" : marker.added ? "added" : "deleted";
      const label=`${kind === "modified" ? "新增与删除" : kind === "added" ? "新增" : "删除"}，定位第 ${index+1} 处改动`;
      return <button key={`${marker.top}:${index}`} type="button" title={label} aria-label={label} data-git-change-marker={kind}
        onClick={()=>{
          if (scrollHost === null) return;
          scrollHost.scrollTop=Math.max(0,Math.min(scrollHost.scrollHeight-scrollHost.clientHeight,marker.top-(scrollHost.clientHeight-marker.height)/2));
        }}
        style={{position:"absolute",right:0,width:8,height:`${marker.height/layout.height*100}%`,minHeight:4,top:`clamp(0px, ${marker.top/layout.height*100}%, calc(100% - 4px))`,padding:0,border:0,background:"transparent",cursor:"pointer",pointerEvents:"auto"}}>
        <span aria-hidden={true} style={{position:"absolute",right:0,top:0,bottom:0,width:4,borderRadius:1,background:marker.added && marker.deleted ? `linear-gradient(to bottom, ${colors.danger} 50%, ${colors.ok} 50%)` : marker.added ? colors.ok : colors.danger}} />
      </button>;
    })}
  </div>;
}

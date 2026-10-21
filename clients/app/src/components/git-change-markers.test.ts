import { describe, expect, it } from "vitest";
import { mergeChangeMarkers } from "./GitChangeMarkers";

describe("native container change markers",()=>{
  it("combines same-position changes from both panes while retaining both colors",()=>{
    expect(mergeChangeMarkers([
      {top:164,height:24,added:true,deleted:false},
      {top:140,height:24,added:false,deleted:true},
      {top:140,height:24,added:true,deleted:false},
      {top:400,height:24,added:false,deleted:true},
    ])).toEqual([
      {top:140,height:48,added:true,deleted:true},
      {top:400,height:24,added:false,deleted:true},
    ]);
  });
  it("keeps additions, deletions and separate stacked-pane positions distinct",()=>{
    const items=[{top:60,height:24,added:false,deleted:true},{top:700,height:24,added:true,deleted:false}];
    expect(mergeChangeMarkers(items)).toEqual(items);
    expect(mergeChangeMarkers([])).toEqual([]);
  });
});

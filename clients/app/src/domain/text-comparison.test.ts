import { describe, expect, it } from "vitest";
import { compareTexts, comparePatch } from "./text-comparison";

describe("complete file comparison", () => {
  it("aligns replacements while preserving unchanged lines and original numbers", () => {
    const {rows} = compareTexts("top\nold\nbottom\n", "top\nnew\nextra\nbottom\n");
    expect(rows[0]?.before?.changed).toBe(false);
    expect(rows[1]).toEqual({before:{number:2,text:"old",changed:true},after:{number:2,text:"new",changed:true}});
    expect(rows[2]).toEqual({before:null,after:{number:3,text:"extra",changed:true}});
    expect(rows[3]).toEqual({before:{number:3,text:"bottom",changed:false},after:{number:4,text:"bottom",changed:false}});
  });
  it("preserves full additions and deletions including empty and missing final newlines", () => {
    expect(compareTexts("", "a\nb").rows.map(row => row.after?.text)).toEqual(["a","b"]);
    expect(compareTexts("a\n", "").rows).toEqual([{before:{number:1,text:"a",changed:true},after:null}]);
    expect(compareTexts("", "").rows).toEqual([]);
  });
  it("bounds alignment work for large replacements without dropping text", () => {
    const a=Array.from({length:1001},(_,i)=>`a${i}`),b=Array.from({length:1001},(_,i)=>`b${i}`);
    const result=compareTexts(a.join("\n"),b.join("\n"));
    expect(result.simplified).toBe(true);
    expect(result.rows.map(row=>row.before?.text)).toEqual(a);
    expect(result.rows.map(row=>row.after?.text)).toEqual(b);
  });
  it("handles Windows line endings and highlights a changed final newline", () => {
    expect(compareTexts("same\n", "same\r\n").rows[0]?.after?.changed).toBe(false);
    expect(compareTexts("same\n", "same").rows[0]?.after?.changed).toBe(true);
  });
  it("keeps partial patch numbering without inventing omitted file content", () => {
    const rows=comparePatch(["@@ -20,2 +30,2 @@"," same","-old","+new"]);
    expect(rows[0]?.before?.number).toBe(20);
    expect(rows[0]?.after?.number).toBe(30);
    expect(rows[1]?.before?.text).toBe("old");
    expect(rows[2]?.after?.text).toBe("new");
  });
});

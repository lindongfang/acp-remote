export interface ComparisonLine {
  readonly number: number;
  readonly text: string;
  readonly changed: boolean;
}
export interface ComparisonRow {
  readonly before: ComparisonLine | null;
  readonly after: ComparisonLine | null;
}

/** Align complete texts without unbounded work on large, unrelated files. */
export function compareTexts(before: string, after: string): { readonly rows: readonly ComparisonRow[]; readonly simplified: boolean } {
  const lines = (text: string): string[] => {
    if (text === "") return [];
    // CRLF is displayed as one line break; source strings remain untouched.
    const result = text.split("\n").map(value => value.endsWith("\r") ? value.slice(0,-1) : value);
    if (result[result.length - 1] === "") result.pop();
    return result;
  };
  const old = lines(before), current = lines(after);
  let prefix = 0, suffix = 0;
  while (prefix < old.length && prefix < current.length && old[prefix] === current[prefix]) prefix++;
  while (suffix < old.length - prefix && suffix < current.length - prefix &&
    old[old.length - suffix - 1] === current[current.length - suffix - 1]) suffix++;
  const rows: ComparisonRow[] = [];
  const line = (text: string, number: number, changed: boolean): ComparisonLine => ({ text, number, changed });
  for (let i = 0; i < prefix; i++) rows.push({before:line(old[i] ?? "",i+1,false),after:line(current[i] ?? "",i+1,false)});
  const n = old.length - prefix - suffix, m = current.length - prefix - suffix;
  const simplified = n * m > 1_000_000;
  const table = simplified ? null : Array.from({length:n+1}, () => new Uint32Array(m+1));
  if (table !== null) {
    for (let i=n-1;i>=0;i--) for (let j=m-1;j>=0;j--) {
      const row = table[i], next = table[i+1];
      if (row === undefined || next === undefined) continue;
      row[j] = old[prefix+i] === current[prefix+j] ? (next[j+1] ?? 0)+1 : Math.max(next[j] ?? 0,row[j+1] ?? 0);
    }
  }
  let i=0,j=0;
  let removed: ComparisonLine[] = [], added: ComparisonLine[] = [];
  const flush = (): void => {
    for (let k=0;k<Math.max(removed.length,added.length);k++) rows.push({before:removed[k] ?? null,after:added[k] ?? null});
    removed=[];added=[];
  };
  while (i<n || j<m) {
    if (!simplified && i<n && j<m && old[prefix+i] === current[prefix+j]) {
      flush();
      rows.push({before:line(old[prefix+i] ?? "",prefix+i+1,false),after:line(current[prefix+j] ?? "",prefix+j+1,false)});
      i++;j++;
    } else if (i<n && (simplified || j===m || (table?.[i+1]?.[j] ?? 0) >= (table?.[i]?.[j+1] ?? 0))) {
      removed.push(line(old[prefix+i] ?? "",prefix+i+1,true));i++;
    } else { added.push(line(current[prefix+j] ?? "",prefix+j+1,true));j++; }
  }
  flush();
  for (let k=suffix;k>0;k--) rows.push({before:line(old[old.length-k] ?? "",old.length-k+1,false),after:line(current[current.length-k] ?? "",current.length-k+1,false)});
  if (before !== "" && after !== "" && before.endsWith("\n") !== after.endsWith("\n")) {
    for (let k=0;k<rows.length;k++) {
      const row=rows[k];
      if (row === undefined) continue;
      rows[k]={before:row.before?.number === old.length ? {...row.before,changed:true} : row.before,
        after:row.after?.number === current.length ? {...row.after,changed:true} : row.after};
    }
  }
  return {rows,simplified};
}

/** Old hosts provide only patch fragments. Keep real line numbers and missing regions explicit. */
export function comparePatch(lines: readonly string[]): readonly ComparisonRow[] {
  const rows: ComparisonRow[]=[];
  let old=0,current=0;
  for (const text of lines) {
    const hunk=/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(text);
    if (hunk !== null) { old=Number(hunk[1]);current=Number(hunk[2]);continue; }
    if (text.startsWith("---") || text.startsWith("+++")) continue;
    if (text.startsWith("-")) rows.push({before:{number:old++,text:text.slice(1),changed:true},after:null});
    else if (text.startsWith("+")) rows.push({before:null,after:{number:current++,text:text.slice(1),changed:true}});
    else if (text.startsWith(" ")) rows.push({before:{number:old++,text:text.slice(1),changed:false},after:{number:current++,text:text.slice(1),changed:false}});
  }
  return rows;
}

import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";

// A v1 history cursor has 108 wire bytes, starting with version 1 and
// direction 0 or 1. This superset detects its complete unpadded base64url
// representation even when embedded in a longer string.
const cursor = /AQ[A-H][A-Za-z0-9_-]{141}/g;
const marker = /(?<![A-Za-z0-9])[UA][0-9]{4}(?![A-Za-z0-9])/g;
const root = process.argv[2];
if (!root) throw new Error("Usage: node log-redaction.mjs <run-log-directory>");
let files = 0;
let cursorMatches = 0;
let markerMatches = 0;
let privateChildMatches = 0;
for (const entry of await readdir(root, { withFileTypes: true, recursive: true })) {
  if (!entry.isFile() || !entry.name.endsWith(".log")) continue;
  const bytes = await readFile(join(entry.parentPath, entry.name));
  const encoding = bytes[0] === 255 && bytes[1] === 254 ? "utf16le" : "utf8";
  const log = bytes.toString(encoding);
  cursorMatches += [...log.matchAll(cursor)].length;
  markerMatches += [...log.matchAll(marker)].length;
  privateChildMatches += [...log.matchAll(/HN029-NATIVE-CONTENT-MARKER|hn029-native-cursor-\d+/g)].length;
  files++;
}
console.log(JSON.stringify({ files, cursorMatches, markerMatches, privateChildMatches }));
if (files === 0 || cursorMatches || markerMatches || privateChildMatches) process.exitCode = 1;

import '../../src/styles.css';
import { createRawInspector, type RawInspector } from '../../src/views/raw-inspector';
import { RawJsonDocument } from '../../src/views/raw-json';

type Vector = 'members' | 'controls' | 'huge-number' | 'long-key' | 'last-page';
const host = document.querySelector<HTMLDivElement>('#visual-host')!;
let current: RawInspector | null = null;
const controls = 'LEFT ' + String.fromCodePoint(0x9b, 0xad, 0x202e, 0x2066, 0x200d, 0xfe0f, 0xe0100) + ' RIGHT';
function vector(name: Vector): string {
  switch (name) {
    case 'members': return '{"z":1,"a":2,"z":3,"' + 'K'.repeat(40 * 1024) + 'KEY_END":4,"control ' + String.fromCodePoint(0x202e) + '":5}';
    case 'controls': return JSON.stringify(controls);
    case 'huge-number': return '-' + '9'.repeat(8193) + 'e+99999';
    case 'long-key': return '{"' + 'K'.repeat(40 * 1024) + 'KEY_END":0}';
    case 'last-page': return '[' + Array.from({ length: 201 }, (_, i) => JSON.stringify('MEMBER_' + i + (i === 200 ? '_LAST' : ''))).join() + ']';
  }
}
function reset(): void { current?.dispose(); current = null; }
function show(name: Vector): object {
  if (!['members', 'controls', 'huge-number', 'long-key', 'last-page'].includes(name)) throw new Error('fixed-synthetic-vector-only');
  reset();
  const raw = RawJsonDocument.parse(new TextEncoder().encode(vector(name)));
  current = createRawInspector(document, raw);
  current.element.id = 'independent-visual-inspector';
  host.append(current.element);
  return { vector: name, realDocument: document instanceof Document, rootOwner: current.element.ownerDocument === document, rawBytes: raw.tokenBytes(0).length, mounted: host.children.length };
}
const api = Object.freeze({ show, reset });
(window as Window & { independentVisual?: typeof api }).independentVisual = api;

import { createRawInspector } from "../../src/views/raw-inspector";
import type { RawJsonDocument } from "../../src/views/raw-json";

declare const owner: Document;
declare const raw: RawJsonDocument;
const inspector = createRawInspector(owner, raw);
const element: HTMLElement = inspector.element;
void element;
inspector.dispose();
// @ts-expect-error RawJsonDocument has private ownership; a plain object is not a raw document.
createRawInspector(owner, {});
// @ts-expect-error The returned owned element reference is read-only.
inspector.element = owner.createElement("div");

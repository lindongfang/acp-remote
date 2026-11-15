import type { ProjectedItem, ProjectionRecord, ProjectionEvidence } from "../../src/views/projection";
import type { ProjectionCollection } from "../../src/views/projection-collection";
declare const item: ProjectedItem;
declare const record: ProjectionRecord;
declare const evidence: ProjectionEvidence;
declare const collection: ProjectionCollection;
const goodRevision: bigint = item.revision; void goodRevision;
const goodItems: readonly ProjectedItem[] = collection.items(); void goodItems;
if (record.type === "item_projection") { const itemBranch: ProjectedItem = record; void itemBranch; }
else { const evidenceBranch: ProjectionEvidence = record; void evidenceBranch; }
// @ts-expect-error validated metadata is deeply read-only
item.revision = 2n;
// @ts-expect-error field knowledge cannot be mutated
item.fields[0]!.knowledge = "known";
// @ts-expect-error raw reference cannot be mutated
item.fields[0]!.raw!.append = true;
// @ts-expect-error fields are read-only, not an owned mutable list
item.fields.push(item.fields[0]!);
// @ts-expect-error collection snapshot is read-only
collection.items().push(item);
// @ts-expect-error evidence must be routed separately
collection.accept(evidence);

import{readViewRecordBody,type CompletedViewRecord}from'../../src/views/records';import{ViewContentStore,type FieldMaterial}from'../../src/views/content-store';import type{ProjectedItem}from'../../src/views/projection';import type{StructuredTimeline}from'../../src/views/timeline';
export function positive(r:CompletedViewRecord,s:ViewContentStore,i:ProjectedItem,t:StructuredTimeline){const b=readViewRecordBody(r,'original_raw');b.bytes[0]=1;const m:FieldMaterial=s.field(i,'content');const d=s.rawDocument(i);t.refresh();return[m,d,b.recordId,s.items(i.segmentId)]as const;}
export function negatives(r:CompletedViewRecord,s:ViewContentStore,i:ProjectedItem,m:FieldMaterial){
 // @ts-expect-error fixed kind only
 readViewRecordBody(r,'status');
 // @ts-expect-error owned header immutable
 readViewRecordBody(r,'original_raw').recordId=2n;
 // @ts-expect-error field material immutable
 m.referencesReady=true;
 // @ts-expect-error evidence array immutable
 m.evidence.push(m.evidence[0]!);
 // @ts-expect-error no native action port
 s.cancel(i);
 // @ts-expect-error document interface is read only
 s.rawDocument(i)?.replace('{}');
}

import {decodeProjectionRecord, PROJECTION_FIELDS, type ProjectedItem, type ProjectionField, type ProjectionKnowledge} from '../../src/views/projection';
import type {CompletedViewRecord} from '../../src/views/records';
export const STREAM='AAECAwQFBgcICQoLDA0ODw',OTHER='Dw4NDAsKCQgHBgUEAwIBAA',SEGMENT='AAAAAAAAAAAAAAAAAAAAAA',SEGMENT2='AQEBAQEBAQEBAQEBAQEBAQ';
export const encode=(s:string):Uint8Array=>new TextEncoder().encode(s);
export interface Ref {rawRecordId:string;byteStart:string;byteEnd:string;append:boolean}
export interface F {field:string;knowledge:ProjectionKnowledge;raw:Ref|null;enumCode:number|null;evidenceCount:string}
export function rec(kind:'original_raw'|'item_projection',id:bigint,text:string,stream=STREAM):CompletedViewRecord{const b=encode(text);return{streamId:stream,kind,recordId:id,length:BigInt(b.length),bytes:()=>b.slice()};}
export function ref(id:bigint,raw:string,literal=raw,append=false,startOccurrence=0):Ref{let pos=-1;for(let i=0;i<=startOccurrence;i++)pos=raw.indexOf(literal,pos+1);if(pos<0)throw Error('independent fixture literal');const start=encode(raw.slice(0,pos)).length;return{rawRecordId:String(id),byteStart:String(start),byteEnd:String(start+encode(literal).length),append};}
export function item(anchor=1n,revision=1n,rawId=11n,changes:Partial<Record<ProjectionField,Partial<F>>>={},options:{segment?:string;stream?:string;kind?:string;position?:bigint}={}):CompletedViewRecord{
 const fields=PROJECTION_FIELDS.map(field=>({field,knowledge:'unknown',raw:null,enumCode:null,evidenceCount:'0',...changes[field]}));
 return rec('item_projection',100n+revision,JSON.stringify({type:'item_projection',segmentId:options.segment??SEGMENT,anchorId:String(anchor),firstPosition:String(options.position??anchor),revision:String(revision),rawRecordId:String(rawId),kind:options.kind??'tool',fields}),options.stream??STREAM);
}
export function evidence(anchor:bigint,revision:bigint,index:bigint,raw:Ref,field:ProjectionField='content',segment=SEGMENT):CompletedViewRecord{return rec('item_projection',900n,JSON.stringify({type:'projection_evidence',segmentId:segment,anchorId:String(anchor),revision:String(revision),field,index:String(index),raw}));}
export function decode(r:CompletedViewRecord):ProjectedItem{const v=decodeProjectionRecord(r);if(v.type!=='item_projection')throw Error('fixture item expected');return v;}

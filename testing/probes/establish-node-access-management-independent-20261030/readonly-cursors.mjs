import {load,auth,counts,assert,delay,json} from './lib.mjs';
const {n,second,A,B,secondA}=await load('state.json');const connections=[];
try{
 const a=await auth(n,A), b=await auth(n,B), other=await auth(second,secondA);connections.push(a,b,other);
 assert((await a.subscribe()).ok,'A subscribe');assert((await b.subscribe()).ok,'B subscribe');await delay(100);
 const before=counts(n);
 for(let i=0;i<10;i++){assert((await a.command('node.info',{})).ok,'readonly info '+i);await delay(160);assert((await a.command('device.list',{cursor:null})).ok,'readonly list '+i);await delay(160)}
 const invalid=await a.command('device.list',{});assert(!invalid.ok&&invalid.error.code==='invalid_request','malformed list fails closed');
 const after=counts(n);assert(before.audit===after.audit&&before.results===after.results,'20 readonly frames plus subscriptions/invalid list do not grow audit/results');
 const issued=performance.now();const first=await a.command('device.list',{cursor:null});const cursor=first.result.nextCursor;assert(typeof cursor==='string','cursor issued');
 const crossActor=await b.command('device.list',{cursor});assert(!crossActor.ok&&crossActor.error.code==='stale_cursor','cross actor cursor rejected');
 const crossNode=await other.command('device.list',{cursor});assert(!crossNode.ok&&crossNode.error.code==='stale_cursor','cross node cursor rejected');
 const modified=(cursor[0]==='0'?'1':'0')+cursor.slice(1);const tampered=await a.command('device.list',{cursor:modified});assert(!tampered.ok&&tampered.error.code==='stale_cursor','tampered cursor rejected');
 console.log('CURSOR_WAIT bounded59/60s');await delay(Math.max(0,59100-(performance.now()-issued)));
 const elapsedBefore=performance.now()-issued;const at59=await a.command('device.list',{cursor});assert(at59.ok,'cursor valid in59th second');
 await delay(Math.max(0,60200-(performance.now()-issued)));const elapsedAfter=performance.now()-issued;
 const at60=await a.command('device.list',{cursor});assert(!at60.ok&&at60.error.code==='stale_cursor','cursor invalid after60 seconds');
 const final=counts(n);assert(before.audit===final.audit&&before.results===final.results,'all cursor/read frames preserve audit/results count');
 await json('readonly-cursors.json',{before,after,final,readCommands:20,subscriptionConnections:2,invalidList:invalid.error.code,crossActor:crossActor.error.code,crossNode:crossNode.error.code,tampered:tampered.error.code,at59:{elapsedMs:elapsedBefore,ok:at59.ok},at60:{elapsedMs:elapsedAfter,code:at60.error.code},noticeShapesValid:[...a.notices,...b.notices].every(m=>Object.keys(m).sort().join('/')==='revision/type/v')});
}finally{for(const c of connections)c.close()}

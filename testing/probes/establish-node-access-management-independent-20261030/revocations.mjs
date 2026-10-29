import {load,auth,counts,assert,delay,json,db,pair,binary} from './lib.mjs';
import {execFileSync} from 'node:child_process';
import {randomId} from './wire.mjs';
const {n,A,B}=await load('state.json');const live=[];
const wait=async(promise,ms=5000)=>{let t;try{return await Promise.race([promise,new Promise((_,no)=>t=setTimeout(()=>no(Error('bounded close/notification timeout')),ms))])}finally{clearTimeout(t)}};
const client=id=>{const d=db(n);try{return d.prepare('SELECT status,auth_generation,revoked_at FROM clients WHERE client_id=?').get(id)}finally{d.close()}};
try{
 const a=await auth(n,A), b1=await auth(n,B), b2=await auth(n,B);live.push(a,b1,b2);
 for(const c of live)assert((await c.subscribe()).ok,'connection subscribed');await delay(150);
 const before=counts(n),stateBefore=client(B.clientId);let database=db(n);
 database.exec("CREATE TRIGGER independent_fail_revoke BEFORE INSERT ON security_audit WHEN NEW.action='device.revoke' BEGIN SELECT RAISE(ABORT,'INDEPENDENT_FAULT'); END");database.close();
 let failed;
 try{failed=await a.command('device.revoke',{clientId:B.clientId});assert(!failed.ok&&failed.error.code==='storage_unavailable','real WSS revoke storage failure is explicit');assert(JSON.stringify(client(B.clientId))===JSON.stringify(stateBefore),'failure preserves B qualification');const after=counts(n);assert(before.audit===after.audit&&before.results===after.results&&before.revision===after.revision,'failure atomically rolls back audit/result/revision');assert((await b1.command('node.info',{})).ok&&(await b2.command('device.list',{cursor:null})).ok,'both B connections remain valid after failed revoke');}
 finally{database=db(n);database.exec('DROP TRIGGER independent_fail_revoke');database.close()}
 const firstCursor=(await a.command('device.list',{cursor:null})).result.nextCursor;
 const C=await pair(n,73);
 const staleAfterTrust=await a.command('device.list',{cursor:firstCursor});assert(!staleAfterTrust.ok&&staleAfterTrust.error.code==='stale_cursor','real new trust invalidates old cursor');
 const refreshed=await a.command('device.list',{cursor:null});assert(refreshed.ok,'restart listing at first page after new trust');
 database=db(n);const D=database.prepare("SELECT client_id FROM clients WHERE display_name LIKE 'Independent paging record %' AND status='trusted' LIMIT 1").get().client_id;database.close();
 const request=randomId();const revokeD=await a.command('device.revoke',{clientId:D},request);assert(revokeD.ok,'real A revokes synthetic D');
 const staleAfterRevoke=await a.command('device.list',{cursor:refreshed.result.nextCursor});assert(!staleAfterRevoke.ok&&staleAfterRevoke.error.code==='stale_cursor','real revoke invalidates old cursor');assert((await a.command('device.list',{cursor:null})).ok,'restart first page after revoke');
 const repeat=await a.command('device.revoke',{clientId:D},request);assert(repeat.ok&&JSON.stringify(repeat.result)===JSON.stringify(revokeD.result),'same request replays exact result');
 const conflict=await a.command('device.revoke',{clientId:B.clientId},request);assert(!conflict.ok&&conflict.error.code==='request_conflict','same request changed payload conflicts');
 assert((await a.command('device.revoke',{clientId:D})).ok,'new request same revoked ID stays revoked');
 const during=[];
 const work=async(c,action,payload)=>{for(let i=0;i<4;i++){try{const r=await c.command(action,payload);during.push({ok:r.ok,action})}catch{during.push({closed:true,action});break}await delay(30)}};
 const w1=work(b1,'node.info',{}), w2=work(b2,'device.list',{cursor:null});
 const revokeB=await a.command('device.revoke',{clientId:B.clientId});assert(revokeB.ok,'A revoke B committed');await Promise.allSettled([w1,w2]);
 const closed=await Promise.all([wait(b1.closedPromise),wait(b2.closedPromise)]);assert(closed.every(e=>e.code===4410),'both real subscribed B connections close4410');
 const reconnect=await auth(n,B);assert(reconnect.closed?.code===4401,'revoked original B proof reconnect rejected4401');reconnect.wire.destroy();
 const expected=counts(n).revision;assert(expected===before.revision+3,'exactly three state mutations: new trust, D revoke, B revoke');
 for(let i=0;i<50&&!a.notices.some(m=>m.revision===expected);i++)await delay(50);
 assert(a.notices.some(m=>m.revision===expected),'final revision delivered after three changes without fourth trigger');
 assert(a.notices.every(m=>Object.keys(m).sort().join('/')==='revision/type/v'),'notifications contain only v/type/revision');
 const fresh=await a.command('device.list',{cursor:null});assert(fresh.ok&&fresh.result.revision===expected,'active query reads latest revision');
 const observations={before,failure:{code:failed.error.code,preservedClient:true,preservedCounts:true,bothBStillValid:true},newTrustCursor:staleAfterTrust.error.code,revokeCursor:staleAfterRevoke.error.code,replay:true,conflict:conflict.error.code,duplicateNewRequest:true,bClosed:closed,originalBReconnect:reconnect.closed.code,during,threeMutationRevision:expected,notices:a.notices,latestListRevision:fresh.result.revision};
 await json('revocations-before-self.json',observations);
 let selfResult=null;try{selfResult=await a.command('device.revoke',{clientId:A.clientId})}catch{}const selfClose=await wait(a.closedPromise);assert(selfClose.code===4410,'self revoke closes A4410 without requiring a success response');assert(client(A.clientId).status==='revoked','self revoked stored state');
 const again=await auth(n,A);assert(again.closed?.code===4401,'self revoked original key reconnect rejected');again.wire.destroy();
 execFileSync(binary,['--config',n.config,'device','list'],{timeout:5000,windowsHide:true,stdio:'pipe'});assert(true,'owner list works after A self revoke');
 const Recovery=await pair(n,74);const restored=await auth(n,Recovery);live.push(restored);assert((await restored.command('device.list',{cursor:null})).ok,'owner can approve fresh identity and restore management');
 await json('revocations-final.json',{...observations,self:{close:selfClose,successResponseObserved:selfResult?.ok===true,storedRevoked:true,originalProofRejected:true,ownerList:true,ownerRepaired:true},Recovery,C,finalCounts:counts(n)});
 console.log('REVOCATIONS_COMPLETE');
}finally{for(const c of live)c.close()}

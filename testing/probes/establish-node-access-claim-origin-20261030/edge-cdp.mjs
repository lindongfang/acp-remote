import fs from 'node:fs/promises';
export const root='D:/Project/acp-remote/.tmp/claim-origin-acceptance-20261030/';
export async function connect(){
 const [port,browserPath]=(await fs.readFile(root+'edge-profile/DevToolsActivePort','utf8')).trim().split(/\r?\n/);
 const socket=new WebSocket('ws://127.0.0.1:'+port+browserPath);await new Promise((ok,no)=>{socket.onopen=ok;socket.onerror=no;});
 let id=0;const pending=new Map();socket.onmessage=e=>{const m=JSON.parse(e.data);if(m.id&&pending.has(m.id)){const p=pending.get(m.id);pending.delete(m.id);m.error?p.no(Error(m.error.message)):p.ok(m.result);}};
 const send=(method,params={},sessionId)=>new Promise((ok,no)=>{const serial=++id;pending.set(serial,{ok,no});socket.send(JSON.stringify({id:serial,method,params,sessionId}));setTimeout(()=>{if(pending.delete(serial))no(Error('CDP deadline '+method));},12000).unref();});
 const targets=await send('Target.getTargets');const target=targets.targetInfos.find(x=>x.type==='page');if(!target)throw Error('owned Edge page missing');
 const {sessionId}=await send('Target.attachToTarget',{targetId:target.targetId,flatten:true});
 const evaluate=async expression=>{const r=await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true},sessionId);if(r.exceptionDetails)throw Error('owned browser evaluation failed: '+r.exceptionDetails.text);return r.result.value;};
 return {send,sessionId,evaluate,close:()=>socket.close()};
}

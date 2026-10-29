export async function connectPrivate(){
 const version=await (await fetch('http://127.0.0.1:9338/json/version')).json();
 const ws=new WebSocket(version.webSocketDebuggerUrl);
 await new Promise((resolve,reject)=>{ws.addEventListener('open',resolve,{once:true});ws.addEventListener('error',reject,{once:true})});
 let serial=0;const pending=new Map();
 ws.addEventListener('message',event=>{const m=JSON.parse(event.data);if(m.id&&pending.has(m.id)){const p=pending.get(m.id);pending.delete(m.id);m.error?p.reject(Error(JSON.stringify(m.error))):p.resolve(m.result)}});
 function send(method,params={},sessionId){return new Promise((resolve,reject)=>{const id=++serial;pending.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params,sessionId}));setTimeout(()=>{if(pending.delete(id))reject(Error('CDP timeout '+method))},10000).unref()})}
 const targets=(await send('Target.getTargets')).targetInfos.filter(x=>x.type==='page'&&x.url.startsWith('https://faraday.tail74842e.ts.net:30453'));
 if(targets.length!==1){ws.close();throw Error('Expected one dedicated private target')}
 const {sessionId}=await send('Target.attachToTarget',{targetId:targets[0].targetId,flatten:true});
 async function evaluate(expression){const r=await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true},sessionId);if(r.exceptionDetails)throw Error(r.exceptionDetails.text);return r.result.value}
 return {send,evaluate,sessionId,close:()=>ws.close()};
}

export async function connect(name){
 const port=name==='A'?9338:9339;
 const version=await(await fetch(`http://127.0.0.1:${port}/json/version`)).json();const ws=new WebSocket(version.webSocketDebuggerUrl);
 await new Promise((res,rej)=>{ws.addEventListener('open',res,{once:true});ws.addEventListener('error',rej,{once:true})});
 let seq=0;const pending=new Map(),network=[];ws.addEventListener('message',e=>{const m=JSON.parse(e.data);if(m.method==='Network.responseReceived'){const r=m.params.response;network.push({url:r.url,status:r.status,remoteIPAddress:r.remoteIPAddress,remotePort:r.remotePort,protocol:r.protocol,securityState:r.securityState,certificateIssuer:r.securityDetails?.issuer})}if(m.id&&pending.has(m.id)){const p=pending.get(m.id);pending.delete(m.id);m.error?p.reject(Error(JSON.stringify(m.error))):p.resolve(m.result)}});
 function send(method,params={},sessionId){return new Promise((resolve,reject)=>{const id=++seq;pending.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params,sessionId}));setTimeout(()=>{if(pending.delete(id))reject(Error('Timeout '+method))},15000).unref()})}
 async function attach(targetId){const {sessionId}=await send('Target.attachToTarget',{targetId,flatten:true});await send('Page.enable',{},sessionId);return {sessionId,evaluate:async expression=>{const r=await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true},sessionId);if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value}}}
 const targets=(await send('Target.getTargets')).targetInfos.filter(t=>t.type==='page'&&t.url.startsWith('https://faraday.tail74842e.ts.net:30453'));
 return {send,attach,targets,network,close:()=>ws.close()};
}

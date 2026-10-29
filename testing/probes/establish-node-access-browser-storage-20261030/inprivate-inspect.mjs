const version=await (await fetch('http://127.0.0.1:9338/json/version')).json();
const ws=new WebSocket(version.webSocketDebuggerUrl);
await new Promise((resolve,reject)=>{ws.addEventListener('open',resolve,{once:true});ws.addEventListener('error',reject,{once:true})});
let serial=0;const pending=new Map();
ws.addEventListener('message',event=>{const m=JSON.parse(event.data);if(m.id&&pending.has(m.id)){const {resolve,reject}=pending.get(m.id);pending.delete(m.id);m.error?reject(Error(JSON.stringify(m.error))):resolve(m.result)}});
function send(method,params={},sessionId){return new Promise((resolve,reject)=>{const id=++serial;pending.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params,sessionId}));setTimeout(()=>{if(pending.delete(id))reject(Error('CDP timeout '+method))},10000).unref()})}
try{
 const contexts=await send('Target.getBrowserContexts');
 const targets=await send('Target.getTargets');
 console.log(JSON.stringify({browser:version.Browser,contexts:contexts.browserContextIds,targets:targets.targetInfos.filter(x=>x.type==='page').map(x=>({targetId:x.targetId,type:x.type,url:x.url,title:x.title,browserContextId:x.browserContextId}))},null,2));
}finally{ws.close()}

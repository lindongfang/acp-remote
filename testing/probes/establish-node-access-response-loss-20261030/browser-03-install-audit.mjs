const t=await taskSpace(10),p=t.page(globalThis.__lossPage??'p1');
const source=`
window.__recoveryAudit={http:[],hello:[],authenticated:[],socketCloses:[]};
const originalFetch=window.fetch.bind(window);
window.fetch=async(...args)=>{const r=await originalFetch(...args);const u=String(args[0]);if(u.includes('/api/access/v1/pair/')){let error=null;try{if(r.status!==200)error=(await r.clone().json()).error;}catch{}window.__recoveryAudit.http.push({path:u,status:r.status,error});}return r;};
const NativeWebSocket=window.WebSocket;
window.WebSocket=class extends NativeWebSocket{
 constructor(...args){super(...args);this.addEventListener('message',e=>{try{const m=JSON.parse(e.data);if(m.type==='authenticated')window.__recoveryAudit.authenticated.push({clientId:m.clientId});}catch{}});this.addEventListener('close',e=>window.__recoveryAudit.socketCloses.push({code:e.code}));}
 send(data){try{const m=JSON.parse(data);if(m.type==='hello'){const h={clientId:m.clientId,publicKeySha256:null};window.__recoveryAudit.hello.push(h);crypto.subtle.digest('SHA-256',new TextEncoder().encode(m.clientPublicKey)).then(b=>h.publicKeySha256=Array.from(new Uint8Array(b),x=>x.toString(16).padStart(2,'0')).join(''));}}catch{}return super.send(data);}
};
`;
if(globalThis.__auditLive){await p.evaluate(source);console.log({liveAuditInstalled:true});}
else {const result=await p.cdp('Page.addScriptToEvaluateOnNewDocument',{source});console.log({auditInstalled:true,identifier:result.identifier});}

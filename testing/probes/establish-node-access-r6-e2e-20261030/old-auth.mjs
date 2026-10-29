import {connect} from './cdp.mjs';const fs=await import('node:fs/promises'),label=process.argv[2]??'revoked',root=process.env.ACP_E2E_ROOT??'D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030';const c=await connect('B');
try{let p;for(const t of c.targets){const candidate=await c.attach(t.targetId);if(await candidate.evaluate(`!!window.__saved`)){p=candidate;break}}if(!p)throw Error('Original B key not in page memory');
 const result=await p.evaluate(`(async()=>{
 const row=window.__saved;const enc=v=>btoa(String.fromCharCode(...new Uint8Array(v))).replace(/=/g,'').replace(/\\+/g,'-').replace(/\\//g,'_');
 const hello={v:1,type:'hello',clientId:row.clientId,clientPublicKey:row.publicKey,clientNonce:enc(crypto.getRandomValues(new Uint8Array(32)))};
 return await new Promise((resolve,reject)=>{let answered=false;const ws=new window.__NativeWebSocket(location.origin.replace('https:','wss:')+'/api/access/v1/control',['acpr.access.v1']);const timer=setTimeout(()=>{ws.close();reject(Error('Old B auth timeout'))},10000);
 ws.onopen=()=>ws.send(JSON.stringify(hello));ws.onmessage=async e=>{const m=JSON.parse(e.data);if(m.type==='challenge'){
 if(m.nodeId!==row.nodeId||m.origin!==row.origin||m.nodePublicKey!==row.nodePublicKey){ws.close();return reject(Error('Node binding mismatch'))}
 const binding={v:1,nodeId:m.nodeId,nodePublicKey:m.nodePublicKey,origin:m.origin,connectionId:m.connectionId,challengeId:m.challengeId,nonce:m.nonce,iat:m.iat,exp:m.exp,clientId:row.clientId,clientPublicKey:row.publicKey,clientNonce:hello.clientNonce};
 const h=enc(new TextEncoder().encode(JSON.stringify({alg:'ES256',typ:'acpr-auth-client+jwt'})));const payload=enc(new TextEncoder().encode(JSON.stringify(binding)));const signing=h+'.'+payload;const sig=await crypto.subtle.sign({name:'ECDSA',hash:'SHA-256'},row.privateKey,new TextEncoder().encode(signing));answered=true;ws.send(JSON.stringify({v:1,type:'proof',proof:signing+'.'+enc(sig)}));
 }else if(m.type==='authenticated'){ws.close();clearTimeout(timer);resolve({unexpectedAuthenticated:true,proofSent:answered})}};
 ws.onclose=e=>{clearTimeout(timer);resolve({code:e.code,reason:e.reason,clean:e.wasClean,proofSent:answered,unexpectedAuthenticated:false})};ws.onerror=()=>{};
 });
 })()`);
 await fs.writeFile(`${root}/evidence/B-old-auth-${label}.json`,JSON.stringify(result,null,2));console.log(result);
}finally{c.close()}

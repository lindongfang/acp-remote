import fs from 'node:fs/promises';
const root='D:/Project/acp-remote/.tmp/runtime-secret-acceptance-20261030/';
const t=await taskSpace(11),p=t.page('p1');
await p.waitForFunction(()=>document.body.innerText.includes('NODE CONTROL / AUTHENTICATED'),undefined,{timeout:15000});
const output=await p.evaluate(async()=>{
 const a=window.__na014;
 const db=await new Promise((ok,no)=>{const q=indexedDB.open('acp-remote-access-v1');q.onsuccess=()=>ok(q.result);q.onerror=()=>no(q.error)});
 const r=await new Promise(ok=>{const q=db.transaction('identity').objectStore('identity').get('primary');q.onsuccess=()=>ok(q.result)});db.close();a.originalIdentity=r;
 const b64=b=>btoa(String.fromCharCode(...new Uint8Array(b))).replace(/=/g,'').replace(/\+/g,'-').replace(/\//g,'_');
 const decode=s=>Uint8Array.from(atob(s.replace(/-/g,'+').replace(/_/g,'/').padEnd(Math.ceil(s.length/4)*4,'=')),c=>c.charCodeAt(0));
 const randomId=()=>b64(crypto.getRandomValues(new Uint8Array(16)));
 a.unknownId=randomId();a.attacker=await crypto.subtle.generateKey({name:'ECDSA',namedCurve:'P-256'},false,['sign','verify']);
 a.probe=async(label,clientId,mode)=>{
  const ws=new WebSocket(location.origin.replace('https:','wss:')+'/api/access/v1/control','acpr.access.v1');const messages=[];let timeout;
  const result=await new Promise((ok,no)=>{
   timeout=setTimeout(()=>{ws.close();no(Error('NA014 socket deadline'))},8000);
   ws.addEventListener('open',()=>ws.send(JSON.stringify({v:1,type:'hello',clientId,clientPublicKey:r.publicKey,clientNonce:b64(crypto.getRandomValues(new Uint8Array(32)))})));
   ws.addEventListener('message',async e=>{try{const m=JSON.parse(e.data);messages.push(m.type);if(m.type!=='challenge')return;
    const parts=m.hostProof.split('.');const key=await crypto.subtle.importKey('raw',decode(r.nodePublicKey),{name:'ECDSA',namedCurve:'P-256'},false,['verify']);
    if(!await crypto.subtle.verify({name:'ECDSA',hash:'SHA-256'},key,decode(parts[2]),new TextEncoder().encode(parts[0]+'.'+parts[1])))throw Error('host proof invalid');
    const payload=JSON.parse(new TextDecoder().decode(decode(parts[1])));
    let proof;if(mode==='marked'){proof=b64(new TextEncoder().encode(JSON.stringify({alg:'ES256',typ:'acpr-auth-client+jwt'})))+'.'+b64(new TextEncoder().encode(JSON.stringify({...payload,proofMarker:a.markers.proof,code:a.markers.code,privateKey:a.markers.private})))+'.'+b64(crypto.getRandomValues(new Uint8Array(64)));}
    else{const input=b64(new TextEncoder().encode(JSON.stringify({alg:'ES256',typ:'acpr-auth-client+jwt'})))+'.'+parts[1];const signature=await crypto.subtle.sign({name:'ECDSA',hash:'SHA-256'},mode==='wrong-signature'?a.attacker.privateKey:r.privateKey,new TextEncoder().encode(input));proof=input+'.'+b64(signature);}
    ws.send(JSON.stringify({v:1,type:'proof',proof}));
   }catch(error){ws.close();no(Error('NA014 probe construction failed'));}});
   ws.addEventListener('close',e=>ok({label,mode,code:e.code,reason:e.reason,messages,echo:a.contains(e.reason)}));
   ws.addEventListener('error',()=>no(Error('NA014 websocket transport error')));
  });clearTimeout(timeout);a.results.push(result);return result;
 };
 await a.probe('known-active',r.clientId,'marked');
 await a.probe('unknown',a.unknownId,'marked');
 await a.probe('known-active',r.clientId,'wrong-signature');
 await a.probe('unknown',a.unknownId,'wrong-signature');
 await a.probe('unknown-well-signed',a.unknownId,'well-signed');
 const http=[];
 for(const path of ['/api/access/v1/pair/claim','/api/access/v1/pair/status']){
  const response=await fetch(path,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({v:1,proof:JSON.stringify(a.markers)}),cache:'no-store'});http.push({path,status:response.status,body:await response.json()});
 }
 const invalidCode='ZZZZ-ZZZZ';a.secrets.push(invalidCode);
 const response=await fetch('/api/access/v1/pair/lookup',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({v:1,code:invalidCode}),cache:'no-store'});http.push({path:'lookup-invalid-code',status:response.status,body:await response.json()});a.httpResults=http;
 return {knownClientId:r.clientId,privateExtractable:r.privateKey.extractable,authenticatedProductionControl:true,results:a.results,http,echoes:a.echoes};
});
await fs.writeFile(root+'evidence/active-unknown.json',JSON.stringify(output,null,2));console.log(output);

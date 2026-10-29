export async function state(page){return page.evaluate(async()=>{
 const db=await new Promise((ok,no)=>{const q=indexedDB.open('acp-remote-access-v1');q.onsuccess=()=>ok(q.result);q.onerror=()=>no(q.error)});
 const r=await new Promise((ok,no)=>{const q=db.transaction('identity').objectStore('identity').get('primary');q.onsuccess=()=>ok(q.result);q.onerror=()=>no(q.error)});db.close();
 const hash=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(r.publicKey));
 return {secureContext:isSecureContext,origin:location.origin,status:r.status,clientId:r.clientId,nodeId:r.nodeId,publicKeySha256:Array.from(new Uint8Array(hash),x=>x.toString(16).padStart(2,'0')).join(''),privateExtractable:r.privateKey.extractable,loss:window.__responseLoss??null,audit:window.__recoveryAudit??null};
});}

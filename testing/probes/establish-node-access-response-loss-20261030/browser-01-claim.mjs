import fs from 'node:fs/promises';
const root='D:/Project/acp-remote/.tmp/approval-response-loss-20261030/';
const t=await taskSpace(10),page=t.page(globalThis.__lossPage??'p1');
await page.evaluate(()=>{
 const original=window.fetch.bind(window);
 window.__responseLoss={armed:true,dropped:0,observations:[]};
 const decode=s=>Uint8Array.from(atob(s.replace(/-/g,'+').replace(/_/g,'/').padEnd(Math.ceil(s.length/4)*4,'=')),c=>c.charCodeAt(0));
 window.fetch=async(...args)=>{
  const response=await original(...args);
  const url=String(args[0]);
  if(url.endsWith('/api/access/v1/pair/status')&&response.status===200&&window.__responseLoss.armed){
   const body=await response.clone().json(); const parts=body.hostProof.split('.');
   const payload=JSON.parse(new TextDecoder().decode(decode(parts[1])));
   if(payload.status==='approved'){
    const db=await new Promise((ok,no)=>{const q=indexedDB.open('acp-remote-access-v1');q.onsuccess=()=>ok(q.result);q.onerror=()=>no(q.error)});
    const record=await new Promise((ok,no)=>{const q=db.transaction('identity').objectStore('identity').get('primary');q.onsuccess=()=>ok(q.result);q.onerror=()=>no(q.error)});db.close();
    const key=await crypto.subtle.importKey('raw',decode(record.nodePublicKey),{name:'ECDSA',namedCurve:'P-256'},false,['verify']);
    const verified=await crypto.subtle.verify({name:'ECDSA',hash:'SHA-256'},key,decode(parts[2]),new TextEncoder().encode(parts[0]+'.'+parts[1]));
    if(!verified)throw Error('test adapter host signature verification failed');
    window.__responseLoss.armed=false;window.__responseLoss.dropped++;
    window.__responseLoss.observations.push({backendStatus:payload.status,clientId:payload.clientId,signatureVerified:verified,localStatus:record.status,localClientId:record.clientId});
    throw new TypeError('Controlled approved-status response delivery loss');
   }
  }
  return response;
 };
});
const input=JSON.parse(await fs.readFile(root+'browser-input.json','utf8'));
await page.fill('loc=css:input[name="pairing-code"]',input.code);
await page.fill('loc=css:input[name="display-name"]','Independent response-loss recovery');
await page.click('loc=role:button[name="认领这台浏览器"]');
await page.waitForFunction(()=>document.body.innerText.includes('等待本机'),undefined,{timeout:10000});
console.log(await page.snapshot());

const fs=await import('node:fs/promises');const root='D:/Project/acp-remote/.tmp/independent-management-r16-20261030';
const expected=JSON.parse(await fs.readFile(root+'/audit-output.json','utf8')).finalRevision;
const ticket=JSON.parse(await fs.readFile(root+'/browser-ticket.json','utf8'));
const task=await taskSpace('independent-management-r16-20261030');const page=task.page('p1');
await page.waitForFunction(r=>window.__managementAudit?.notices.some(n=>n.revision===r),expected,{timeout:10000});
const result=await page.evaluate(async({expected,code})=>{
 const a=window.__managementAudit;
 if(!a.notices.some(n=>n.revision===expected)||a.notices.some(n=>Object.keys(n).sort().join('/')!=='revision/type/v'))throw Error('latest notices');
 const database=await new Promise((ok,no)=>{const r=indexedDB.open('acp-remote-access-v1');r.onsuccess=()=>ok(r.result);r.onerror=()=>no(r.error)});
 const records=await new Promise((ok,no)=>{const tx=database.transaction([...database.objectStoreNames],'readonly');const r=tx.objectStore(database.objectStoreNames[0]).getAll();r.onsuccess=()=>ok(r.result);r.onerror=()=>no(r.error)});database.close();
 if(records.length!==1||records[0].privateKey.extractable||records[0].pendingClaim!==null)throw Error('identity secret retention');
 const publicRecord={...records[0]};delete publicRecord.privateKey;
 if(JSON.stringify(publicRecord).includes(code)||JSON.stringify(publicRecord).includes(code.replace('-','')))throw Error('pair code in identity');
 if(location.href.includes(code)||JSON.stringify({...localStorage,...sessionStorage}).includes(code))throw Error('code persisted');
 return{authenticated:document.body.innerText.includes('AUTHENTICATED'),latestRevision:expected,noticeRevisions:a.notices.map(n=>n.revision),notificationFieldsOnly:true,nonExtractableKey:true,pendingProofCleared:true,codeNotInIdentityUrlLocalSession:true};
},{expected,code:ticket.code});
await fs.writeFile(root+'/browser-final.json',JSON.stringify(result,null,2));console.log(result);

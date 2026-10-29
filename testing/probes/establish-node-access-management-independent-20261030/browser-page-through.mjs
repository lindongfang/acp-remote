const fs=await import('node:fs/promises');const root='D:/Project/acp-remote/.tmp/independent-management-r16-20261030';
const expected=JSON.parse(await fs.readFile(root+'/expected70.json','utf8'));const ticket=JSON.parse(await fs.readFile(root+'/browser-ticket.json','utf8'));
const task=await taskSpace('independent-management-r16-20261030');const page=task.page('p1');
await page.evaluate(()=>{window.__managementAudit.pages=[];[...document.querySelectorAll('button')].find(b=>b.textContent==='刷新列表').click()});
await page.waitForFunction(()=>window.__managementAudit.pages.length===1,undefined,{timeout:10000});
for(let count=2;count<=3;count++){await page.evaluate(()=>[...document.querySelectorAll('button')].find(b=>b.textContent==='载入下一页').click());await page.waitForFunction(n=>window.__managementAudit.pages.length>=n,count,{timeout:10000});}
const result=await page.evaluate(async({expected,code})=>{
 const a=window.__managementAudit;const pages=a.pages.filter(p=>p.ok).slice(-3).map(p=>p.result);
 const lengths=pages.map(p=>p.items.length);if(lengths.join('/')!=='32/32/6')throw Error('page lengths '+lengths);
 const ids=pages.flatMap(p=>p.items.map(i=>i.clientId));if(new Set(ids).size!==70||JSON.stringify([...ids].sort())!==JSON.stringify(expected))throw Error('ID collection differs');
 if(pages[2].nextCursor!==null)throw Error('last cursor');
 const fields=['clientId','createdAt','displayName','publicKeyFingerprint','revokedAt','status'].sort();
 if(pages.some(p=>p.items.some(i=>JSON.stringify(Object.keys(i).sort())!==JSON.stringify(fields))))throw Error('item field whitelist');
 if(a.notices.some(n=>JSON.stringify(Object.keys(n).sort())!==JSON.stringify(['revision','type','v'])))throw Error('notice fields');
 const response=await fetch('/',{cache:'no-store'});const html=await response.text();
 const headers=Object.fromEntries(['cache-control','referrer-policy','content-security-policy'].map(h=>[h,response.headers.get(h)]));
 if(!headers['cache-control']?.includes('no-store')||headers['referrer-policy']!=='no-referrer'||!headers['content-security-policy']?.includes("'self'"))throw Error('security headers');
 if(expected.some(id=>html.includes(id)))throw Error('static page contains management IDs');
 const stored=JSON.stringify({local:{...localStorage},session:{...sessionStorage}});
 if(stored.includes(code)||location.href.includes(code))throw Error('pair code retained in URL/storage');
 const external=performance.getEntriesByType('resource').map(e=>e.name).filter(n=>new URL(n).origin!==location.origin);
 if(external.length)throw Error('third party resource');
 return{secureContext:isSecureContext,userAgent:navigator.userAgent,lengths,uniqueIds:70,exactIdSet:true,lastCursorNull:true,itemFields:fields,notices:a.notices,headers,staticPageHasNoClientIds:true,codeNotInUrlLocalOrSession:true,thirdPartyRequests:external.length,authenticated:document.body.innerText.includes('AUTHENTICATED')};
},{expected,code:ticket.code});
await fs.writeFile(root+'/browser-paging.json',JSON.stringify(result,null,2));console.log(result);

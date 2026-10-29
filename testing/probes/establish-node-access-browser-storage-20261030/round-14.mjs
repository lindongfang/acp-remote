const fs=await import('node:fs/promises');
const task=await taskSpace(8);
if(task.ownership!=='agent')throw Error('Task ownership changed');
const evidence={spaceId:task.spaceId,ownership:task.ownership,clearedPage:await task.page('p1').snapshot(),otherOriginPage:await task.page('p2').snapshot(),browser:await task.page('p1').evaluate(()=>navigator.userAgent)};
await fs.writeFile('D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/evidence/browser-final.json',JSON.stringify(evidence,null,2));
const cleared=[];
for(const [label,origin] of [['p1','https://faraday.tail74842e.ts.net:30453'],['p2','https://faraday.tail74842e.ts.net:30454']]){
 const result=await task.page(label).evaluate(async expected=>{
  if(location.origin!==expected)throw Error('Wrong origin');
  await new Promise((res,rej)=>{const r=indexedDB.deleteDatabase('acp-remote-access-v1');r.onsuccess=res;r.onerror=()=>rej(r.error);r.onblocked=()=>rej(Error('blocked'))});
  return {origin:location.origin,remainingApplicationDatabases:(await indexedDB.databases()).filter(x=>x.name==='acp-remote-access-v1').length};
 },origin);cleared.push(result);
}
await fs.writeFile('D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/evidence/browser-cleanup.json',JSON.stringify(cleared,null,2));
console.log(cleared);
await task.finish({keep:[]});
console.log('Agent task space 8 finished; no profile-wide clearing performed');

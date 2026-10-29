import fs from 'node:fs/promises';
const root='D:/Project/acp-remote/.tmp/runtime-secret-acceptance-20261030/';
const t=await taskSpace(11),p=t.page('p1');
const external={ordinaryStdout:await fs.readFile(root+'node.out','utf8'),ordinaryStderr:await fs.readFile(root+'node.err','utf8'),audit:await fs.readFile(root+'audit-transient.json','utf8'),ownerBefore:await fs.readFile(root+'evidence/owner-before.txt','utf8'),ownerAfter:await fs.readFile(root+'evidence/owner-after.txt','utf8')};
const history=await p.cdp('Page.getNavigationHistory');
const output=await p.evaluate(async({external,history})=>{
 const a=window.__na014;
 const b64=s=>btoa(String.fromCharCode(...new TextEncoder().encode(s)));
 const candidates=[...new Set(a.secrets.flatMap(s=>[s,JSON.stringify(s).slice(1,-1),encodeURIComponent(s),b64(s),b64(s).replace(/=/g,'').replace(/\+/g,'-').replace(/\//g,'_')]))];
 const count=s=>candidates.filter(x=>String(s).includes(x)).length;
 const scans=Object.fromEntries(Object.entries(external).map(([name,text])=>[name,{bytes:new TextEncoder().encode(text).length,markerHits:count(text),sha256:null}]));
 for(const [name,text] of Object.entries(external))scans[name].sha256=await a.hash(text);
 const storageRows=[];let storageKeyMetadata=[];
 function visit(v){if(v instanceof CryptoKey){storageKeyMetadata.push({type:v.type,extractable:v.extractable,algorithm:v.algorithm.name,usages:[...v.usages]});return;}if(typeof v==='string'){storageRows.push(v);return;}if(v&&typeof v==='object')for(const [k,x] of Object.entries(v)){storageRows.push(k);visit(x);}}
 const databases=await indexedDB.databases();let records=0;
 for(const info of databases){const db=await new Promise((ok,no)=>{const q=indexedDB.open(info.name);q.onsuccess=()=>ok(q.result);q.onerror=()=>no(q.error)});for(const name of db.objectStoreNames){const values=await new Promise((ok,no)=>{const q=db.transaction(name).objectStore(name).getAll();q.onsuccess=()=>ok(q.result);q.onerror=()=>no(q.error)});records+=values.length;values.forEach(visit);}db.close();}
 const local=Object.fromEntries(Object.keys(localStorage).map(k=>[k,localStorage.getItem(k)]));
 const session=Object.fromEntries(Object.keys(sessionStorage).map(k=>[k,sessionStorage.getItem(k)]));
 const cacheNames=await caches.keys();const cacheTexts=[];for(const name of cacheNames){const cache=await caches.open(name);for(const request of await cache.keys()){cacheTexts.push(request.url);const response=await cache.match(request);if(response)cacheTexts.push(await response.text());}}
 const historyUrls=(history.entries??[]).map(e=>e.url),resourceUrls=performance.getEntriesByType('resource').map(e=>e.name);
 const check={indexedDb:{databases:databases.map(d=>d.name),records,stringFields:storageRows.length,markerHits:count(storageRows.join('\n')),keyMetadata:storageKeyMetadata},localStorage:{keys:Object.keys(local),markerHits:count(JSON.stringify(local))},sessionStorage:{keys:Object.keys(session),markerHits:count(JSON.stringify(session))},cacheStorage:{caches:cacheNames.length,markerHits:count(cacheTexts.join('\n'))},cookies:{markerHits:count(document.cookie)},navigationHistory:{entries:historyUrls.length,markerHits:count(historyUrls.join('\n'))},resourceHistory:{entries:resourceUrls.length,markerHits:count(resourceUrls.join('\n'))},dom:{markerHits:count(document.body.innerText)},networkUrls:{markerHits:count(a.network.filter(x=>x.url).map(x=>x.url).join('\n'))}};
 const publicMetadata={originalClientId:a.originalIdentity.clientId,nodeId:a.originalIdentity.nodeId,originalPublicKeySha256:await a.hash(a.originalIdentity.publicKey)};
 const network=a.network.map(x=>({...x,url:x.url?String(x.url).replace(/\?.*$/,'?<redacted-query>'):undefined}));
 return {secureContext:isSecureContext,origin:location.origin,syntheticMarkerDigests:await Promise.all(Object.entries(a.markers).map(async([kind,value])=>({kind,sha256:await a.hash(value)}))),candidateStrings:a.secrets.length,candidateVariants:candidates.length,scans,storageAndHistory:check,publicMetadata,network,authenticationResults:a.results,httpResults:a.httpResults,responseEchoes:a.echoes,allExternalClean:Object.values(scans).every(x=>x.markerHits===0),allStorageHistoryClean:Object.values(check).every(x=>x.markerHits===0)};
},{external,history});
await fs.writeFile(root+'evidence/artifacts.json',JSON.stringify(output,null,2));
console.log({allExternalClean:output.allExternalClean,allStorageHistoryClean:output.allStorageHistoryClean,responseEchoes:output.responseEchoes,scans:output.scans,storageAndHistory:output.storageAndHistory,networkRecords:output.network.length});
if(!output.allExternalClean||!output.allStorageHistoryClean||output.responseEchoes.length)throw Error('NA014-LEAK-001 runtime secret marker persisted or echoed; see redacted hit counts');

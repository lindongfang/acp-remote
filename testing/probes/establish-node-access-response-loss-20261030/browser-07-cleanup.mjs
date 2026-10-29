const t=await taskSpace(10);
for(const [label,origin] of [['p1','https://faraday.tail74842e.ts.net:30455'],['p2','https://faraday.tail74842e.ts.net:30456']]){
 const p=t.page(label);if(new URL(await p.url()).origin!==origin)throw Error('cleanup origin mismatch');
 await p.goto('about:blank');
 await p.cdp('Storage.clearDataForOrigin',{origin,storageTypes:'indexeddb'});
 await p.close();
}
console.log({ownedPagesClosed:true,onlyOwnedOriginIndexedDbCleared:true});
help('TaskSpace.finish');

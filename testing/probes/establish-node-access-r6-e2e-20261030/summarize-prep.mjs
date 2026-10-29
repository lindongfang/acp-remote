const fs=await import('node:fs/promises'),root='D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/evidence';
const before=JSON.parse(await fs.readFile(`${root}/damage-prepare.json`,'utf8')),after=JSON.parse(await fs.readFile(`${root}/damage-after.json`,'utf8'));
const damage=Object.fromEntries(Object.keys(before).map(k=>[k,{logicalStateUnchanged:JSON.stringify(before[k])===JSON.stringify(after[k]),clientCount:after[k].clients.length,auditCount:after[k].auditCount}]));
const b=JSON.parse(await fs.readFile(`${root}/B-all-revoked-final.json`,'utf8'));const summary={damage,bConnections:b.map(x=>({targetId:x.targetId,closeEvents:x.events.filter(e=>e.event==='close'),authenticated:x.events.some(e=>e.type==='authenticated'),subscribed:x.events.some(e=>e.event==='send'&&e.type==='subscribe'),pairForm:x.pairForm,resetFatal:x.body.includes('无法准备重新配对')}))};
await fs.writeFile(`${root}/pre-r9-summary.json`,JSON.stringify(summary,null,2));console.log(JSON.stringify(summary,null,2));

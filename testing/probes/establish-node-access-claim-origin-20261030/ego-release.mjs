import fs from 'node:fs/promises';
const root='D:/Project/acp-remote/.tmp/claim-origin-acceptance-20261030/';const {releaseAt}=JSON.parse(await fs.readFile(root+'release.json','utf8'));
const t=await taskSpace(12),p=t.page('p1');await p.evaluate(time=>window.__claimHarness.releaseAt(time),releaseAt);console.log({scheduled:true});

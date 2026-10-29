import fs from 'node:fs/promises';
const root='D:/Project/acp-remote/.tmp/claim-origin-acceptance-20261030/';const t=await taskSpace(12),p=t.page('p1');
const result=await p.evaluate('window.__claimHarness.summary()');if(result.sentinel!=='owned-ego-profile-A')throw Error('profile isolation failed');await fs.writeFile(root+'evidence/profile-A.json',JSON.stringify(result,null,2));console.log(result);

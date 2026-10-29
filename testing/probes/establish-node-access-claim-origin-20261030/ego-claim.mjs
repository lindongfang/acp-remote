import fs from 'node:fs/promises';
const root='D:/Project/acp-remote/.tmp/claim-origin-acceptance-20261030/';
const {code}=JSON.parse(await fs.readFile(root+'browser-input.json','utf8'));const t=await taskSpace(12),p=t.page('p1');
await p.fill('loc=css:input[name="pairing-code"]',code);await p.fill('loc=css:input[name="display-name"]','NA021 Profile A Ego');await p.click('loc=role:button[name="认领这台浏览器"]');
await p.waitForFunction(()=>window.__claimHarness.ready,undefined,{timeout:10000});console.log(await p.evaluate('window.__claimHarness.summary()'));

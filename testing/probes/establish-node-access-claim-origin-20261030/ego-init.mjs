import fs from 'node:fs/promises';
const t=await taskSpace(12),p=t.page('p1');
await p.waitForFunction(()=>!!document.querySelector('input[name="pairing-code"]'),undefined,{timeout:10000});
await p.evaluate(()=>localStorage.setItem('na021-profile-sentinel','owned-ego-profile-A'));
console.log(await p.evaluate(await fs.readFile(new URL('./harness.js',import.meta.url),'utf8')));

import {execFile} from 'node:child_process';import {promisify} from 'node:util';
const run=promisify(execFile),caseName=globalThis.__caseName;
if(!['committed','precommit','control'].includes(caseName))throw Error('owned case required');
const {stdout}=await run('C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe',['-NoProfile','-ExecutionPolicy','Bypass','-File','D:/Project/acp-remote/testing/probes/establish-node-access-approval-cancel-r20-20261030/console.ps1','-Action','code','-Case',caseName],{timeout:10000,windowsHide:true});
const {code}=JSON.parse(stdout.trim());const t=await (await import('./browser-common.mjs')).ownedSpace(),p=t.page('p1');
await p.fill('loc=css:input[name="pairing-code"]',code);await p.fill('loc=css:input[name="display-name"]','NA007 seam '+caseName);await p.click('loc=role:button[name="认领这台浏览器"]');await p.waitForFunction(()=>document.body.innerText.includes('等待本机'),undefined,{timeout:10000});console.log({case:caseName,productionClaimPending:true});

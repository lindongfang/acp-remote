const fs=await import('node:fs/promises');const task=await taskSpace('independent-management-r16-20261030');const page=task.page('p1');
await page.goto('about:blank');await page.cdp('Storage.clearDataForOrigin',{origin:'https://faraday.tail74842e.ts.net:30451',storageTypes:'indexeddb'});
const receipt=await task.finish({keep:[]});await fs.writeFile('D:/Project/acp-remote/.tmp/independent-management-r16-20261030/browser-cleanup.json',JSON.stringify({ownedOriginOnly:true,receipt},null,2));console.log(receipt);

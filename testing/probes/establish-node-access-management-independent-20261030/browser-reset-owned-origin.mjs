const task=await taskSpace('independent-management-r16-20261030');const page=task.page('p1');
// Port30451 belongs to this tester's isolated fixtures, never user30447.
await page.cdp('Storage.clearDataForOrigin',{origin:'https://faraday.tail74842e.ts.net:30451',storageTypes:'indexeddb'});
await page.goto('https://faraday.tail74842e.ts.net:30451');console.log(await page.snapshot());

const fs=await import('node:fs/promises');
const task=await taskSpace(8),page=task.page('p1');
await page.screenshot({path:'D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/evidence/cleared-site.png'});
const result={browser:await page.evaluate(()=>navigator.userAgent),ownership:task.ownership,spaceId:task.spaceId,clearedPage:await page.snapshot(),otherOriginPage:await task.page('p2').snapshot()};
await fs.writeFile('D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/evidence/browser-final.json',JSON.stringify(result,null,2));
console.log({spaceId:task.spaceId,ownership:task.ownership,browser:result.browser});

const task=await taskSpace('independent-jws-20261030');
const page=task.page('p1');
await page.goto('http://127.0.0.1:4178/tests/browser/jws.html');
await page.waitForFunction(()=>document.documentElement.dataset.result!==undefined,undefined,{timeout:15000});
if(await page.evaluate(()=>document.documentElement.dataset.result)!=='pass')throw Error(await page.evaluate(()=>document.querySelector('#result').textContent));
console.log({spaceId:task.spaceId,ownership:task.ownership});
console.log(await page.snapshot());

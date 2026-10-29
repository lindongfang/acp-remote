const task=await taskSpace(8),page=task.page('p1');
console.log(await page.snapshot());

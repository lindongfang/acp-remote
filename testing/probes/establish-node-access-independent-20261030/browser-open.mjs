const task = await taskSpace('independent-node-access-20261030');
console.log({ spaceId: task.spaceId, ownership: task.ownership });
console.log(await task.page('p1').snapshot());

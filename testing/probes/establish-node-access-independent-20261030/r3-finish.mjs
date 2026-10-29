const task=await taskSpace(4);
console.log({spaceId:task.spaceId,name:task.name,ownership:task.ownership});
await task.finish({keep:[]});

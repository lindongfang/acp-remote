const task=await taskSpace(3);
console.log({spaceId:task.spaceId,ownership:task.ownership,name:task.name});
await task.finish({keep:[]});

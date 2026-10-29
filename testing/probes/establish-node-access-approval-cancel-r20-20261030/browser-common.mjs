import fs from 'node:fs/promises';
export async function ownedSpace(){const {spaceId}=JSON.parse(await fs.readFile('D:/Project/acp-remote/.tmp/approval-cancel-boundary-r20-20261030/task-space.json','utf8'));return taskSpace(spaceId);}

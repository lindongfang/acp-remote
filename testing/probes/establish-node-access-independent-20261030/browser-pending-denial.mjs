import { writeFile } from 'node:fs/promises';
const task = await taskSpace(3);
const page = task.page('p1');
const results = await page.evaluate(async () => {
  const outcomes = [];
  for (const action of ['node.info', 'device.list', 'subscribe']) {
    outcomes.push(await new Promise((resolve, reject) => {
      const socket = new WebSocket(`wss://${location.host}/api/access/v1/control`, 'acpr.access.v1');
      const messages = [];
      const timer = setTimeout(() => { socket.close(); reject(new Error('probe timeout')); }, 8000);
      socket.onopen = () => socket.send(JSON.stringify(action === 'subscribe'
        ? { v: 1, type: 'subscribe', requestId: 'AAAAAAAAAAAAAAAAAAAAAQ', resource: 'devices' }
        : { v: 1, type: 'command', requestId: 'AAAAAAAAAAAAAAAAAAAAAQ', action, payload: action === 'device.list' ? {cursor:null} : {} }));
      socket.onmessage = event => messages.push(JSON.parse(event.data));
      socket.onclose = event => { clearTimeout(timer); resolve({ action, code:event.code, messages }); };
    }));
  }
  return outcomes;
});
if (results.some(result => result.code !== 4400 || result.messages.length !== 0)) throw new Error(JSON.stringify(results));
await writeFile('D:/Project/acp-remote/.tmp/independent-acceptance-20261030/pending-denial.json', JSON.stringify(results, null, 2));
console.log(results);

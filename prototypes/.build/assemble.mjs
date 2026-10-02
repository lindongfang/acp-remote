import fs from 'node:fs';
const head = fs.readFileSync('prototypes/.build/m-head.html','utf8');
const reuse = fs.readFileSync('prototypes/.build/reuse.js','utf8');
const tail = fs.readFileSync('prototypes/.build/m-tail.js','utf8');
fs.writeFileSync('prototypes/acp-remote-pwa-mobile.html', head + reuse + '\n' + tail + '\n</script>\n</body>\n</html>\n');
console.log('重新生成完成');

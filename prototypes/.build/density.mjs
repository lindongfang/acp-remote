import fs from 'node:fs';
const p = 'prototypes/.build/m-head.html';
let s = fs.readFileSync(p, 'utf8');

/* 移动端密度收紧：[旧, 新] */
const R = [
  ['.appbar{flex:none;padding:calc(var(--sat) + 8px) 12px 8px;background:var(--panel);',
   '.appbar{flex:none;padding:calc(var(--sat) + 5px) 12px 5px;background:var(--panel);'],
  ['border-bottom:1px solid var(--line);display:flex;align-items:center;gap:10px;min-height:52px}',
   'border-bottom:1px solid var(--line);display:flex;align-items:center;gap:10px;min-height:48px}'],
  ['.scroll{flex:1;overflow:auto;padding:12px;padding-bottom:calc(12px + var(--sab))}',
   '.scroll{flex:1;overflow:auto;padding:10px 12px;padding-bottom:calc(10px + var(--sab))}'],
  ['overflow:hidden;margin-bottom:12px}', 'overflow:hidden;margin-bottom:9px}'],
  ['.dhead{display:flex;align-items:center;gap:8px;padding:12px 14px;background:var(--panel-2);',
   '.dhead{display:flex;align-items:center;gap:8px;padding:10px 13px;background:var(--panel-2);'],
  ['cursor:pointer;min-height:56px}', 'cursor:pointer;min-height:50px}'],
  ['.srow{display:flex;align-items:center;gap:10px;padding:12px 14px;border-bottom:1px solid var(--line-2);',
   '.srow{display:flex;align-items:center;gap:10px;padding:10px 13px;border-bottom:1px solid var(--line-2);'],
  ['.srow .t{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:14.5px}',
   '.srow .t{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:14px}'],
  ['padding:10px 8px;min-height:44px;border-radius:8px}', 'padding:7px 5px;min-height:38px;border-radius:8px}'],
  ['.btn{border:1px solid var(--line);background:var(--panel);padding:11px 16px;border-radius:10px;\n    cursor:pointer;min-height:44px;font-weight:600}',
   '.btn{border:1px solid var(--line);background:var(--panel);padding:9px 15px;border-radius:10px;\n    cursor:pointer;min-height:42px;font-weight:600}'],
  ['.blank{text-align:center;padding:52px 24px;color:var(--ink-3)}',
   '.blank{text-align:center;padding:38px 20px;color:var(--ink-3)}'],
  ['border-radius:10px;padding:10px 12px;margin-bottom:10px}', 'border-radius:10px;padding:8px 11px;margin-bottom:8px}'],
  ['.segbar{display:flex;gap:0;border:1px solid var(--line);border-radius:10px;overflow:hidden;background:var(--panel);margin-bottom:10px}',
   '.segbar{display:flex;gap:0;border:1px solid var(--line);border-radius:10px;overflow:hidden;background:var(--panel);margin-bottom:8px}'],
  ['.segbar button{flex:1;border:0;background:transparent;padding:11px 0;cursor:pointer;font-size:13.5px;color:var(--ink-2)}',
   '.segbar button{flex:1;border:0;background:transparent;padding:9px 0;cursor:pointer;font-size:13.5px;color:var(--ink-2)}'],
  ['.connstrip{flex:none;display:flex;align-items:center;gap:9px;padding:8px 14px;',
   '.connstrip{flex:none;display:flex;align-items:center;gap:9px;padding:6px 13px;'],
  ['.tabbar button{flex:1;border:0;background:transparent;padding:11px 0;cursor:pointer;font-size:13px;\n    color:var(--ink-3);min-height:48px;',
   '.tabbar button{flex:1;border:0;background:transparent;padding:8px 0;cursor:pointer;font-size:13px;\n    color:var(--ink-3);min-height:42px;'],
  ['.composer{flex:none;background:var(--panel);border-top:1px solid var(--line);padding:8px 12px calc(8px + var(--sab))}',
   '.composer{flex:none;background:var(--panel);border-top:1px solid var(--line);padding:6px 12px calc(6px + var(--sab))}'],
  ['.chips{display:flex;gap:7px;overflow-x:auto;padding-bottom:8px;-webkit-overflow-scrolling:touch}',
   '.chips{display:flex;gap:6px;overflow-x:auto;padding-bottom:6px;-webkit-overflow-scrolling:touch}'],
  ['.chip-pick{flex:none;border:1px solid var(--line);background:var(--panel-2);border-radius:999px;\n    padding:6px 12px;font-size:12.5px;color:var(--ink-2);cursor:pointer;min-height:34px}',
   '.chip-pick{flex:none;border:1px solid var(--line);background:var(--panel-2);border-radius:999px;\n    padding:5px 11px;font-size:12.5px;color:var(--ink-2);cursor:pointer;min-height:32px}'],
  ['padding:6px 12px;font-size:12px;color:var(--ink-2);min-height:34px}',
   'padding:5px 11px;font-size:12px;color:var(--ink-2);min-height:32px}'],
  ['.inputrow textarea{flex:1;border:0;outline:0;background:transparent;font:inherit;color:var(--ink);\n    resize:none;min-height:40px;max-height:120px;line-height:1.5}',
   '.inputrow textarea{flex:1;border:0;outline:0;background:transparent;font:inherit;color:var(--ink);\n    resize:none;min-height:34px;max-height:104px;line-height:1.45}'],
  ['.hintrow{font-size:11px;color:var(--ink-3);margin-top:6px;display:flex;gap:10px;flex-wrap:wrap}',
   '.hintrow{font-size:11px;color:var(--ink-3);margin-top:4px;display:flex;gap:10px;flex-wrap:wrap}'],
  ['.msg-user{max-width:84%;margin-left:auto;background:var(--accent-soft);border-radius:14px;\n    padding:10px 13px;margin-top:14px;word-break:break-word}',
   '.msg-user{max-width:84%;margin-left:auto;background:var(--accent-soft);border-radius:14px;\n    padding:9px 12px;margin-top:10px;word-break:break-word}'],
  ['.msg-agent{margin-top:16px}', '.msg-agent{margin-top:12px}'],
  ['.md p{margin:0 0 9px;white-space:pre-wrap}', '.md p{margin:0 0 7px;white-space:pre-wrap}'],
  ['.md h1{font-size:17px;margin:14px 0 7px} .md h2{font-size:16px;margin:14px 0 7px} .md h3{font-size:15px;margin:12px 0 6px}',
   '.md h1{font-size:17px;margin:11px 0 5px} .md h2{font-size:16px;margin:11px 0 5px} .md h3{font-size:15px;margin:9px 0 4px}'],
  ['.md ul,.md ol{margin:0 0 9px;padding-left:22px} .md li{margin:3px 0}',
   '.md ul,.md ol{margin:0 0 7px;padding-left:20px} .md li{margin:2px 0}'],
  ['.md table{border-collapse:collapse;margin:0 0 10px;font-size:12.5px;width:100%}',
   '.md table{border-collapse:collapse;margin:0 0 8px;font-size:12.5px;width:100%}'],
  ['.fold{border:1px solid var(--line);border-left:3px solid var(--line);border-radius:10px;\n    background:var(--panel);overflow:hidden;margin-top:8px}',
   '.fold{border:1px solid var(--line);border-left:3px solid var(--line);border-radius:10px;\n    background:var(--panel);overflow:hidden;margin-top:6px}'],
  ['.fold .fh{background:transparent;display:flex;align-items:center;gap:9px;padding:11px 12px;cursor:pointer;\n    font-size:13px;color:var(--ink-2);min-height:44px}',
   '.fold .fh{background:transparent;display:flex;align-items:center;gap:9px;padding:8px 11px;cursor:pointer;\n    font-size:13px;color:var(--ink-2);min-height:40px}'],
  ['.fold .fb{padding:10px 12px;border-top:1px solid var(--line-2);font-size:13px;color:var(--ink-2);',
   '.fold .fb{padding:9px 11px;border-top:1px solid var(--line-2);font-size:13px;color:var(--ink-2);'],
  ['.perm{border:1px solid var(--warn);border-left:3px solid var(--warn);background:var(--warn-soft);\n    border-radius:12px;padding:12px;margin-top:8px}',
   '.perm{border:1px solid var(--warn);border-left:3px solid var(--warn);background:var(--warn-soft);\n    border-radius:12px;padding:11px;margin-top:6px}'],
  ['.deg{border:1px dashed var(--line);border-left:3px solid var(--ink-3);background:var(--panel-2);\n    border-radius:10px;padding:11px 12px;margin-top:8px}',
   '.deg{border:1px dashed var(--line);border-left:3px solid var(--ink-3);background:var(--panel-2);\n    border-radius:10px;padding:10px 11px;margin-top:6px}'],
  ['.code{margin-top:10px;border:1px solid var(--line);border-left:3px solid var(--accent);border-radius:10px;',
   '.code{margin-top:8px;border:1px solid var(--line);border-left:3px solid var(--accent);border-radius:10px;'],
  ['.fitem{display:flex;align-items:center;gap:10px;padding:12px 14px;border-bottom:1px solid var(--line-2);\n    cursor:pointer;min-height:52px}',
   '.fitem{display:flex;align-items:center;gap:10px;padding:10px 13px;border-bottom:1px solid var(--line-2);\n    cursor:pointer;min-height:48px}'],
  ['.pairbody{flex:1;overflow:auto;padding:20px 18px calc(24px + var(--sab))}',
   '.pairbody{flex:1;overflow:auto;padding:16px 16px calc(18px + var(--sab))}'],
  ['.pair h1{font-size:23px;line-height:1.3;margin:0 0 10px}', '.pair h1{font-size:21px;line-height:1.3;margin:0 0 8px}'],
  ['.pair .lede{margin:0 0 22px;color:var(--ink-2)}', '.pair .lede{margin:0 0 16px;color:var(--ink-2)}'],
  ['.sasgrid span{flex:1;aspect-ratio:3/4;display:grid;place-items:center;font-family:var(--mono);\n    font-size:26px;font-weight:600;',
   '.sasgrid span{flex:1;aspect-ratio:1/1;display:grid;place-items:center;font-family:var(--mono);\n    font-size:24px;font-weight:600;'],
  ['.countdown{display:flex;align-items:center;gap:12px;margin:18px 0 6px}', '.countdown{display:flex;align-items:center;gap:12px;margin:12px 0 4px}'],
  ['.olsteps li{margin:5px 0}', '.olsteps li{margin:3px 0}'],
  ['.scopelist li{display:flex;gap:10px;font-size:13.5px;margin:7px 0;align-items:flex-start}',
   '.scopelist li{display:flex;gap:10px;font-size:13.5px;margin:5px 0;align-items:flex-start}'],
];

let ok = 0; const miss = [];
for (const [a, b] of R) {
  if (s.includes(a)) { s = s.replace(a, b); ok++; }
  else miss.push(a.slice(0, 48).replace(/\n/g, '|'));
}

/* 按画布宽度隐藏键盘提示（桌面预览手机时也生效） */
const frameOld = '.frame{width:100%;max-width:430px;height:100dvh;max-height:100dvh;background:var(--bg);\n    display:flex;flex-direction:column;position:relative;overflow:hidden}';
const frameNew = '.frame{width:100%;max-width:430px;height:100dvh;max-height:100dvh;background:var(--bg);\n    display:flex;flex-direction:column;position:relative;overflow:hidden;container-type:inline-size}';
if (s.includes(frameOld)) { s = s.replace(frameOld, frameNew); ok++; } else miss.push('.frame{...}');

const marker = '  /* ---------- 底部 sheet（全屏） ---------- */';
if (s.includes(marker)) {
  s = s.replace(marker, '  /* 容器查询：按画布宽度而非视口判断，桌面预览手机时同样生效 */\n  @container (max-width:520px){ .kbdhint{display:none} }\n\n' + marker);
  ok++;
} else miss.push('@container 标记点');

fs.writeFileSync(p, s);
console.log('已收紧 ' + ok + ' 项' + (miss.length ? '\n未匹配 ' + miss.length + ' 项：\n  ' + miss.join('\n  ') : ''));
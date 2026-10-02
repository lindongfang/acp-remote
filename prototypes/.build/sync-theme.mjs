import fs from 'node:fs';

/* 把移动端的「主题图标」与桌面端定稿对齐：
   样式=实心(solid)、含义=点击后的效果(target)、颜色=亮橙(data-ic，深浅两套由 CSS 给)。
   同时移除控制台里对应的下拉与状态字段。 */

let t = fs.readFileSync('prototypes/.build/m-tail.js', 'utf8');
let h = fs.readFileSync('prototypes/.build/m-head.html', 'utf8');
let ok = 0; const miss = [];
const sub = (str, a, b) => {
  if (a instanceof RegExp) { const re = a; if (!re.test(str)) { miss.push(re.source.slice(0,46)); return str; } ok++; return str.replace(re, b); }
  if (!str.includes(a)) { miss.push(a.slice(0, 46).replace(/\n/g, '|')); return str; }
  ok++; return str.replace(a, b);
};

/* ---- 1) 精简图标构建：只保留实心 + target，路径常量与桌面端一致 ---- */
const oldGlyphStart = t.indexOf('/* 主题图标：SVG。样式 line / solid / duo');
const oldGlyphEnd = t.indexOf('function viewDirs(){');
if (oldGlyphStart < 0 || oldGlyphEnd < 0) { console.log('定位图标块失败'); process.exit(1); }
const newGlyph = [
  '/* 主题图标：实心 SVG，含义=点击后的效果，颜色=亮橙（data-ic，深浅两套由 CSS 给） */',
  'var SUN_RAYS  = "M12 2.6v2M12 19.4v2M4.7 4.7l1.4 1.4M17.9 17.9l1.4 1.4M2.6 12h2M19.4 12h2M4.7 19.3l1.4-1.4M17.9 6.1l1.4-1.4";',
  'var MOON_C    = "M20.2 14.6A8.4 8.4 0 0 1 9.4 3.8a8.4 8.4 0 1 0 10.8 10.8z";',
  'function svg(inner, fill){',
  '  return \'<svg viewBox="0 0 24 24" fill="\'+(fill||"none")+\'" stroke="currentColor" stroke-width="1.6" \'+',
  '    \'stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">\'+inner+\'</svg>\';',
  '}',
  'function themeGlyph(){',
  '  /* 点击后变成什么：白天显示月亮（点了变黑），黑夜显示太阳（点了变白） */',
  '  return (effTheme() === "dark")',
  '    ? svg(\'<circle cx="12" cy="12" r="4.8" fill="currentColor" stroke="none"/><path d="\'+SUN_RAYS+\' stroke-width="1.8"/></svg>\')',
  '    : svg(\'<path d="\'+MOON_C+\'" fill="currentColor"/></svg>\', "none");',
  '}',
  'function themeBtnHtml(){',
  '  return \'<button class="ab-btn" data-ic="orange" data-act="theme" title="切换白天 / 黑夜" aria-label="切换白天或黑夜模式">\' +',
  '    themeGlyph() + \'</button>\';',
  '}',
  '',
].join('\n');
t = t.slice(0, oldGlyphStart) + newGlyph + t.slice(oldGlyphEnd);
ok++;

/* ---- 2) 移除控制台里的两个下拉及其 handler ---- */
t = sub(t, '$("iconStyleSel").onchange = function(){ A.iconStyle = this.value; render(); };\n', '');
t = sub(t, '$("iconSemSel").onchange = function(){ A.iconSem = this.value; render(); };\n', '');

/* ---- 3) head：删下拉 markup、删状态字段、加亮橙 CSS ---- */
h = sub(h, /    <label>主题图标<select id="iconStyleSel">[\s\S]*?<\/select><\/label>\n    <label>图标含义<select id="iconSemSel">[\s\S]*?<\/select><\/label>\n/, '');
h = sub(h, '    iconStyle:"line", iconSem:"target",\n', '');
h = sub(h, '  .ab-btn svg{width:17px;height:17px;display:block}',
        '  .ab-btn svg{width:17px;height:17px;display:block}\n' +
        '  .ab-btn[data-ic="orange"]{color:#d9480f}\n' +
        '  :root[data-theme="dark"] .ab-btn[data-ic="orange"]{color:#ffa94d}');

fs.writeFileSync('prototypes/.build/m-tail.js', t);
fs.writeFileSync('prototypes/.build/m-head.html', h);
console.log('移动端对齐完成：' + ok + ' 处' + (miss.length ? '；未匹配 ' + miss.length + ' 处：\n  ' + miss.join('\n  ') : ''));
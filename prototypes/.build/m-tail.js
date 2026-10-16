/* ============ 以下复用桌面原型的纯逻辑函数（Markdown / 高亮 / 折叠 / 降级 / 主机面板） ============ */
var HL_RE = /(\/\/[^\n]*)|("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`)|([A-Za-z_$][\w$]*)|(\b\d+(?:\.\d+)?\b)|(#[A-Za-z_][\w]*)|(@[A-Za-z_][\w]*)/g;
var HL_KW = {function:1,const:1,let:1,var:1,return:1,if:1,else:1,for:1,while:1,new:1,await:1,async:1,
  export:1,import:1,from:1,class:1,extends:1,typeof:1,instanceof:1,null:1,undefined:1,true:1,false:1,
  this:1,try:1,catch:1,finally:1,throw:1,interface:1,type:1,as:1,implements:1,public:1,private:1,
  readonly:1,void:1,switch:1,case:1,break:1,continue:1,of:1,in:1,yield:1,static:1,enum:1};

function cur(){ return SESSIONS.filter(function(x){ return x.id===A.session; })[0] || SESSIONS[0]; }
/* 当前会话的模式状态：候选列表来自该会话 Agent；当前值优先用本会话本地已选（等 `session.mode.changed` 回执前的乐观值），
   否则是 Agent 的 `currentModeId`；两者都不在候选里就退回第一个候选，候选为空则是 null。 */
function modeState(){
  var base = AGENT_MODES[cur().agent] || { currentModeId:null, availableModes:[] };
  var list = base.availableModes;
  var id = A.modeSel[A.session] || base.currentModeId;
  if(id !== null && !list.some(function(m){ return m.id===id; })) id = list.length ? list[0].id : null;
  return { currentModeId:id, availableModes:list };
}
function modeLabel(ms){
  var hit = ms.availableModes.filter(function(m){ return m.id===ms.currentModeId; })[0];
  return hit ? hit.name : null;
}
/* 当前会话的模型选择项：候选与当前值都来自该会话 Agent；本地已选（等 `session.config.changed` 回执前的乐观值）优先。 */
function modelState(){
  var opt = (AGENT_CONFIGS[cur().agent] || { model:{ currentValue:null, options:[] } }).model;
  var list = opt.options || [];
  var value = A.modelSel[A.session] || opt.currentValue;
  if(value !== null && !list.some(function(o){ return o.value===value; })) value = list.length ? list[0].value : null;
  return { currentValue:value, options:list };
}
function modelLabel(ms){
  var hit = ms.options.filter(function(o){ return o.value===ms.currentValue; })[0];
  return hit ? hit.name : null;
}
function turnState(){
  var s=cur();
  var st = (s.id==="s1") ? A.chatScenario : s.state;
  return (st==="waiting") ? "waiting_permission" : st;
}
function active(){
  var st=turnState();
  return st==="running"||st==="queued"||st==="waiting_permission"||st==="waiting_input";
}
function isClosed(){ return turnState()==="closed"; }
function dirById(id){ return DIRS.filter(function(d){ return d.id===id; })[0]; }
/* 远端节点上的目录树（原型假数据）：Windows 可能有多个盘，所以根是「此电脑」+ 盘符列表；
   已登记目录都在 D: 上，另外补几层让浏览看起来像真的磁盘。真实实现里这些来自服务端的目录列举。 */
var NODE_DRIVES = ["C:\\", "D:\\", "E:\\"];
var NODE_DRIVE_LABELS = { "c:\\":"系统盘", "d:\\":"本地磁盘", "e:\\":"移动磁盘" };
var NODE_PATHS = [
  "C:\\Users","C:\\Users\\dev","C:\\Users\\dev\\Desktop","C:\\Users\\dev\\Documents","C:\\Program Files",
  "D:\\work","D:\\work\\api","D:\\work\\api\\src","D:\\work\\api\\tests","D:\\work\\infra","D:\\work\\infra\\deploy",
  "D:\\personal","D:\\personal\\site","D:\\personal\\notes","D:\\personal\\blog","D:\\personal\\api",
  "D:\\tmp","D:\\tmp\\sandbox","D:\\archive","D:\\archive\\old",
  "E:\\media","E:\\backup","E:\\backup\\2025"
];
/* 路径工具：统一成 Windows 反斜杠；盘符根保留尾分隔符（否则 "D:" 会被判成相对路径）；"" = 此电脑。 */
function normPath(p){
  var s = String(p||"").replace(/[\\\/]+/g, "\\").replace(/\\+$/,"");
  if(/^[A-Za-z]:$/.test(s)) s += "\\";
  return s;
}
function joinPath(dir, name){
  var d = normPath(dir);
  return (/[\\\/]$/.test(d) ? d : d+"\\") + name;
}
function dirNameOf(p){ var s=normPath(p), i=s.lastIndexOf("\\"); return i<0 ? s : s.slice(i+1); }
function parentOf(p){
  var s = normPath(p);
  if(s==="") return null;
  if(/^[A-Za-z]:\\?$/.test(s)) return "";   /* 盘符根的上一级是「此电脑」 */
  var i = s.lastIndexOf("\\");
  return i<=0 ? null : s.slice(0,i)+"\\";
}
function isBrowsable(p){
  var s = normPath(p);
  if(s==="") return true;
  var low = s.toLowerCase();
  for(var i=0;i<NODE_DRIVES.length;i++){ if(low.indexOf(NODE_DRIVES[i].toLowerCase())===0) return true; }
  return false;
}
function driveLabel(p){ return NODE_DRIVE_LABELS[String(p).toLowerCase()] || ""; }
function childPaths(p){
  if(normPath(p)==="") return NODE_DRIVES.slice();
  var d = normPath(p);
  var prefix = (/[\\\/]$/.test(d) ? d : d+"\\"), low = prefix.toLowerCase();
  return NODE_PATHS.filter(function(x){
    if(x.toLowerCase().indexOf(low)!==0) return false;
    var rest = x.slice(prefix.length);
    return rest.length>0 && rest.indexOf("\\")<0;
  });
}
function crumbs(p){
  var out=[{ name:"此电脑", path:"" }];
  if(normPath(p)==="") return out;
  var prefix="";
  normPath(p).split(/[\\\/]+/).filter(Boolean).forEach(function(seg, i){
    prefix = i===0 ? seg+"\\" : joinPath(prefix, seg);
    out.push({ name: i===0 ? seg+"\\" : seg, path: prefix });
  });
  return out;
}
function isAbsPath(p){ return /^[A-Za-z]:[\\\/]/.test(p) || /^[\\\/]{1,2}/.test(p); }
function registeredByPath(p){
  var low = normPath(p).toLowerCase();
  for(var i=0;i<DIRS.length;i++){ if(DIRS[i].path && normPath(DIRS[i].path).toLowerCase()===low) return DIRS[i]; }
  return null;
}
/* 顶部「新建」的目录情形：这台电脑能不能为这次创建新建目录由控制台模拟；
   在弹窗里新建的文件夹必然属于「会先新建」那一支。 */
function dirCaseKind(){
  if(A.mkdirCase==="blocked") return "blocked";
  return A.pickKind==="create" ? "missing" : "exists";
}
/* 目录分支提示：路径为空（还没指定）时没有分支可讲。 */
function dirBranchNotice(path){
  if(!path) return "";
  var kind = dirCaseKind();
  /* 形状不合法的路径不该再谈存不存在：它在到达文件系统之前就会被拒。 */
  if(!isAbsPath(path) || path.indexOf("..")>=0)
    return '<div class="notice warn"><span>⚠</span><span><b>'+esc(path)+'</b> 不是可用的目录：必须是绝对路径，' +
      '且不能包含「..」。提交会被服务端以参数类错误（protocol.schema_invalid）拒绝，不创建会话。</span></div>';
  if(kind==="blocked")
    return '<div class="notice warn"><span>⚠</span><span>这台电脑无法新建 <b>'+esc(path)+
      '</b>：父目录不存在，或没有写入权限。提交会明确失败（服务端不可用类错误 internal.unavailable），' +
      '不新建目录、不创建会话，也不会留下半成品。</span></div>';
  if(kind==="missing")
    return '<div class="notice info"><span>ⓘ</span><span>这台电脑上还没有 <b>'+esc(path)+
      '</b>。创建会话时会先在电脑上新建这个目录，再在它下面启动 Agent。' +
      '新建出来的目录不登记到目录列表，因此这个会话按「未分组」呈现。</span></div>';
  return '<div class="notice info"><span>ⓘ</span><span>将使用电脑上已存在的 <b>'+esc(path)+
    '</b>。它不在已登记目录里，因此这个会话按「未分组」呈现。</span></div>';
}
function dirPath(){ return (A.newDir||"").trim(); }
function mdInlinePlain(t){ return esc(t); }
function passFilter(s){
  if(A.filter==="running") return s.state==="running"||s.state==="queued";
  if(A.filter==="waiting") return s.state==="waiting_permission"||s.state==="waiting_input";
  return true;
}
function toast(m){ var t=$("toast"); t.textContent=m; t.classList.add("show");
  clearTimeout(t._x); t._x=setTimeout(function(){ t.classList.remove("show"); },1900); }
function fmtPct(c){ return c.used===null||!c.size ? null : Math.round(c.used/c.size*100); }
function connChip(){
  var c = CTX[A.ctx], pct = fmtPct(c);
  var cls = pct===null?"":(pct>=85?" crit":(pct>=60?" warn":""));
  return '<button class="chip-ctx'+cls+'" data-act="ctx"><span class="bar"><i style="width:'+
    (pct===null?0:pct)+'%"></i></span>'+(pct===null?"—":pct+"%")+
    ' <span class="mono">上下文</span></button>';
}
function ctxWarn(){
  var c=CTX[A.ctx], pct=fmtPct(c);
  if(pct===null||pct<85) return "";
  return '<div class="notice warn"><span>⚠</span><span>上下文已用 '+pct+'%。继续发送时 Agent 可能截断较早的内容。</span></div>';
}

/* ============ 移动端视图 ============ */
function appbar(title, sub, right, back){
  return '<div class="appbar">' +
    (back!==false ? '<button class="ab-btn" data-act="back">‹</button>' : '') +
    '<div class="ab-title"><div class="t">'+title+'</div>'+(sub?'<div class="s">'+sub+'</div>':'')+'</div>' +
    (right||'') + themeBtnHtml() + '</div>';
}
/* 主题图标：实心 SVG，含义=点击后的效果，颜色=亮橙（data-ic，深浅两套由 CSS 给） */
var SUN_RAYS  = "M12 2.6v2M12 19.4v2M4.7 4.7l1.4 1.4M17.9 17.9l1.4 1.4M2.6 12h2M19.4 12h2M4.7 19.3l1.4-1.4M17.9 6.1l1.4-1.4";
var MOON_C    = "M20.2 14.6A8.4 8.4 0 0 1 9.4 3.8a8.4 8.4 0 1 0 10.8 10.8z";
function svg(inner, fill){
  return '<svg viewBox="0 0 24 24" fill="'+(fill||"none")+'" stroke="currentColor" stroke-width="1.6" '+
    'stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'+inner+'</svg>';
}
function themeGlyph(){
  /* 点击后变成什么：白天显示月亮（点了变黑），黑夜显示太阳（点了变白） */
  return (effTheme() === "dark")
    ? svg('<circle cx="12" cy="12" r="4.8" fill="currentColor" stroke="none"/><path d="'+SUN_RAYS+' stroke-width="1.8"/></svg>')
    : svg('<path d="'+MOON_C+'" fill="currentColor"/></svg>', "none");
}
function themeBtnHtml(){
  return '<button class="ab-btn" data-ic="orange" data-act="theme" title="切换白天 / 黑夜" aria-label="切换白天或黑夜模式">' +
    themeGlyph() + '</button>';
}
/* 目录页顶栏：左边是设备标识（连接状态也在这一个胶囊里表达），点它打开「主机与连接」；
   右边是「新建」（底色/文案参考桌面端主按钮）与主题。原来那个 ⋯ 已被设备标识取代。 */
function dirsAppbar(){
  var c = CONN[A.conn] || CONN.online;
  var tone = c.tone==="warn" ? " warn" : (c.tone ? "" : " danger");
  var dot = c.tone==="warn" ? "var(--warn)" : (c.tone ? "var(--ok)" : "var(--danger)");
  return '<div class="appbar">' +
    '<button class="devchip'+tone+'" data-act="host" aria-label="'+esc(HOSTNAME)+'（点此打开主机与连接）">' +
      '<span class="dot" style="background:'+dot+'"></span>' +
      '<span class="nm">'+esc(HOSTNAME)+'</span>' +
      (c.chip ? '<span class="st">'+esc(c.chip)+'</span>' : '') +
      '<span class="caret" aria-hidden="true"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" ' +
        'stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M6.5 10 12 15.2 17.5 10"/></svg></span>' +
    '</button>' +
    '<div style="flex:1;min-width:0"></div>' +
    '<button class="ab-new" data-act="newdir">⊕ 新建</button>' +
    themeBtnHtml() + '</div>';
}
function viewDirs(){
  var h = dirsAppbar();
  var body = '<div class="scroll">';
  var q = A.query;
  var matched = DIRS.filter(function(d){
    if(!q) return true;
    var any = SESSIONS.some(function(s){ return s.dir===d.id && passFilter(s) &&
      (s.title||"未命名会话").toLowerCase().indexOf(q)>=0; });
    return d.name.toLowerCase().indexOf(q)>=0 || any;
  });
  if(!matched.length){ body += '<div class="blank"><h2>没有匹配的结果</h2><p>换个关键词。</p></div>'; }
  var nameCount = {};
  DIRS.forEach(function(d){ nameCount[d.name] = (nameCount[d.name]||0) + 1; });
  matched.forEach(function(dir){
    var all = SESSIONS.filter(function(s){ return s.dir===dir.id; });
    var rows = all.filter(passFilter);
    if(!rows.length && q) return;
    var active0 = all.filter(function(s){ return s.state==="running"||s.state==="queued"; }).length;
    var collapsed = !!A.open["d-"+dir.id];
    body += '<div class="card'+(collapsed?" collapsed":"")+'"><div class="dhead">' +
      '<span class="caret" data-toggle="d-'+dir.id+'">▼</span>' +
      '<span class="dicon">▤</span><span class="dname">'+esc(dir.name)+'</span>' +
      /* 同名目录靠路径尾段区分（与桌面一致）：没有别名之后这是唯一的区分手段 */
      ((nameCount[dir.name]>1 && dir.path) ? '<span class="dpath">'+esc((parentOf(dir.path)||dir.path).replace(/[\\\/]+$/,""))+'</span>' : '') +
      (active0?'<span class="chip flat" style="color:var(--ok);background:var(--ok-soft)">运行中 '+active0+'</span>':'') +
      '<span class="chip flat">'+all.length+' 个会话</span></div><div class="dbody">' +
      rows.slice(0,3).map(function(s){ return srow(s); }).join("") +
      (!all.length ? '<div style="padding:10px 12px;color:var(--ink-3);font-size:13px">这个目录还没有会话</div>' : '') +
      '<div class="cardacts">' +
        (all.length>3 ? '<button class="linkbtn" data-all="'+dir.id+'">查看全部 ›</button>' : '<span></span>') +
        '<button class="linkbtn add" data-new="'+dir.id+'">⊕ 新建</button>' +
      '</div></div></div>';
  });
  body += '</div>';
  return h + '<div style="flex:1;display:flex;flex-direction:column;min-height:0">' +
    '<div style="padding:8px 12px 0"><div class="search"><span>🔍</span>' +
    '<input data-q value="'+esc(A.query)+'" placeholder="搜索目录或会话" /></div>' +
    '<div class="chips" style="margin-bottom:6px">' +
      '<button class="chip-pick'+(A.filter==="all"?" on":"")+'" data-f="all">全部</button>' +
      '<button class="chip-pick'+(A.filter==="running"?" on":"")+'" data-f="running">运行中</button>' +
      '<button class="chip-pick'+(A.filter==="waiting"?" on":"")+'" data-f="waiting">待处理</button>' +
    '</div></div>' + body + '</div>';
}
function srow(s){
  var name = s.title ? esc(s.title) : '<span class="noname">未命名会话</span>';
  var st = s.state==="waiting_permission" ? '<span class="st">待授权</span>' : '';
  return '<div class="srow'+(s.id===A.session?" on":"")+'" data-session="'+s.id+'">' +
    '<span class="dot '+s.state+'"></span><span class="t'+(s.title?"":" noname")+'">'+name+'</span>'+st+
    '<span class="w">'+esc(whenLabel(s.at))+'</span></div>';
}

/* 跳到某一页：页码夹到 1..pageCount，只有真的换页才重画，跳完滚回顶部 */
function goToPage(n){
  var pages = A.pageCount || 1;
  if(isNaN(n)) return;
  var p = Math.min(pages, Math.max(1, n));
  if(p === A.page) return;
  A.page = p;
  render();
  var sc = document.querySelector(".scroll"); if(sc) sc.scrollTop = 0;
}
/* 详情页：只按时间（新→旧）列会话，每页 PAGE_SIZE 条、可翻页；不再按月分组，也没有筛选。 */
var PAGE_SIZE = 10;
/* 翻页区不重复总数——顶栏目录名旁边已经显示了 */
function pagerHtml(pages){
  if(pages <= 1) return '';
  return '<div class="pagerbar">' +
    '<button class="btn" data-pagedir="-1"'+(A.page>1?"":" disabled")+'>‹ 上一页</button>' +
    '<span class="pinfo">第 <input id="pageInput" class="pageinput" type="text" inputmode="numeric" value="'+A.page+
      '" aria-label="页码（回车跳转）" title="输入页码后回车跳转" /> / '+pages+' 页</span>' +
    '<button class="btn" data-pagedir="1"'+(A.page<pages?"":" disabled")+'>下一页 ›</button>' +
  '</div>';
}
function viewDirDetail(id){
  var dir = dirById(id);
  if(!dir) return viewDirs();
  var all = SESSIONS.filter(function(s){ return s.dir===id; });   /* 已按时间倒序 */
  var pages = Math.max(1, Math.ceil(all.length / PAGE_SIZE));
  A.pageCount = pages;
  if(A.page > pages) A.page = pages;
  if(A.page < 1) A.page = 1;
  var pageRows = all.slice((A.page-1)*PAGE_SIZE, A.page*PAGE_SIZE);
  var h = appbar(esc(dir.name) + '<span class="chip">'+all.length+' 个会话</span>', '', '', true);
  var body = '<div class="scroll">';
  /* 路径单独一行、与新建弹层同一档字体 */
  if(dir.path) body += '<div class="pathline"><div class="dirinfo"><div class="drow"><span class="dk">路径</span>' +
    '<span class="dpath">'+esc(dir.path)+'</span></div></div></div>';
  if(!all.length){
    body += '<div class="blank"><h2>这个目录还没有会话</h2><p>在这里或电脑上新建一个会话，它会出现在这里。</p></div>';
  } else {
    body += pageRows.map(function(s){ return srow(s); }).join("");
    body += pagerHtml(pages);
  }
  body += '<div style="padding:6px 0 20px"><button class="btn block primary" data-new="'+dir.id+'">⊕ 新建</button>' +
    '<p class="hint" style="text-align:center;margin-top:8px">将在你的电脑上启动一个 Agent 进程</p></div>';
  body += '</div>';
  return h + body;
}

function viewSessions(){
  var s = cur(), dir = dirById(s.dir);
  var list = SESSIONS.filter(function(x){ return x.dir===s.dir; });
  var h = appbar('本目录的会话', dir?dir.name:'', '', true);
  /* 目录路径与别处同一档（原来塞在 appbar 副标题里，又小又淡） */
  return h + '<div class="scroll">' +
    (dir&&dir.path ? '<div class="pathline"><div class="dirinfo"><div class="drow"><span class="dk">路径</span>' +
      '<span class="dpath">'+esc(dir.path)+'</span></div></div></div>' : '') +
    '<div class="search"><span>🔍</span>' +
    '<input placeholder="在本目录内搜索" /></div>' +
    list.map(function(x){ return srow(x); }).join("") +
    '<div style="padding:14px 0"><button class="btn block" data-new="'+s.dir+'">⊕ 新建会话</button></div></div>';
}

function streamHTML(s){
  var out = "";
  var jumping = !!A.jumpTo && A.jumpTo.session===s.id;
  if(turnState()==="waiting_permission"||turnState()==="waiting_input")
    out += '<div class="notice warn"><span>⏸</span><span>有 1 个待处理授权请求，处理后才能继续。</span></div>';
  if(s.id==="s1"){
    out += userMsg("继续，把订阅清理也补上","14:31:10");
    if(jumping) out += '<div class="notice info"><span>↕</span><span>已定位到你这条消息。' +
      (A.jumpTo.perm ? '授权卡片就在下方。' : '继续往下看即可。') + '</span></div>';
    out += fold("t.think","think","🧠","思考中…","run","关闭路径有两处：超时回调和主动 close。",null,"14:31:12") +
      fold("t.read","tool","📄","src/auth/session.ts","done","",null,"14:31:26",
        {rows:[["kind","read"],["位置","session.ts:88-97"]]}) +
      fold("t.edit","tool","🔧","src/auth/session.ts","run","","src/auth/session.ts","14:31:40",
        {rows:[["kind","edit"],["改动","替换 1 行 · 新增 6 行"]],
         preview:[{t:"del",x:"- const idleTimeout = 30_000;"},
                  {t:"add",x:"+ const idleTimeout = opts.idleTimeoutMs ?? 10_000;"},
                  {t:"ctx",x:"  return { id: client.id };"}]}) +
      agentMsg("正在把空闲超时从 30 秒调整为 10 秒，并让关闭前先清理订阅…","14:31:48") +
      (active()?'<div><span class="live"><span class="dot running"></span>Agent 正在输出…</span></div>':'') +
      permCard(s) +
      (A.degradeDemo ? degCard("unknown","未识别的事件类型",
        '<span class="mono">acp.agent.custom_status</span><br>当前网页端没有专用视图，已保留原文。',"unknown","查看原文与元数据") +
      degCard("warn","原文未同步（超出单条上限）",
        '<span class="mono">file.changed · src/legacy/report.ts</span><br>diff 正文未随事件下发。',"bigdiff","查看原因") : "");
  } else {
    out += userMsg("（这个会话的最近一条输入）", whenLabel(s.at)) +
      agentMsg("这是 `"+s.agent+"` 在 "+(dirById(s.dir)?dirById(s.dir).name:"")+" 下的会话。", whenLabel(s.at)) +
      fold("g.think","think","🧠","思考","done","示例思考内容。",null,whenLabel(s.at));
    if(s.state==="waiting_permission") out += permCard(s);
  }
  /* 「跳到你最近的输入」胶囊：滚动离开用户消息时浮出（移动端需手动吸附到底部） */
  out += '<button class="jumppill" id="jumpPill"></button>';
  return out;
}

function chatPane(s){
  var c = CONN[A.conn] || CONN.online;
  /* 会话页顶栏只留四样：返回、会话标题、agent 标识、主题（颜色）切换。
     标题与 agent 同一行，靠字级与胶囊区分（agent 用与详情页个数相同的小胶囊）。 */
  var head = appbar(esc(s.title||"未命名会话") + '<span class="chip">'+esc(s.agent)+'</span>',
    '',
    '');
  var strip = c.strip ? '<div class="connstrip'+(c.strip[1]?" warn":"")+'"><span class="spin"></span>'+
    esc(c.strip[0])+'</div>' : '';
  var body = '<div class="scroll" id="stream">'+streamHTML(s)+
    '<div style="height:8px"></div></div>';
  var tabbar = '<div class="tabbar"><button data-tab="chat" class="'+(A.tab==="chat"?"on":"")+'">对话</button>' +
    '<button data-tab="files" class="'+(A.tab==="files"?"on":"")+'">变更文件 ('+FILE_ORDER.length+')</button></div>';
  var composer = '<div class="composer">' + ctxWarn() +
    '<div class="chips">' + connChip() +
      '<button class="chip-pick" data-act="mode">模式 <b>'+esc(modeLabel(modeState())||"未提供")+'</b> ▾</button>' +
      '<button class="chip-pick" data-act="model">模型 <b>'+esc(modelLabel(modelState())||"未提供")+'</b> ▾</button>' +
    '</div>' +
    '<div class="inputrow"><textarea id="ta"'+(active()||isClosed()?' readonly':'')+' placeholder="'+
      (isClosed()?"会话已关闭":(active()?"进行中，完成后可继续输入":"给 Agent 发消息"))+'"></textarea>' +
      (active()?'<button class="btn stop" data-act="stop">■ 停止</button>'
               :'<button class="btn primary" data-act="send">发送</button>')+'</div>' +
    '<div class="hintrow"><span class="kbdhint">Enter 发送 · Shift+Enter 换行</span><span>📎 附件暂不支持</span></div></div>';
  return head + strip + '<div style="flex:1;display:flex;flex-direction:column;min-height:0">' +
    body + tabbar + composer + '</div>';
}
function filesPane(){
  var f = FILES[A.selFile];
  var head = appbar('变更文件', FILE_ORDER.length + ' 个 · 整个会话', '', false);
  var list = FILE_ORDER.map(function(p){
    var x=FILES[p];
    return '<div class="fitem'+(p===A.selFile?" on":"")+'" data-file="'+esc(p)+'">' +
      '<span class="fst '+x.status+'">'+x.status+'</span><span class="mono" style="flex:1;min-width:0;'+
      'overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:11.5px">'+esc(p)+'</span>' +
      '<span style="font-size:11px;color:var(--ink-3)">+'+x.add+' −'+x.del+'</span></div>';
  }).join("");
  var d = f ? '<div class="diffwrap"><div class="dt">'+esc(A.selFile)+'</div>' +
    '<div class="dm">+'+f.add+' −'+f.del+'</div><div class="diff">' + f.diff.map(function(l){
      return '<div class="'+l.t+'">'+(l.t==="hd"?"":(l.t==="del"?"- ":l.t==="add"?"+ ":"  "))+hl(l.x)+'</div>';
    }).join("") + '</div><div class="dm" style="margin:10px 0 0">超出单条上限或未同步时显示明确说明，不渲染占位 diff。</div></div>' : '';
  return head + '<div class="scroll" style="padding:0">'+list+d+'</div>';
}

function viewChat(id){
  A.session = id || A.session;
  var s = cur();
  var c = CONN[A.conn] || CONN.online;
  if(c.block){
    return appbar('会话不可用','','',false) + '<div class="scroll"><div class="hblock">' +
      '<div class="t">'+esc(c.block[0])+'</div><div class="d">'+esc(c.block[1])+'</div>' +
      '<div style="margin-top:12px"><button class="btn primary block" data-go="'+esc(c.block[3]||"")+'">' +
      esc(c.block[2])+'</button></div></div></div>';
  }
  if(A.tab==="files") return filesPane();
  return chatPane(s);
}

function viewPair(){
  var p = A.pair, h = appbar('配对', HOSTNAME + ' · 安全连接', '', false), b = "";
  if(p==="S4"){
    b = '<h1>请核对电脑上的这 6 位数字</h1><p class="lede">两边一致再继续。不一致请立刻停止并重新扫码——' +
      '这通常意味着链接被转发给了别人。</p>' +
      '<div class="sasgrid" role="img" aria-label="验证码">'+PAIR.sas.split("").map(function(d){
        return '<span>'+d+'</span>'; }).join("")+'</div>' +
      '<p class="hint" style="color:var(--danger);font-weight:600">不一致就不要继续。</p>' +
      '<div class="countdown"><span class="cdtime">'+(Math.floor(A.pairLeft/60))+':'+
      String(A.pairLeft%60).padStart(2,"0")+'</span><span class="hint">后失效</span></div>' +
      '<p class="hint" style="margin-top:14px"><span class="spin"></span> 正在等待电脑确认…</p>';
  } else if(p==="S9"){
    b = '<div style="text-align:center;padding:20px 0"><div class="glyph" style="margin:0 auto 16px;' +
      'background:var(--ok-soft);color:var(--ok)">✓</div>' +
      '<h1 style="font-size:20px">已连接到 '+HOSTNAME+'</h1></div>' +
      '<div class="kvbox"><div class="kvrow"><span class="k">设备</span><span class="v" style="font-family:var(--sans)">' +
      '我的手机 · 浏览器 PWA</span></div>' +
      '<div class="kvrow"><span class="k">主机</span><span class="v">'+esc(PAIR.origin)+'</span></div>' +
      '<div class="kvrow"><span class="k">指纹</span><span class="v">'+esc(PAIR.fingerprint.slice(0,32))+'…</span></div></div>' +
      '<div class="notice warn"><span>⚠</span><span>身份密钥保存在本浏览器。清除数据或更换域名会永久删除它，之后必须重新配对。</span></div>' +
      '<button class="btn block primary" data-go="#/dirs">进入目录</button>';
  } else {
    b = '<div class="glyph danger">⚠</div><h1>这台电脑的身份密钥已变化</h1>' +
      '<p class="lede">网页不会自动接受新的密钥。请先在电脑上确认：是换了电脑，还是连接被人拦截过。</p>' +
      '<div class="kvbox"><div class="kvrow"><span class="k">已记录</span><span class="v">'+esc(PAIR.fingerprint.slice(0,28))+'…</span></div>' +
      '<div class="kvrow"><span class="k">本次出示</span><span class="v">7d41c0b95e2f…（不同）</span></div></div>' +
      '<button class="btn block" data-go="#/dirs">回目录</button>';
  }
  return h + '<div class="pairbody pair">'+b+
    '<hr style="border:0;border-top:1px solid var(--line-2);margin:26px 0 18px" />' +
    '<h2>这台设备申请了这些权限</h2><ul class="scopelist">'+
    SCOPES.map(function(s){ return '<li><span class="tick on">✓</span><span>'+esc(s.label)+'</span></li>'; }).join("")+
    '</ul><p class="hint" style="margin-top:10px">由电脑端发起时申请，本页只读。</p></div>';
}

/* ============ sheet（底部全屏层） ============ */
var sheetState = { type:null, key:null, sec:{} };
/* 弹层（底部面板）也要占一条历史记录：这样系统返回/右划（以及右边缘左右划的手势）会先关掉弹层，
   而不是把用户带出应用。用计数而不是布尔——面板可以在开着的时候再开一个（例如主机面板里点进别的面板）。 */
var sheetGuards = 0;
var pendingNav = null;   /* 弹层里的跳转：等弹层那条记录出栈后再执行，避免两次导航互相打架 */
function pushSheetGuard(){ history.pushState({ acprSheet:true }, "", location.href); sheetGuards++; }
function flushPendingNav(){ if(!pendingNav) return; var h = pendingNav; pendingNav = null; if(h) location.hash = h; }
/* 弹层里的链接（比如主机面板的阻断操作）：先关弹层，等它的历史记录回退落地再跳目标。 */
function closeSheetAndGo(h){ pendingNav = h; closeSheet(); }

function openSheet(type, key){
  sheetState.type = type; sheetState.key = key || null; sheetState.sec = {};
  paintSheet();
  $("sheet").classList.add("open"); $("scrim").classList.add("open");
  pushSheetGuard();
}
function closeSheet(){
  if(!$("sheet").classList.contains("open")){ flushPendingNav(); return; }
  $("sheet").classList.remove("open"); $("scrim").classList.remove("open"); closeDirPicker();
  /* 清掉弹层状态：否则「返回」会一直以为弹层还开着（关过一次之后返回就永久失效了） */
  sheetState.type = null; sheetState.key = null;
  if(sheetGuards > 0){ sheetGuards--; history.back(); }   /* 吃掉它自己那条记录 */
  else flushPendingNav();
}
function paintSheet(){
  var t = sheetState.type, h = "";
  if(t==="dx"){
    $("sheetTitle").textContent = "事件详情";
    h = dxBody(EVT[sheetState.key]);
  } else if(t==="host"){
    $("sheetTitle").textContent = "主机与连接";
    h = hostSheet();
  } else if(t==="new"){
    $("sheetTitle").textContent = "新建会话";
    h = newSessionSheet();
  }
  $("sheetBody").innerHTML = h;
  if(t!=="new"){ $("sheetClose").textContent = "关闭"; }
  else { $("sheetClose").textContent = "取消"; }
}
function dxBody(e){
  if(!e) return "";
  var sec = sheetState.sec, h = '<div class="kvrow"><span class="k">类型</span><span class="v">'+esc(e.type)+'</span></div>' +
    '<div class="kvrow"><span class="k">事件 ID</span><span class="v">'+esc(e.id)+'</span></div>' +
    '<div class="kvrow"><span class="k">全局序号</span><span class="v">'+esc(e.gseq)+'</span></div>' +
    '<div class="kvrow"><span class="k">会话序号</span><span class="v">'+esc(e.sseq)+'</span></div>' +
    '<div class="kvrow"><span class="k">时间</span><span class="v">'+esc(e.at)+'</span></div>';
  h += '<div class="dxsec'+(sec.view?" open":"")+'" data-sec="view"><div class="sh" data-seckey="view">' +
    '<span class="car">▶</span><span>结构化视图</span><span class="lg">payload.view</span></div>' +
    '<pre>'+esc(JSON.stringify(e.view,null,2))+'</pre></div>';
  if(e.rawJson){
    h += '<div class="dxsec'+(sec.raw?" open":"")+'" data-sec="raw"><div class="sh" data-seckey="raw">' +
      '<span class="car">▶</span><span>ACP 原文</span><span class="lg">acp.rawJson</span></div>' +
      '<pre>'+esc(e.rawJson)+'</pre><div class="meta">'+new Blob([e.rawJson]).size+' bytes · sha256 a41f6c9d…</div></div>';
  } else {
    h += '<div class="dxavail">原文未同步：'+esc(RAW_REASON[e.rawUnavailable.reason])+
      '（'+esc(e.rawUnavailable.reason)+'）<div class="mono" style="margin-top:6px">byteLength '+
      esc(e.rawUnavailable.byteLength)+'</div></div>';
  }
  h += '<p class="dxnote">原文按原始字节保存，不重新解析。请当作不可信数据处理。</p>';
  if(e.rawJson||e.rawUnavailable) h += '<button class="btn block" style="margin-top:12px" data-act="copy">⤓ 复制 JSON</button>';
  return h;
}
function hostSheet(){
  var c = CONN[A.conn] || CONN.online, h = "";
  if(c.block){
    h = '<div class="hblock"><div class="t">'+esc(c.block[0])+'</div><div class="d">'+esc(c.block[1])+'</div>' +
      '<div style="margin-top:12px"><button class="btn primary block" data-go="'+esc(c.block[3]||"")+'">' +
      esc(c.block[2])+'</button></div></div>';
    return h;
  }
  h += '<div class="kvrow"><span class="k">状态</span><span class="v" style="font-family:var(--sans)">' +
    (A.conn==="syncing"?"在线 · 正在追平":(c.badge))+'</span></div>' +
    '<div class="kvrow"><span class="k">上次同步</span><span class="v">'+esc(HOSTINFO.lastSync)+'</span></div>';
  h += '<div class="hsec"><h5>这台设备</h5>' +
    '<div class="kvrow"><span class="k">名称</span><span class="v" style="font-family:var(--sans)">' +
    esc(HOSTINFO.deviceName)+' · '+esc(HOSTINFO.clientKind)+'</span></div>' +
    '<div class="kvrow"><span class="k">权限</span><span class="v" style="font-family:var(--sans)">' +
    SCOPES.map(function(s){ return esc(s.label)+'　'; }).join('')+'</span></div></div>';
  h += '<div class="hsec"><h5>主机身份</h5>' +
    '<div class="kvrow"><span class="k">地址</span><span class="v">'+esc(HOSTINFO.address)+'</span></div>' +
    '<div class="kvrow"><span class="k">指纹</span><span class="v">'+esc(HOSTINFO.fingerprint)+'</span></div>' +
    '<p class="hnote">仅支持直连一台电脑，不展示经其他节点转发的会话；网页端不能新增或撤销权限。</p></div>';
  h += '<div style="padding-top:14px;display:flex;flex-direction:column;gap:10px">' +
    '<button class="btn block" data-act="repair">重新配对</button>' +
    '<button class="btn block" data-act="clear">清除这台设备的数据</button></div>';
  if(A.clearArmed){
    h = '<div class="hconfirm"><b>确认清除？</b>会删除本浏览器的身份密钥与同步元数据，之后必须重新配对。' +
      '<b>电脑端仍会保留这台设备的记录。</b><div style="display:flex;gap:10px;margin-top:12px">' +
      '<button class="btn stop" style="flex:1" data-act="clear-yes">确认清除</button>' +
      '<button class="btn" style="flex:1" data-act="clear-no">取消</button></div></div>' + h;
  }
  return h;
}
/* 新建会话弹层：目录层级进来时目录已知（不需要指定）；顶部「新建」进来时要指定目录，
   而且允许指定电脑上还没有的目录——那种情况下创建会话会先新建目录。
   两种入口都在「目录 / 路径」两行里呈现，字体样式一致。 */
function newSessionSheet(){
  var fixed = sheetState.key ? dirById(sheetState.key) : null;
  var path = dirPath();
  var head;
  if(fixed){
    head = '<div class="dirinfo">' +
      '<div class="drow"><span class="dk">目录</span><span class="dv">'+esc(fixed.name)+'</span></div>' +
      '<div class="drow"><span class="dk">路径</span><span class="dv mono">'+esc(fixed.path||"—")+'</span></div>' +
    '</div>';
  } else {
    var name = path ? dirNameOf(path) : "";
    head = '<div class="dirinfo">' +
      '<div class="drow"><span class="dk">目录</span><span class="dv"><span class="filec">' +
        '<button class="filecbtn" data-dirpickopen>选择文件夹</button>' +
        '<span class="filecval'+(name?"":" ph")+'" id="shDirVal">'+esc(name||"未选择任何文件")+'</span>' +
      '</span></span></div>' +
      '<div class="drow"><span class="dk">路径</span><span class="dv mono">'+esc(path||"—")+'</span></div>' +
      '<div class="hint">点「选择文件夹」浏览这台电脑上的目录并选中一层；也可以在里面新建文件夹——' +
      '新建出来的目录，创建会话时会先在电脑上建它。</div>' +
    '</div>' +
    '<div id="shDirNotice">'+dirBranchNotice(path)+'</div>';
  }
  var where = fixed ? (fixed.path || fixed.name) : path;
  return head +
    '<h2 style="margin-bottom:10px">选择 Agent</h2>' +
    AGENT_PROFILES.map(function(a){
      return '<div class="agentopt'+(A.newAgent===a.id?" on":"")+'" data-agent="'+a.id+'">' +
        '<span class="radio"></span><span><span style="font-weight:600">'+esc(a.name)+'</span>'+
        (a.def?' <span class="hint">默认</span>':'')+'<br><span class="hint">已配置 · 凭据已就绪</span></span></div>';
    }).join("") +
    '<div class="notice info"><span>ⓘ</span><span>会话标题会在你发出第一条消息后由 Agent 自动设置。</span></div>' +
    (A.newAgent?'<div class="notice warn"><span>⚠</span><span>将在你的电脑上启动一个 '+esc(A.newAgent)+
      ' 进程，工作目录 '+esc(where||"—")+'。</span></div>':'') +
    (A.newErr?'<div class="errbox">'+A.newErr+'</div>':'') +
    '<button class="btn block primary" style="margin-top:14px" data-act="create">创建会话</button>';
}

/* ============ 主题（跟随系统 / 白天 / 黑夜） ============ */
var THEME_KEY = "acpr-proto-theme";
function storedTheme(){ try{ return localStorage.getItem(THEME_KEY) || "system"; }catch(e){ return "system"; } }
function effTheme(){
  return (storedTheme() === "system")
    ? ((window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches) ? "dark" : "light")
    : storedTheme();
}
function applyTheme(mode){
  var eff = mode;
  if(mode === "system"){
    eff = (window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches) ? "dark" : "light";
  }
  document.documentElement.setAttribute("data-theme", eff);
  var meta = document.querySelector('meta[name="theme-color"]');
  if(meta) meta.setAttribute("content", eff === "dark" ? "#0e1116" : "#ffffff");
  try{ localStorage.setItem(THEME_KEY, mode); }catch(e){}
  if($("themeSel")) $("themeSel").value = mode;
}
if(window.matchMedia){
  var mq = window.matchMedia("(prefers-color-scheme: dark)");
  var onSys = function(){ if(storedTheme() === "system") applyTheme("system"); };
  if(mq.addEventListener) mq.addEventListener("change", onSys); else if(mq.addListener) mq.addListener(onSys);
}
$("themeSel").value = storedTheme();
$("themeSel").onchange = function(){ applyTheme(this.value); render(); };

/* ============ 跳到用户输入（移动端：手动定位，不用 scrollIntoView） ============ */
function updateJump(){
  var st = $("stream"), pill = $("jumpPill");
  if(!st || !pill) return;
  if(A.tab !== "chat"){ pill.style.display = "none"; return; }
  var msgs = Array.prototype.slice.call(st.querySelectorAll("[data-umsg]"));
  if(!msgs.length){ pill.style.display = "none"; return; }
  var stTop = st.scrollTop, stH = st.clientHeight, c = st.getBoundingClientRect();
  var vis = false, above = 0, below = 0;
  msgs.forEach(function(m){
    var r = m.getBoundingClientRect(), top = r.top - c.top, bot = r.bottom - c.top;
    if(bot > stTop + 12 && top < stTop + stH - 12) vis = true;
    else if(bot <= stTop + 12) above++; else below++;
  });
  if(vis || (above === 0 && below === 0)){ pill.style.display = "none"; pill.dataset.dir = ""; return; }
  pill.style.display = "flex";
  pill.innerHTML = above > 0 ? "↑ 回到你最近的输入" : "↓ 看你下一条输入";
  pill.dataset.dir = above > 0 ? "up" : "down";
}
function jumpToUser(){
  var st = $("stream"), pill = $("jumpPill");
  if(!st || !pill || !pill.dataset.dir) return;
  var msgs = Array.prototype.slice.call(st.querySelectorAll("[data-umsg]"));
  if(!msgs.length) return;
  var c = st.getBoundingClientRect(), target = null;
  if(pill.dataset.dir === "up"){
    for(var i = msgs.length - 1; i >= 0; i--){
      if(msgs[i].getBoundingClientRect().bottom - c.top <= st.scrollTop + 12){ target = msgs[i]; break; }
    }
    if(!target) target = msgs[msgs.length - 1];
  } else {
    for(var j = 0; j < msgs.length; j++){
      if(msgs[j].getBoundingClientRect().top - c.top > st.scrollTop + st.clientHeight - 12){ target = msgs[j]; break; }
    }
    if(!target) target = msgs[msgs.length - 1];
  }
  if(!target) return;
  /* 手动 scrollTop：移动端对 scrollIntoView(block:"center") 支持不一致 */
  st.scrollTop = (target.offsetTop - st.clientHeight / 2) + target.offsetHeight / 2;
  toast("已定位到你这条消息");
}

/* ============ 路由 ============ */
function render(){
  var h = location.hash || "#/dirs";
  if(h.indexOf("#/pair")===0)      $("frame").innerHTML = viewPair();
  else if(h.indexOf("#/sessions")===0) $("frame").innerHTML = viewSessions();
  else if(h.indexOf("#/chat/")===0)   { A.tab = A.tab || "chat"; $("frame").innerHTML = viewChat(h.slice(7)); }
  else if(h.indexOf("#/dir/")===0)    $("frame").innerHTML = viewDirDetail(h.slice(6));
  else                               $("frame").innerHTML = viewDirs();
  bind();
  document.querySelectorAll("#navRow button").forEach(function(b){
    b.classList.toggle("cur", (b.dataset.go||"") === h);
  });
}
function bind(){
  var f = $("frame");
  /* 一次性绑定：#frame 是常驻元素，每次 render 都 addEventListener 会累积，
     导致一次点击被处理 N 次（主题切换这种 toggle 会被抵消，表现为「有时失效」） */
  if(!f.__bound){ f.__bound = true;
  f.addEventListener("click", function(ev){
    var t = ev.target;
    var go = t.closest("[data-go]");
    if(go){ closeSheet(); location.hash = go.dataset.go; return; }
    var act = t.closest("[data-act]");
    if(act){ handleAct(act.dataset.act); return; }
    var all = t.closest("[data-all]");
    if(all){ location.hash = "#/dir/"+all.dataset.all; return; }
    var nw = t.closest("[data-new]");
    if(nw){ A.newDir=""; A.dirPickOpen=false; A.newErr=null; openSheet("new", nw.dataset.new); return; }
    var ag = t.closest("[data-agent]");
    if(ag){ A.newAgent = ag.dataset.agent; A.newErr = null; paintSheet(); return; }
    var sk = t.closest("[data-seckey]");
    if(sk){ sheetState.sec[sk.dataset.seckey] = !sheetState.sec[sk.dataset.seckey]; paintSheet(); return; }
    var fd = t.closest("[data-file]");
    if(fd){ A.selFile = fd.dataset.file; A.tab = "files"; render(); return; }
    var fc = t.closest(".fold[data-file]");
    if(fc){ A.selFile = fc.dataset.file; A.tab = "files"; render(); return; }
    var dx = t.closest("[data-dx]");
    if(dx){ openSheet("dx", dx.dataset.dx); return; }
    var dg = t.closest("[data-diag]");
    if(dg){ openSheet("dx", dg.getAttribute("data-diag")||"tool"); return; }
    var pm = t.closest("[data-perm]");
    if(pm){ A.permSet[pm.dataset.sess] = pm.dataset.perm; render(); toast("授权已提交，等待终态事件"); return; }
    var tb = t.closest("[data-tab]");
    if(tb){ A.tab = tb.dataset.tab; render(); return; }
    var mt = t.closest("[data-pagedir]");
    if(mt){ if(mt.disabled) return; goToPage(A.page + (+mt.dataset.pagedir)); return; }
    var ss = t.closest("[data-session]");
    if(ss){ A.session = ss.dataset.session; A.tab = "chat"; location.hash = "#/chat/"+ss.dataset.session; return; }
    var ft = t.closest("[data-f]");
    if(ft){ A.filter = ft.dataset.f; render(); return; }
    var tg = t.closest("[data-toggle]");
    if(tg){ A.open[tg.dataset.toggle] = !A.open[tg.dataset.toggle]; render(); return; }
  });
  }
  var q = f.querySelector("[data-q]");
  if(q) q.addEventListener("input", function(){ A.query = this.value; render();
    var el = $("frame").querySelector("[data-q]"); if(el){ el.focus(); el.setSelectionRange(el.value.length, el.value.length); } });
  var ta = f.querySelector("#ta");
  if(ta) ta.addEventListener("keydown", function(e){
    if(e.key==="Enter" && !e.shiftKey){ e.preventDefault(); handleAct("send"); }
  });
  var pill = f.querySelector("#jumpPill");
  if(pill){ pill.onclick = jumpToUser; }
  /* 详情页翻页：上一页/下一页用相对方向；页码输入框回车/失焦时跳转，非法或越界的值原地纠正、不重画 */
  var pi = f.querySelector("#pageInput");
  if(pi){
    var commitPage = function(){
      var n = parseInt(this.value, 10);
      if(isNaN(n)){ this.value = A.page; return; }
      var p = Math.min(A.pageCount || 1, Math.max(1, n));
      this.value = p;
      goToPage(p);
    };
    pi.onkeydown=function(e){ if(e.key==="Enter"){ e.preventDefault(); commitPage.call(this); } };
    pi.onchange=commitPage;
  }
  var stEl = f.querySelector("#stream");
  if(stEl){ stEl.addEventListener("scroll", updateJump); }
  requestAnimationFrame(updateJump);
  if(sheetState.type) paintSheet();
}
function handleAct(a){
  if(a==="back"){
    if($("sheet").classList.contains("open")){ closeSheet(); return; }
    /* 用应用内路由栈判定「有没有上一页」：没有就回目录页，而不是退出应用 */
    if(A.navStack.length >= 2){ A.navStack.pop(); location.hash = A.navStack[A.navStack.length-1]; return; }
    location.hash = "#/dirs"; return;
  }
  if(a==="theme"){ applyTheme(effTheme() === "dark" ? "light" : "dark"); render(); return; }
  if(a==="host"){ openSheet("host"); return; }
  /* 顶部「新建」：要自己指定目录（可以是电脑上还没有的目录）。 */
  if(a==="newdir"){ A.newDir=""; A.dirPickOpen=false; A.newErr=null; openSheet("new", null); return; }
  if(a==="sessions"){ location.hash="#/sessions"; return; }
  if(a==="send"){
    if(isClosed()){ toast("会话已关闭"); return; }
    if(active()){ toast("进行中，完成后再发送（可先点「停止」）"); return; }
    var ta=$("ta"); if(!ta||!ta.value.trim()) return; ta.value="";
    toast("已提交（accepted ≠ 完成，等待终态事件）"); return;
  }
  if(a==="stop"){ if(cur().id==="s1"){ A.chatScenario="idle"; $("scenSel").value="idle"; } render();
    toast("已请求取消当前 turn（等待终态事件）"); return; }
  if(a==="mode"){ pickMode(); return; }
  if(a==="model"){ pickModel(); return; }
  if(a==="ctx"){ openSheet("host"); return; }
  if(a==="repair"){ closeSheet(); location.hash="#/pair/S4"; return; }
  if(a==="clear"){ A.clearArmed=true; paintSheet(); return; }
  if(a==="clear-no"){ A.clearArmed=false; paintSheet(); return; }
  if(a==="clear-yes"){ A.clearArmed=false; closeSheet(); location.hash="#/pair/S4";
    toast("本机数据已清除；下次使用需要重新配对"); return; }
  if(a==="create"){
    if(A.creating) return;
    var fixed = sheetState.key ? dirById(sheetState.key) : null;
    var path = fixed ? "" : dirPath();
    if(!fixed && !path){
      A.newErr = "请先选择目录：点「选择文件夹」浏览这台电脑上的目录，选中一层。";
      paintSheet(); return;
    }
    if(path && (!isAbsPath(path) || path.indexOf("..")>=0)){
      A.newErr = "服务端拒绝了这个目录：必须是绝对路径，且不能包含「..」（参数类错误 protocol.schema_invalid）。" +
        "没有创建会话，也没有触碰电脑上的文件系统。";
      paintSheet(); return;
    }
    if(!A.newAgent){ A.newErr = "请先选择一个 Agent。"; paintSheet(); return; }
    if(path && dirCaseKind()==="blocked"){
      A.newErr = "无法在这个目录创建会话：父目录不存在，或没有写入权限（服务端不可用类错误 internal.unavailable）。" +
        "没有新建目录，也没有创建会话。";
      paintSheet(); return;
    }
    /* 这一步是顶部入口独有的：指定的目录在电脑上还不存在时要先新建它，再启动 Agent。 */
    var willCreateDir = !!path && dirCaseKind()==="missing";
    var target = fixed ? fixed.id : "unsorted";
    closeSheet();
    A.creating = true; A.newErr = null;
    if(willCreateDir) toast("正在电脑上新建目录 " + path);
    setTimeout(function(){
      A.seq++; var id = "ns-"+A.seq;
      SESSIONS.unshift(mk(id, null, A.newAgent, "idle", NOW, target));
      A.creating = null; A.session = id; A.tab = "chat";
      location.hash = "#/chat/"+id; render();
      if(willCreateDir) toast("已在电脑上新建目录 " + path);
    }, 1100);
    return;
  }
  if(a==="copy"){
    var e = EVT[sheetState.key]; if(!e) return;
    var text = e.rawJson || JSON.stringify(e.rawUnavailable, null, 2);
    var clip = (typeof navigator!=="undefined") ? navigator.clipboard : null;
    if(!clip||!clip.writeText){ toast("复制失败：需要 HTTPS 或 localhost"); return; }
    clip.writeText(text).then(function(){ toast("已复制（按不可信文本处理）"); },
      function(){ toast("复制失败：浏览器要求 HTTPS 或 localhost"); });
  }
}
function openPickSheet(title, headHtml, bodyHtml, onPick){
  $("sheetTitle").textContent = title;
  $("sheetBody").innerHTML = headHtml + bodyHtml;
  $("sheetBody").onclick = function(ev){
    var p = ev.target.closest("[data-pick]"); if(!p) return;
    onPick(p.dataset.pick); closeSheet(); render();
  };
  $("sheet").classList.add("open"); $("scrim").classList.add("open");
  pushSheetGuard();
  $("sheetClose").textContent = "取消";
}
function pickRows(refs, currentId){
  return refs.map(function(r){
    return '<div class="agentopt'+(r.id===currentId?" on":"")+'" data-pick="'+esc(r.id)+'">'+
      '<span class="radio"></span><span>'+esc(r.name)+
      (r.description ? '<br><span style="font-size:11.5px;color:var(--ink-3)">'+esc(r.description)+'</span>' : '')+
      '</span></div>';
  }).join("");
}
/* 会话模式：候选由 Agent 给出（`session.mode.list` 的 `availableModes`），这里只把 Agent 给的名字显示出来、
   把选中的 `modeId` 交回去；候选为空时不编造，直接说明这个 Agent 没提供会话模式。 */
function pickMode(){
  var ms = modeState();
  if(!ms.availableModes.length){ toast("这个 Agent 没有提供会话模式"); return; }
  openPickSheet("会话模式", pickHead("会话模式"),
    pickRows(ms.availableModes.map(function(m){ return { id:m.id, name:m.name }; }), ms.currentModeId),
    function(id){ A.modeSel[A.session] = id;
      toast("模式已切换到「"+modeLabel(modeState())+"」，下一轮生效"); });
}
/* 模型同理：候选取自 Agent 的 `category=model` 配置项，显示 Agent 的 `name`、回传 `value`。 */
function pickModel(){
  var ms = modelState();
  if(!ms.options.length){ toast("这个 Agent 没有提供模型选择项"); return; }
  openPickSheet("选择模型", pickHead("选择模型"),
    pickRows(ms.options.map(function(o){ return { id:o.value, name:o.name, description:o.description }; }), ms.currentValue),
    function(value){ A.modelSel[A.session] = value;
      toast("模型已切换到「"+modelLabel(modelState())+"」，下一轮生效"); });
}
function pickHead(title){
  return '<h2 style="margin-bottom:10px">'+title+'</h2>'+
    '<p style="margin:-4px 0 10px;color:var(--ink-3);font-size:12.5px">由 Agent 提供，下一轮生效</p>';
}

/* ============ 交互与控制台 ============ */
/* 弹层自己的交互：`#sheet` 是 `#frame` 的兄弟节点，`bind()` 挂在 `#frame` 上的那套委托看不到它，
   因此这里单独委托一次（弹层里的 Agent 选择、`data-act` 与分区折叠此前都点不动）。
   `[data-pick]` 仍由 `openPickSheet` 自己的处理器负责。 */
$("sheetBody").addEventListener("click", function(ev){
  var t = ev.target;
  var go = t.closest("[data-go]");
  if(go){ var h = go.dataset.go; if(h) closeSheetAndGo(h); else closeSheet(); return; }
  if(t.closest("[data-dirpickopen]")){ A.newErr = null; openDirPicker(); return; }
  var ag = t.closest("[data-agent]");
  if(ag){ A.newAgent = ag.dataset.agent; A.newErr = null; paintSheet(); return; }
  var act = t.closest("[data-act]");
  if(act){ handleAct(act.dataset.act); return; }
  var sk = t.closest("[data-seckey]");
  if(sk){ sheetState.sec[sk.dataset.seckey] = !sheetState.sec[sk.dataset.seckey]; paintSheet(); return; }
});

/* 系统返回 / 右划：弹层开着就只关弹层（这条记录已经被系统弹掉了，不要再退一层） */
window.addEventListener("popstate", function(){
  if($("sheet").classList.contains("open")){
    if(sheetGuards > 0) sheetGuards--;
    closeSheet();
    return;
  }
  flushPendingNav();
});

/* ============ 「选择文件夹」弹窗（顶部入口用） ============ */
/* 浏览远端节点上的目录来选：面包屑 + 这一层的子目录 + 上一级；也可以在当前位置新建文件夹
   （新建的文件夹在创建会话时才真的建，所以走「会先新建」那一支）。 */
function openDirPicker(){
  A.dirPickOpen = true; A.pickKind = "existing"; A.newFolderOpen = false; A.newFolderName = "";
  A.browsePath = (isAbsPath(A.newDir) && isBrowsable(A.newDir)) ? normPath(A.newDir) : "";
  paintDirPicker();
  $("dirPick").classList.add("open");
}
function closeDirPicker(){ A.dirPickOpen = false; $("dirPick").classList.remove("open"); }
function paintDirPicker(){
  var here = isBrowsable(A.browsePath) ? normPath(A.browsePath) : "";
  A.browsePath = here;
  var atComputer = (here === "");
  /* 面包屑（第一段是「此电脑」；盘符根上一层就是它）+ 上一级 */
  $("dpCrumb").innerHTML = crumbs(here).map(function(c,i){
    return (i? '<span class="sep">›</span>' : '') +
      '<button data-crumb="'+esc(c.path)+'">'+esc(c.name)+'</button>';
  }).join("") + (parentOf(here) !== null ? '<button class="dpup" data-up>↑ 上一级</button>' : '');
  /* 这一层：此电脑下面列盘符，盘符下面列子目录；已登记的目录就地标出来 */
  var kids = childPaths(here);
  $("dpList").innerHTML = kids.length ? kids.map(function(p){
    var reg = registeredByPath(p);
    return '<button class="diropt" data-enter="'+esc(p)+'">' +
      '<span class="dn">'+esc(atComputer ? p : dirNameOf(p))+'</span>' +
      '<span class="da">'+(reg ? '已登记' : (atComputer ? esc(driveLabel(p)) : ''))+'</span>' +
      '<span class="ch">›</span></button>';
  }).join("") : '<div class="dpempty">这一层没有子文件夹</div>';
  /* 新建文件夹：只有进了某个盘之后才能建 */
  $("dpNewRow").innerHTML = atComputer ? '' : (A.newFolderOpen
    ? '<div class="dpnew"><input id="dpNewName" type="text" value="'+esc(A.newFolderName)+'" placeholder="新文件夹的名字" />' +
      '<button class="btn sm primary" id="dpNewOk">新建并选择</button>' +
      '<button class="btn sm" id="dpNewCancel">取消</button></div>'
    : '<button class="diropt other" data-newfolder><span class="dn">＋ 新建文件夹…</span>' +
      '<span class="da">在这里新建一个文件夹；创建会话时会先在电脑上建它</span></button>');
  $("dpCur").textContent = atComputer ? "此电脑（先选一个盘）" : ("当前位置：" + here);
  $("dpOk").disabled = atComputer;   /* 此电脑本身不是一个目录，不能选 */
  bindDirPicker();
}
function bindDirPicker(){
  $("dpCrumb").onclick = function(ev){
    var c = ev.target.closest("[data-crumb]");
    if(c){ A.browsePath = c.dataset.crumb; paintDirPicker(); return; }
    if(ev.target.closest("[data-up]")){
      var up = parentOf(A.browsePath);
      if(up){ A.browsePath = up; paintDirPicker(); }
    }
  };
  $("dpList").onclick = function(ev){
    var e = ev.target.closest("[data-enter]");
    if(e){ A.browsePath = e.dataset.enter; paintDirPicker(); }
  };
  $("dpNewRow").onclick = function(ev){
    if(ev.target.closest("[data-newfolder]")){
      A.newFolderOpen = true; A.newFolderName = ""; paintDirPicker();
      var i = $("dpNewName"); if(i) i.focus();
      return;
    }
    if(ev.target.closest("#dpNewCancel")){ A.newFolderOpen = false; paintDirPicker(); }
  };
  var nn = $("dpNewName");
  if(nn){
    nn.oninput = function(){ A.newFolderName = this.value; };
    nn.onkeydown = function(e){ if(e.key==="Enter"){ e.preventDefault(); createFolderAndPick(); } };
  }
  if($("dpNewOk")) $("dpNewOk").onclick = createFolderAndPick;
}
function createFolderAndPick(){
  var name = (A.newFolderName||"").trim();
  if(!name || /[\\\/:]/.test(name)){ toast("文件夹名字不能为空，也不能含 \\ / :"); return; }
  A.pickKind = "create";
  applyDirPicker(joinPath(A.browsePath, name));
}
function applyDirPicker(path){
  var p = normPath(path);
  if(!p) return;                       /* 此电脑本身不是目录，选不了 */
  A.newDir = p;
  A.newErr = null;
  closeDirPicker();
  if(sheetState.type === "new") paintSheet();
}
$("dpCancel").onclick = closeDirPicker;
/* 「选择此文件夹」= 选当前所在的那一层（浏览到哪一层就选哪一层，跟系统文件夹选择器一致）。 */
$("dpOk").onclick = function(){ A.pickKind = "existing"; applyDirPicker(A.browsePath); };

$("sheetClose").onclick = closeSheet;
$("scrim").onclick = closeSheet;
document.addEventListener("keydown", function(e){ if(e.key==="Escape"){
  if(A.dirPickOpen){ closeDirPicker(); return; }   /* 弹窗开着时先关弹窗，不关底下的新建弹层 */
  closeSheet(); } });
$("consoleHead").onclick = function(){ $("console").classList.toggle("collapsed"); };
$("navRow").onclick = function(e){ var b = e.target.closest("[data-go]"); if(b) location.hash = b.dataset.go; };
$("connSel").onchange = function(){ A.conn = this.value; A.clearArmed = false; render(); };
$("scenSel").onchange = function(){ A.chatScenario = this.value; render(); };
$("ctxSel").onchange  = function(){ A.ctx = this.value; render(); };
/* 控制台只决定「这台电脑能不能为这次创建新建目录」。 */
$("mkdirSel").onchange = function(){ A.mkdirCase = this.value;
  if(A.dirPickOpen) paintDirPicker();
  else if(sheetState.type === "new" && !sheetState.key) paintSheet(); };
$("cbDeg").onchange    = function(){ A.degradeDemo = this.checked; render(); };
/* 应用内路由栈：判定「有没有上一页」只看我们自己走过的路由。
   直接打开深链（分享链接/通知/图标进入某会话）时 history.length 也 >1——那是标签页的初始条目，
   照它判定会把用户带出应用（实测会落到 about:blank），所以不能再用 history.length。 */
function trackRoute(h){
  var st = A.navStack;
  if(st.length >= 2 && st[st.length-2] === h){ st.pop(); return; }   /* 这是回退 */
  if(st[st.length-1] !== h) st.push(h);
}
window.addEventListener("hashchange", function(){ trackRoute(location.hash || "#/dirs"); A.query=""; A.filter="all"; A.page=1; render(); });

if(!location.hash) location.hash = "#/dirs";
A.navStack = [location.hash || "#/dirs"];   /* 起点：当前这一页就是栈底 */
render();
setTimeout(function(){
  var st = $("stream");
  if(st) st.scrollTop = st.scrollHeight;   /* 进入对话默认停在最新 */
  updateJump();
}, 60);
})();
/* ============ 以下复用桌面原型的纯逻辑函数（Markdown / 高亮 / 折叠 / 降级 / 主机面板） ============ */
var HL_RE = /(\/\/[^\n]*)|("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`)|([A-Za-z_$][\w$]*)|(\b\d+(?:\.\d+)?\b)|(#[A-Za-z_][\w]*)|(@[A-Za-z_][\w]*)/g;
var HL_KW = {function:1,const:1,let:1,var:1,return:1,if:1,else:1,for:1,while:1,new:1,await:1,async:1,
  export:1,import:1,from:1,class:1,extends:1,typeof:1,instanceof:1,null:1,undefined:1,true:1,false:1,
  this:1,try:1,catch:1,finally:1,throw:1,interface:1,type:1,as:1,implements:1,public:1,private:1,
  readonly:1,void:1,switch:1,case:1,break:1,continue:1,of:1,in:1,yield:1,static:1,enum:1};

function cur(){ return SESSIONS.filter(function(x){ return x.id===A.session; })[0] || SESSIONS[0]; }
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
function stateChip(){
  var st=turnState();
  if(st==="running"||st==="queued") return '<span class="ab-badge ok">● 运行中</span>';
  if(st==="waiting_permission") return '<span class="ab-badge warn">⏸ 等待授权</span>';
  if(st==="waiting_input") return '<span class="ab-badge warn">⏸ 等待回复</span>';
  if(st==="failed") return '<span class="ab-badge danger">✕ 失败</span>';
  if(st==="closed") return '<span class="ab-badge">○ 已关闭</span>';
  return '<span class="ab-badge">○ 空闲</span>';
}
function dirById(id){ return DIRS.filter(function(d){ return d.id===id; })[0]; }
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
    (right||'')+'</div>';
}

function viewDirs(){
  var h = appbar('目录', HOSTNAME + ' · 点此打开主机与连接', '<button class="ab-btn" data-act="host">⋯</button>', false);
  var body = '<div class="scroll">';
  var q = A.query;
  var matched = DIRS.filter(function(d){
    if(!q) return true;
    var any = SESSIONS.some(function(s){ return s.dir===d.id && passFilter(s) &&
      (s.title||"未命名会话").toLowerCase().indexOf(q)>=0; });
    return d.name.toLowerCase().indexOf(q)>=0 || any;
  });
  if(!matched.length){ body += '<div class="blank"><h2>没有匹配的结果</h2><p>换个关键词。</p></div>'; }
  matched.forEach(function(dir){
    var all = SESSIONS.filter(function(s){ return s.dir===dir.id; });
    var rows = all.filter(passFilter);
    if(!rows.length && q) return;
    var active0 = all.filter(function(s){ return s.state==="running"||s.state==="queued"; }).length;
    body += '<div class="card"><div class="dhead">' +
      '<span class="dicon">▤</span><span class="dname">'+esc(dir.name)+'</span>' +
      (active0?'<span class="chip" style="color:var(--ok);border-color:transparent;background:var(--ok-soft)">运行中 '+active0+'</span>':'') +
      '<span class="chip">'+all.length+' 个</span></div><div class="dbody">' +
      rows.slice(0,5).map(function(s){ return srow(s); }).join("") +
      (all.length>5 ? '<div style="padding:2px 0"><button class="linkbtn" data-all="'+dir.id+'">查看全部 '+all.length+' 个 ›</button></div>' : '') +
      (!all.length ? '<div style="padding:12px 14px;color:var(--ink-3);font-size:13px">这个目录还没有会话</div>' : '') +
      '<div style="padding:4px 0"><button class="linkbtn" data-new="'+dir.id+'">⊕ 新建会话</button></div>' +
      '</div></div>';
  });
  body += '</div>';
  return h + '<div style="flex:1;display:flex;flex-direction:column;min-height:0">' +
    '<div style="padding:10px 12px 0"><div class="search"><span>🔍</span>' +
    '<input data-q value="'+esc(A.query)+'" placeholder="搜索目录或会话" /></div>' +
    '<div class="segbar"><button data-f="all" class="'+(A.filter==="all"?"on":"")+'">全部</button>' +
    '<button data-f="running" class="'+(A.filter==="running"?"on":"")+'">运行中</button>' +
    '<button data-f="waiting" class="'+(A.filter==="waiting"?"on":"")+'">待处理</button></div></div>' + body + '</div>';
}
function srow(s){
  var name = s.title ? esc(s.title) : '<span class="noname">未命名会话</span>';
  var st = s.state==="waiting_permission" ? '<span class="st">待授权</span>' : '';
  return '<div class="srow'+(s.id===A.session?" on":"")+'" data-session="'+s.id+'">' +
    '<span class="dot '+s.state+'"></span><span class="t'+(s.title?"":" noname")+'">'+name+'</span>'+st+
    '<span class="w">'+esc(s.when)+'</span></div>';
}

function viewDirDetail(id){
  var dir = dirById(id);
  if(!dir) return viewDirs();
  var all = SESSIONS.filter(function(s){ return s.dir===id; });
  var rows = all.filter(passFilter);
  var h = appbar(dir.name, (dir.path||"") + ' · ' + rows.length + ' 个会话', '', true);
  var body = '<div class="scroll">';
  /* 月份分档（横向滚动的分段控件，触屏比下拉合适） */
  var months = [];
  rows.forEach(function(s){ var m = monthLabel(s.when); if(months.indexOf(m)<0) months.push(m); });
  if(rows.length > 6 && months.length > 1){
    body += '<div class="segbar">' +
      '<button data-month="all" class="'+(A.month==="all"?"on":"")+'">全部</button>' +
      months.map(function(m){ return '<button data-month="'+esc(m)+'" class="'+(A.month===m?"on":"")+'">'+esc(m)+'</button>'; }).join("") +
      '</div>';
  }
  var shown = A.month==="all" ? rows : rows.filter(function(s){ return monthLabel(s.when)===A.month; });
  if(!rows.length) body += '<div class="blank"><h2>没有符合条件的会话</h2></div>';
  else if(!shown.length) body += '<div class="blank"><h2>这个月没有会话</h2></div>';
  else {
    var last = null;
    shown.forEach(function(s){
      var m = monthLabel(s.when);
      if(m !== last){ body += '<div style="padding:14px 4px 6px;font-size:12px;color:var(--ink-3);font-weight:600">'+esc(m)+'</div>'; last = m; }
      body += srow(s);
    });
    body += '<div style="padding:16px 0;text-align:center;font-size:12px;color:var(--ink-3)">已显示全部 '+rows.length+' 个会话</div>';
  }
  body += '<div style="padding:6px 0 20px"><button class="btn block primary" data-new="'+dir.id+'">⊕ 在此目录新建会话</button>' +
    '<p class="hint" style="text-align:center;margin-top:8px">将在你的电脑上启动一个 Agent 进程</p></div>';
  body += '</div>';
  return h + body;
}
function monthLabel(when){
  if(/今天|昨天|分钟前|刚刚/.test(when)) return "今天";
  var m = /(\\d+)月(\\d+)日/.exec(when);
  return m ? (m[1]+"月") : when;
}

function viewSessions(){
  var s = cur(), dir = dirById(s.dir);
  var list = SESSIONS.filter(function(x){ return x.dir===s.dir; });
  var h = appbar('本目录的会话', (dir?dir.name:"") + (dir&&dir.path?' · '+dir.path:''), '', true);
  return h + '<div class="scroll"><div class="search"><span>🔍</span>' +
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
    out += userMsg("（这个会话的最近一条输入）", s.when) +
      agentMsg("这是 `"+s.agent+"` 在 "+(dirById(s.dir)?dirById(s.dir).name:"")+" 下的会话。", s.when) +
      fold("g.think","think","🧠","思考","done","示例思考内容。",null,s.when);
    if(s.state==="waiting_permission") out += permCard(s);
  }
  /* 「跳到你最近的输入」胶囊：滚动离开用户消息时浮出（移动端需手动吸附到底部） */
  out += '<button class="jumppill" id="jumpPill"></button>';
  return out;
}

function chatPane(s){
  var c = CONN[A.conn] || CONN.online;
  var head = appbar(esc(s.title||"未命名会话"),
    esc(s.agent)+' · '+(dirById(s.dir)?dirById(s.dir).name:""),
    stateChip()+'<button class="ab-btn" data-act="sessions">☰</button>');
  var strip = c.strip ? '<div class="connstrip'+(c.strip[1]?" warn":"")+'"><span class="spin"></span>'+
    esc(c.strip[0])+'</div>' : '';
  var body = '<div class="scroll" id="stream">'+streamHTML(s)+
    '<div style="height:8px"></div></div>';
  var tabbar = '<div class="tabbar"><button data-tab="chat" class="'+(A.tab==="chat"?"on":"")+'">对话</button>' +
    '<button data-tab="files" class="'+(A.tab==="files"?"on":"")+'">变更文件 ('+FILE_ORDER.length+')</button></div>';
  var composer = '<div class="composer">' + ctxWarn() +
    '<div class="chips">' + connChip() +
      '<button class="chip-pick" data-act="mode">模式 <b>'+esc(A.mode)+'</b> ▾</button>' +
      '<button class="chip-pick" data-act="model">模型 <b>'+esc(A.model)+'</b> ▾</button>' +
    '</div>' +
    '<div class="inputrow"><textarea id="ta"'+(active()||isClosed()?' readonly':'')+' placeholder="'+
      (isClosed()?"会话已关闭":(active()?"进行中，完成后可继续输入":"给 Agent 发消息"))+'"></textarea>' +
      (active()?'<button class="btn stop" data-act="stop">■ 停止</button>'
               :'<button class="btn primary" data-act="send">发送</button>')+'</div>' +
    '<div class="hintrow"><span>Enter 发送 · Shift+Enter 换行</span><span>📎 附件暂不支持</span></div></div>';
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
function openSheet(type, key){
  sheetState.type = type; sheetState.key = key || null; sheetState.sec = {};
  paintSheet();
  $("sheet").classList.add("open"); $("scrim").classList.add("open");
}
function closeSheet(){ $("sheet").classList.remove("open"); $("scrim").classList.remove("open"); }
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
function newSessionSheet(){
  var dir = dirById(A.creating || cur().dir);
  return '<div class="kvbox" style="margin-bottom:16px"><div class="kvrow"><span class="k">目录</span>' +
    '<span class="v">'+esc(dir?dir.name:"")+'</span></div>' +
    (dir&&dir.path?'<div class="kvrow"><span class="k">路径</span><span class="v">'+esc(dir.path)+'</span></div>':'')+'</div>' +
    '<h2 style="margin-bottom:10px">选择 Agent</h2>' +
    AGENT_PROFILES.map(function(a){
      return '<div class="agentopt'+(A.newAgent===a.id?" on":"")+'" data-agent="'+a.id+'">' +
        '<span class="radio"></span><span><span style="font-weight:600">'+esc(a.name)+'</span>'+
        (a.def?' <span class="hint">默认</span>':'')+'<br><span class="hint">已配置 · 凭据已就绪</span></span></div>';
    }).join("") +
    '<div class="notice info"><span>ⓘ</span><span>会话标题会在你发出第一条消息后由 Agent 自动设置。</span></div>' +
    (A.newAgent?'<div class="notice warn"><span>⚠</span><span>将在你的电脑上启动一个 '+esc(A.newAgent)+
      ' 进程。</span></div>':'') +
    (A.newErr?'<div class="errbox">'+A.newErr+'</div>':'') +
    '<button class="btn block primary" style="margin-top:14px" data-act="create">创建会话</button>';
}

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
  f.addEventListener("click", function(ev){
    var t = ev.target;
    var go = t.closest("[data-go]");
    if(go){ closeSheet(); location.hash = go.dataset.go; return; }
    var act = t.closest("[data-act]");
    if(act){ handleAct(act.dataset.act); return; }
    var all = t.closest("[data-all]");
    if(all){ location.hash = "#/dir/"+all.dataset.all; return; }
    var nw = t.closest("[data-new]");
    if(nw){ A.creating = nw.dataset.new; openSheet("new"); return; }
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
    var mt = t.closest("[data-month]");
    if(mt){ A.month = mt.dataset.month; render(); return; }
    var ss = t.closest("[data-session]");
    if(ss){ A.session = ss.dataset.session; A.tab = "chat"; location.hash = "#/chat/"+ss.dataset.session; return; }
    var ft = t.closest("[data-f]");
    if(ft){ A.filter = ft.dataset.f; render(); return; }
    var tg = t.closest("[data-toggle]");
    if(tg){ A.open[tg.dataset.toggle] = !A.open[tg.dataset.toggle]; render(); return; }
  });
  var q = f.querySelector("[data-q]");
  if(q) q.addEventListener("input", function(){ A.query = this.value; render();
    var el = $("frame").querySelector("[data-q]"); if(el){ el.focus(); el.setSelectionRange(el.value.length, el.value.length); } });
  var ta = f.querySelector("#ta");
  if(ta) ta.addEventListener("keydown", function(e){
    if(e.key==="Enter" && !e.shiftKey){ e.preventDefault(); handleAct("send"); }
  });
  var pill = f.querySelector("#jumpPill");
  if(pill){ pill.onclick = jumpToUser; }
  var stEl = f.querySelector("#stream");
  if(stEl){ stEl.addEventListener("scroll", updateJump); }
  requestAnimationFrame(updateJump);
  if(sheetState.type) paintSheet();
}
function handleAct(a){
  if(a==="back"){ if(sheetState.type){ closeSheet(); return; } history.length > 1 ? history.back() : (location.hash="#/dirs"); return; }
  if(a==="host"){ openSheet("host"); return; }
  if(a==="sessions"){ location.hash="#/sessions"; return; }
  if(a==="send"){
    if(isClosed()){ toast("会话已关闭"); return; }
    if(active()){ toast("进行中，完成后再发送（可先点「停止」）"); return; }
    var ta=$("ta"); if(!ta||!ta.value.trim()) return; ta.value="";
    toast("已提交（accepted ≠ 完成，等待终态事件）"); return;
  }
  if(a==="stop"){ if(cur().id==="s1"){ A.chatScenario="idle"; $("scenSel").value="idle"; } render();
    toast("已请求取消当前 turn（等待终态事件）"); return; }
  if(a==="mode"){ pickList("会话模式", MODES, function(v){ A.mode=v; toast("模式已切换，下一轮生效"); }); return; }
  if(a==="model"){ pickList("选择模型", MODELS, function(v){ A.model=v; toast("模型已切换，下一轮生效"); }); return; }
  if(a==="ctx"){ openSheet("host"); return; }
  if(a==="repair"){ closeSheet(); location.hash="#/pair/S4"; return; }
  if(a==="clear"){ A.clearArmed=true; paintSheet(); return; }
  if(a==="clear-no"){ A.clearArmed=false; paintSheet(); return; }
  if(a==="clear-yes"){ A.clearArmed=false; closeSheet(); location.hash="#/pair/S4";
    toast("本机数据已清除；下次使用需要重新配对"); return; }
  if(a==="create"){
    if(A.creating) return;
    var dirId = A.creating;
    closeSheet(); A.creating = "busy"; render();
    setTimeout(function(){
      A.seq++; var id = "ns-"+A.seq;
      SESSIONS.unshift(mk(id, null, A.newAgent, "idle", "刚刚", dirId));
      A.creating = null; A.session = id; A.tab = "chat";
      location.hash = "#/chat/"+id; render();
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
function pickList(title, items, cb){
  var h = '<h2 style="margin-bottom:10px">'+title+'</h2>' + items.map(function(v){
    return '<div class="agentopt" data-pick="'+esc(v)+'"><span class="radio"></span><span>'+esc(v)+'</span></div>';
  }).join("");
  $("sheetTitle").textContent = title;
  $("sheetBody").innerHTML = h;
  $("sheetBody").onclick = function(ev){
    var p = ev.target.closest("[data-pick]"); if(!p) return;
    cb(p.dataset.pick); closeSheet(); render();
  };
  $("sheet").classList.add("open"); $("scrim").classList.add("open");
  $("sheetClose").textContent = "取消";
}

/* ============ 交互与控制台 ============ */
$("sheetClose").onclick = closeSheet;
$("scrim").onclick = closeSheet;
document.addEventListener("keydown", function(e){ if(e.key==="Escape") closeSheet(); });
$("consoleHead").onclick = function(){ $("console").classList.toggle("collapsed"); };
$("navRow").onclick = function(e){ var b = e.target.closest("[data-go]"); if(b) location.hash = b.dataset.go; };
$("connSel").onchange = function(){ A.conn = this.value; A.clearArmed = false; render(); };
$("scenSel").onchange = function(){ A.chatScenario = this.value; render(); };
$("ctxSel").onchange  = function(){ A.ctx = this.value; render(); };
$("cbDeg").onchange    = function(){ A.degradeDemo = this.checked; render(); };
window.addEventListener("hashchange", function(){ A.query=""; A.filter="all"; render(); });

if(!location.hash) location.hash = "#/dirs";
render();
setTimeout(function(){
  var st = $("stream");
  if(st) st.scrollTop = st.scrollHeight;   /* 进入对话默认停在最新 */
  updateJump();
}, 60);
})();
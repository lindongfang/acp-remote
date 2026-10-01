function esc(s){ return String(s).replace(/[&<>"]/g, function(c){
    return {"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;"}[c]; }); }

function fmtTok(n){
    if(n===null||n===undefined) return "—";
    if(n>=1000000) return (n/1000000).toFixed(1)+"M";
    if(n>=1000) return Math.round(n/1000)+"k";
    return String(n);
  }

function pathCandidates(name, p, keepLast){
    if(!p) return [];
    var sep = p.indexOf("\\")>=0 ? "\\" : "/";
    var parts = p.split(/[\\\/]+/).filter(Boolean);
    if(!keepLast && parts.length && name && parts[parts.length-1].toLowerCase()===name.toLowerCase()) parts.pop();
    if(!parts.length) return [p];
    var out=[];
    for(var k=parts.length;k>=1;k--){
      var s = parts.slice(parts.length-k).join(sep);
      out.push(k===parts.length ? s : "…"+sep+s);
    }
    return out;
  }

function hl(raw){
    var out="", last=0, m; HL_RE.lastIndex=0;
    while((m=HL_RE.exec(raw))!==null){
      out += esc(raw.slice(last,m.index));
      var cls = m[1]?"com":m[2]?"str":m[3]?(HL_KW[m[3]]?"kw":""):m[4]?"num":m[5]?"dec":"kw";
      out += cls ? '<span class="s-'+cls+'">'+esc(m[0])+'</span>' : esc(m[0]);
      last = HL_RE.lastIndex;
    }
    return out + esc(raw.slice(last));
  }

function inlineMd(t){
    var codes=[];
    t=String(t).replace(/`([^`]+)`/g, function(_,c){ codes.push(c); return "C"+(codes.length-1)+""; });
    t=esc(t);
    t=t.replace(/!\[([^\]]*)\]\(([^)\s]+)\)/g,'<span class="imgblk">「$1」已禁用远程图片加载</span>');
    t=t.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, function(_,txt,url){
      if(/^(https?:\/\/|#)/i.test(url)) return '<a href="'+url+'" target="_blank" rel="noopener noreferrer nofollow">'+txt+'</a>';
      return '<span class="badlink">'+txt+'（链接协议不允许）</span>'; });
    t=t.replace(/\*\*([^*]+)\*\*/g,"<strong>$1</strong>");
    t=t.replace(/(^|\W)\*([^*\n]+)\*/g,"$1<em>$2</em>");
    t=t.replace(/C(\d+)/g, function(_,i){ return "<code>"+esc(codes[+i])+"</code>"; });
    return t;
  }

function codeCard(lang, code){
    var body = code.split("\n").map(function(l,i){
      return '<div><span class="ln">'+(i+1)+'</span><span class="tx">'+hl(l)+'</span></div>'; }).join("");
    return '<div class="code"><div class="ch"><span class="fn">'+esc(lang||"text")+'</span>' +
      '<button class="btn sm" data-copy>复制</button></div><pre>'+body+'</pre></div>';
  }

function md(src){
    var fences=[];
    var s=String(src).replace(/\r\n/g,"\n").replace(/```([a-zA-Z0-9+#._-]*)\n([\s\S]*?)```/g,
      function(_,lang,code){ fences.push({lang:(lang||"").trim(),code:code.replace(/\n$/,"")});
        return "\n\u0002"+(fences.length-1)+"\u0002\n"; });
    var lines=s.split("\n"), out=[], i=0;
    function isF(ln){ return /^\u0002\d+\u0002$/.test((ln||"").trim()); }
    while(i<lines.length){
      var ln=lines[i];
      if(isF(ln)){ out.push("\u0003"+ln.trim().replace(/\D/g,"")+"\u0003"); i++; continue; }
      var h=/^(#{1,4})\s+(.*)$/.exec(ln);
      if(h){ var lv=h[1].length; out.push("<h"+lv+">"+inlineMd(h[2])+"</h"+lv+">"); i++; continue; }
      if(/^\s*([-*_])(\s*\1){2,}\s*$/.test(ln)){ out.push("<hr/>"); i++; continue; }
      if(/^&gt;\s?/.test(ln)){ var b=[];
        while(i<lines.length && /^&gt;\s?/.test(lines[i])){ b.push(lines[i].replace(/^&gt;\s?/,"")); i++; }
        out.push("<blockquote>"+inlineMd(b.join("\n"))+"</blockquote>"); continue; }
      if(/^\s*[-*+]\s+/.test(ln)){ var ul=[];
        while(i<lines.length && /^\s*[-*+]\s+/.test(lines[i])){ ul.push(lines[i].replace(/^\s*[-*+]\s+/,"")); i++; }
        out.push("<ul>"+ul.map(function(x){ return "<li>"+inlineMd(x)+"</li>"; }).join("")+"</ul>"); continue; }
      if(/^\s*\d+[.)]\s+/.test(ln)){ var ol=[];
        while(i<lines.length && /^\s*\d+[.)]\s+/.test(lines[i])){ ol.push(lines[i].replace(/^\s*\d+[.)]\s+/,"")); i++; }
        out.push("<ol>"+ol.map(function(x){ return "<li>"+inlineMd(x)+"</li>"; }).join("")+"</ol>"); continue; }
      if(ln.indexOf("|")>=0 && i+1<lines.length && /^\s*\|?[\s:|-]+\|\s*$/.test(lines[i+1])){
        var hd=ln.trim().replace(/^\||\|$/g,"").split("|").map(function(c){ return c.trim(); });
        i+=2; var rows=[];
        while(i<lines.length && lines[i].indexOf("|")>=0 && lines[i].trim()){
          rows.push(lines[i].trim().replace(/^\||\|$/g,"").split("|").map(function(c){ return c.trim(); })); i++; }
        out.push("<table><thead><tr>"+hd.map(function(c){ return "<th>"+inlineMd(c)+"</th>"; }).join("")+
          "</tr></thead><tbody>"+rows.map(function(r){
            return "<tr>"+r.map(function(c){ return "<td>"+inlineMd(c)+"</td>"; }).join("")+"</tr>"; }).join("")+
          "</tbody></table>"); continue; }
      if(!ln.trim()){ i++; continue; }
      var para=[];
      while(i<lines.length && lines[i].trim() && !isF(lines[i]) && !/^(#{1,4})\s/.test(lines[i]) &&
            !/^\s*[-*+]\s/.test(lines[i]) && !/^\s*\d+[.)]\s/.test(lines[i])){ para.push(lines[i]); i++; }
      if(para.length) out.push("<p>"+inlineMd(para.join("\n"))+"</p>");
    }
    return out.join("").replace(/\u0003(\d+)\u0003/g, function(_,n){ return codeCard(fences[+n].lang,fences[+n].code); });
  }

function fold(key, cls, kind, title, state, detail, file, time, extra){
    var explicit = Object.prototype.hasOwnProperty.call(A.open, key);
    var open = explicit ? !!A.open[key] : (state === "run");
    A.keys.push(key);
    var stCls = state==="done" ? "ok" : (state==="run" ? "run" : "");
    var stTx  = state==="done" ? "✓ 完成" : (state==="run" ? "⟳ 进行中" : "");
    var body  = detail ? '<div class="fb'+(detail.indexOf("\n")>0?"":" dim")+'">'+esc(detail)+'</div>' : "";
    if(extra) body += '<div class="fb">'+foldExtra(extra)+'</div>';
    return '<div class="fold '+cls+(open?" open":"")+(file&&file===A.selFile?" sel":"")+'"'+
      (file?' data-file="'+esc(file)+'"':'')+'>' +
      '<div class="fh" data-toggle="'+key+'"><span class="caret">▶</span><span class="kind">'+kind+'</span>' +
      '<span class="ttl'+(file?" mono":"")+'">'+esc(title)+'</span>' +
      (stTx?'<span class="st '+stCls+'">'+stTx+'</span>':'') + '</div>' + body + '</div>' +
      (time?'<div class="tstamp">'+esc(time)+'</div>':'');
  }

function foldExtra(x){
    var h="";
    if(x.rows&&x.rows.length) h+='<dl class="kv">'+x.rows.map(function(r){
      return '<dt>'+esc(r[0])+'</dt><dd>'+esc(r[1])+'</dd>'; }).join("")+'</dl>';
    if(x.output) h+='<div class="out">'+esc(x.output)+'</div>';
    if(x.preview&&x.preview.length) h+='<div class="prev">'+x.preview.map(function(l){
      var c=l.t==="add"?"add":(l.t==="del"?"del":"ctx");
      var m=l.t==="add"?"+ ":(l.t==="del"?"- ":"  ");
      return '<div class="'+c+'">'+m+hl(l.x)+'</div>'; }).join("")+'</div>';
    h+='<button class="linkbtn" data-diag="tool">⤷ 查看完整参数与输出（ACP 原文·诊断）</button>';
    return h;
  }

function userMsg(text,time){
    return '<div class="msg-user" data-umsg="1"><div class="who">你</div>'+mdInlinePlain(text)+'</div>' +
      (time?'<div class="tstamp">'+esc(time)+'</div>':'');
  }

function agentMsg(text,time){
    return '<div class="msg-agent"><div class="who">Agent · '+esc(cur().agent)+'</div>' +
      '<div class="bubble md">'+md(text)+'</div></div>' +
      (time?'<div class="tstamp">'+esc(time)+'</div>':'');
  }

function permCard(s){
    var st = A.permSet[s.id] || (s.state === "waiting_permission" ? "pending" : "remote");
    if(st === "pending"){
      return '<div class="perm"><div class="ptitle">🔐 需要你的授权</div>' +
        '<div>运行 npm 命令，改动测试快照文件</div>' +
        '<div class="pdesc">npm test -- --watch\n作用域：src/** 与 tests/**</div>' +
        '<div class="opts">' +
        '<button class="btn p-once" data-perm="allow-once" data-sess="'+s.id+'">允许一次</button>' +
        '<button class="btn p-always" data-perm="allow-always" data-sess="'+s.id+'">始终允许</button>' +
        '<button class="btn p-reject" data-perm="reject" data-sess="'+s.id+'">拒绝</button></div></div>' +
        '<div class="tstamp">14:31:52</div>';
    }
    if(st === "remote")
      return fold("perm.r"+s.id,"perm","🔐","已被这台电脑处理（已拒绝）","done",
        "resolution: reject\nresolvedByDeviceId: —（不是这台设备）\n先一步应答，本设备不再重复提交；"+
        "客户端会收到 interaction.already_resolved。",null,"14:29");
    var label = st==="allow-once" ? "已允许一次" : (st==="allow-always" ? "已始终允许" : "已拒绝");
    var detail = st==="reject" ? "optionId: reject\n提交于 14:31:52 · 已收到 command.completed"
      : "optionId: "+st+"\n作用域：src/** 与 tests/**\n提交于 14:31:52 · 已收到 command.completed";
    return fold("perm.r"+s.id,"perm","🔐",label+" · 由这台设备","done",detail,null,"14:31");
  }

function degCard(kind, title, detail, dxKey, btnText){
    return '<div class="deg'+(kind==="warn"?" warn":"")+'"><div class="dh"><span>⚠</span><span>'+esc(title)+'</span></div>' +
      '<p class="det">'+detail+'</p>' +
      '<div class="db"><button class="linkbtn" data-dx="'+dxKey+'">'+
        esc(btnText||"查看原文")+' ›</button></div></div>';
  }

function connInfo(){ return CONN[A.conn] || CONN.online; }

function hostPanelHtml(){
    var c = connInfo(), h = "";
    if(c.block){
      return '<div class="hblock"><div class="t">'+esc(c.block[0])+'</div><div class="d">'+esc(c.block[1])+'</div>' +
        '<div style="margin-top:12px"><button class="btn primary" data-hostgo="'+esc(c.block[3]||"")+'">' +
        esc(c.block[2])+'</button></div></div>' +
        '<div class="hsec"><h5>主机</h5>' +
        '<div class="hrow"><span class="k">地址</span><span class="v">'+esc(HOSTINFO.address)+'</span></div>' +
        '<div class="hrow"><span class="k">指纹</span><span class="v">'+esc(HOSTINFO.fingerprint.slice(0,24))+'…</span></div>' +
        '</div>';
    }
    var extra = "";
    if(A.conn==="reconnecting") extra='<button class="btn" data-hostact="retry">立即重连</button>';
    if(A.conn==="offline")     extra='<button class="btn" data-hostact="retry">立即重连</button>';
    h += '<div class="hsec"><div class="hstate"><span class="dot '+c.tone+'"></span>' +
      (A.conn==="syncing" ? "在线 · 正在追平" : (A.conn==="reconnecting" ? "正在重连（第 3 次，约 8 秒后重试）" : "在线"))+
      '</div><div class="hmeta">上次同步 '+esc(HOSTINFO.lastSync)+'</div></div>';
    h += '<div class="hsec"><h5>这台设备</h5>' +
      '<div class="hrow"><span class="k">名称</span><span class="v" style="font-family:var(--sans)">' +
        esc(HOSTINFO.deviceName)+' · '+esc(HOSTINFO.clientKind)+'</span></div>' +
      '<div class="hrow"><span class="k">设备 ID</span><span class="v">'+esc(HOSTINFO.deviceId)+'</span></div>' +
      '<div class="hrow"><span class="k">权限</span><span class="v" style="font-family:var(--sans)">' +
        '<div class="hscope">'+SCOPES.filter(function(s){return s.on;}).map(function(s){
          return '<span class="tag">'+esc(s.label)+'</span>'; }).join("")+'</div></span></div></div>';
    h += '<div class="hsec"><h5>主机身份</h5>' +
      '<div class="hrow"><span class="k">地址</span><span class="v">'+esc(HOSTINFO.address)+'</span></div>' +
      '<div class="hrow"><span class="k">节点 ID</span><span class="v">'+esc(HOSTINFO.hostId)+'</span></div>' +
      '<div class="hrow"><span class="k">指纹</span><span class="v">'+esc(HOSTINFO.fingerprint)+'</span></div>' +
      '<p class="hnote">指纹 = SHA-256(主机公钥)，与电脑终端显示的一致。绑定后这台浏览器只认这个地址；' +
      '换地址必须重新配对。</p></div>';
    h += '<div class="hsec"><h5>本版本边界</h5><p class="hnote">仅支持直连一台电脑（Owner Node），' +
      '不展示经其他节点转发的会话；网页端不能新增、修改或撤销权限，那些操作只能在电脑上完成。</p></div>';
    h += '<div class="hactions">' + extra +
      '<button class="btn" data-hostact="repair">重新配对</button>' +
      '<button class="btn" data-hostact="clear">清除这台设备的数据</button></div>';
    if(A.clearArmed){
      h += '<div class="hconfirm"><div><b>确认清除？</b>会删除本浏览器的设备身份密钥、IndexedDB 里的' +
        '同步元数据与草稿。清除后下次使用必须重新配对。<br><b>电脑端仍会保留这台设备的记录</b>，' +
        '需要时在电脑上撤销。</div><div class="row">' +
        '<button class="btn stop" data-hostact="clear-yes">确认清除</button>' +
        '<button class="btn" data-hostact="clear-no">取消</button></div></div>';
    }
    return h;
  }
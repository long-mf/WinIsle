//! 通过 CDP（Chrome DevTools Protocol）直接从「汽水音乐」渲染进程的 Vue 组件
//! `LyricsView.props.lyric` 里读取当前歌曲的官方歌词（含逐字时间），
//! 不需要汽水显示歌词页、也不联网。
//!
//! 前提：汽水 app.asar 已打补丁，启动时带 `--remote-debugging-port=9222`（见 README 记载）。

use crate::lyrics::{LrcLine, Seg};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// 从本机 9222 端口找到主页面 target 的 WebSocket 调试地址。
fn page_ws_url() -> Option<String> {
    let body = ureq::get("http://127.0.0.1:9222/json/list")
        .timeout(Duration::from_millis(800))
        .call()
        .ok()?
        .into_string()
        .ok()?;
    let v: serde_json::Value = serde_json::from_str(&body).ok()?;
    let arr = v.as_array()?;
    for t in arr {
        if t.get("type").and_then(|x| x.as_str()) != Some("page") {
            continue;
        }
        let url = t.get("url").and_then(|x| x.as_str()).unwrap_or("");
        if url.contains("main.asar/main.html") {
            if let Some(ws) = t.get("webSocketDebuggerUrl").and_then(|x| x.as_str()) {
                return Some(ws.to_string());
            }
        }
    }
    None
}

fn parse_ws_url(u: &str) -> Option<(String, String)> {
    // ws://127.0.0.1:9222/devtools/page/XXXX
    let rest = u.strip_prefix("ws://")?;
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    Some((hostport.to_string(), path.to_string()))
}

fn ws_key() -> String {
    // 固定的 16 字节做 base64（RFC6455 只要求可打印随机串，不要求真的随机）
    const B: [u8; 16] = [
        0x4c, 0x69, 0x6e, 0x67, 0x44, 0x61, 0x6f, 0x57, 0x53, 0x4b, 0x65, 0x79, 0x31, 0x32, 0x33,
        0x21,
    ];
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i < B.len() {
        let b0 = B[i] as u32;
        let b1 = if i + 1 < B.len() { B[i + 1] as u32 } else { 0 };
        let b2 = if i + 2 < B.len() { B[i + 2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if i + 1 < B.len() {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if i + 2 < B.len() {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
        i += 3;
    }
    out
}

fn ws_connect(u: &str) -> Option<TcpStream> {
    let (hostport, path) = parse_ws_url(u)?;
    let mut s = TcpStream::connect(&hostport).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(2500))).ok()?;
    s.set_write_timeout(Some(Duration::from_millis(2500))).ok()?;
    // 固定的 base64 随机 key（RFC6455 只要求是 16 字节的 base64）
    let key = ws_key();
    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {}\r\nSec-WebSocket-Version: 13\r\n\r\n",
        path, hostport, key
    );
    s.write_all(req.as_bytes()).ok()?;
    // 读到头部结束
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    loop {
        let n = s.read(&mut tmp).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if buf.len() > 16384 {
            return None;
        }
    }
    if !buf.starts_with(b"HTTP/1.1 101") {
        return None;
    }
    Some(s)
}

fn ws_send_text(s: &mut TcpStream, text: &str) -> Option<()> {
    let payload = text.as_bytes();
    let len = payload.len();
    let mut frame = Vec::with_capacity(len + 14);
    frame.push(0x81u8); // FIN + text
    if len < 126 {
        frame.push(0x80 | (len as u8)); // masked
    } else if len <= 65535 {
        frame.push(0x80 | 126);
        frame.push((len >> 8) as u8);
        frame.push((len & 0xff) as u8);
    } else {
        frame.push(0x80 | 127);
        for i in (0..8).rev() {
            frame.push(((len as u64) >> (8 * i)) as u8);
        }
    }
    let mask = [0x12u8, 0x34, 0x56, 0x78];
    frame.extend_from_slice(&mask);
    for (i, b) in payload.iter().enumerate() {
        frame.push(b ^ mask[i % 4]);
    }
    s.write_all(&frame).ok()?;
    s.flush().ok()?;
    Some(())
}

/// 读取一个完整的文本帧（忽略 ping/pong），返回 JSON 文本。
fn ws_read_text(s: &mut TcpStream) -> Option<String> {
    for _ in 0..16 {
        let mut h = [0u8; 2];
        s.read_exact(&mut h).ok()?;
        let opcode = h[0] & 0x0f;
        let masked = h[1] & 0x80 != 0;
        let mut len = (h[1] & 0x7f) as usize;
        if len == 126 {
            let mut e = [0u8; 2];
            s.read_exact(&mut e).ok()?;
            len = ((e[0] as usize) << 8) | e[1] as usize;
        } else if len == 127 {
            let mut e = [0u8; 8];
            s.read_exact(&mut e).ok()?;
            let mut l = 0usize;
            for b in e {
                l = (l << 8) | b as usize;
            }
            len = l;
        }
        let mut mask = [0u8; 4];
        if masked {
            s.read_exact(&mut mask).ok()?;
        }
        let mut data = vec![0u8; len];
        s.read_exact(&mut data).ok()?;
        if masked {
            for i in 0..len {
                data[i] ^= mask[i % 4];
            }
        }
        match opcode {
            0x1 => {
                return String::from_utf8(data).ok();
            }
            0x9 => {
                // ping → 回 pong（简单忽略）
                continue;
            }
            0x8 => return None,
            _ => continue,
        }
    }
    None
}

/// 当前歌词视图对应的歌曲信息与原始歌词内容。
pub struct SodaLyric {
    pub name: String,
    pub artist: String,
    pub duration: f64, // 秒
    pub content: String,
    pub cn: String,
    pub cover: String,
}

fn evaluate(expr: &str) -> Option<serde_json::Value> {
    let wsurl = page_ws_url()?;
    let mut s = ws_connect(&wsurl)?;
    let msg = serde_json::json!({
        "id": 1,
        "method": "Runtime.evaluate",
        "params": { "expression": expr, "returnByValue": true, "awaitPromise": true }
    });
    ws_send_text(&mut s, &msg.to_string())?;
    // 读到 id=1 的响应
    for _ in 0..32 {
        let txt = ws_read_text(&mut s)?;
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
            if v.get("id").and_then(|x| x.as_i64()) == Some(1) {
                return Some(v);
            }
        }
    }
    None
}

/// 汽水真实播放时钟：直接读取页面里游离的 <video>/<audio> 元素的 currentTime。
/// DOM 进度条（.slider-fill）约 1 秒才跳一次且滞后 0.75–1.7 秒，用它驱动逐字歌词必然慢半拍；
/// 媒体元素的 currentTime 是播放器内核的真实时钟，连续、精确。
#[derive(Clone, Copy, Debug)]
pub struct SodaMediaClock {
    pub pos: f64,
    pub dur: f64,
    pub paused: bool,
    pub ready: u32,
}

fn cdp_send_wait(
    s: &mut TcpStream,
    id: i64,
    method: &str,
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let msg = serde_json::json!({ "id": id, "method": method, "params": params });
    ws_send_text(s, &msg.to_string())?;
    // Runtime.enable 会把页面已缓存的 consoleAPICalled 事件全部重放（实测可达 1000+ 条，
    // 每条都是无 id 的事件帧），因此这里必须能跳过大量事件，不能只等几帧。
    for _ in 0..8192 {
        let txt = ws_read_text(s)?;
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
            if v.get("id").and_then(|x| x.as_i64()) == Some(id) {
                return Some(v);
            }
        }
    }
    None
}

const MEDIA_PICK_FN: &str = r#"function(hintPos,hintDur){
  try{
    const arr=this;
    if(!arr||typeof arr.length!=='number') return {found:false};
    try{ window.__winislePool=arr; }catch(e){}
    try{
      if(window.__winisleTrack){
        for(let i=0;i<arr.length;i++) window.__winisleTrack(arr[i]);
      }
    }catch(e){}
    const playing=[]; const playingAny=[]; const paused=[];
    const hp=isFinite(hintPos)?+hintPos:0, hd=isFinite(hintDur)?+hintDur:0;
    for(let i=0;i<arr.length;i++){
      try{
        const a=arr[i];
        if(!a) continue;
        const d=+a.duration, ct=+a.currentTime, rs=+a.readyState;
        if(!isFinite(d)||d<=0.05||!isFinite(ct)||ct<0) continue;
        const rec={el:a,ct:ct,d:d,paused:!!a.paused,ready:rs};
        const durOk = !(hd>1 && Math.abs(d-hd)>3);
        if(!a.paused){ playingAny.push(rec); if(durOk) playing.push(rec); }
        else if(ct < d-1.5){ if(durOk) paused.push(rec); }
      }catch(e){}
    }
    let best=null;
    if(playing.length){
      playing.sort((x,y)=>y.ct-x.ct);
      best=playing[0];
    }else if(playingAny.length && hd<=1){
      // 首次还不知道时长时，至少优先当前真实在播的元素；一旦有时长提示，
      // 就只认可时长匹配的元素，避免选中旁边预加载的短视频/旧曲目。
      playingAny.sort((x,y)=>y.ct-x.ct);
      best=playingAny[0];
    }else if(paused.length){
      paused.sort((x,y)=>{
        const dx = hd>1 ? Math.abs(x.d-hd)-Math.abs(y.d-hd) : 0;
        if(Math.abs(dx)>0.5) return dx;
        return Math.abs(x.ct-hp)-Math.abs(y.ct-hp);
      });
      best=paused[0];
    }
    if(!best) return {found:false};
    try{ window.__winisleMedia=best.el; }catch(e){}
    return {found:true,pos:best.ct,dur:best.d,paused:best.paused,ready:best.ready};
  }catch(e){ return {found:false,err:String(e)}; }
}"#;

fn media_value_to_clock(v: &serde_json::Value) -> Option<SodaMediaClock> {
    if v.get("found").and_then(|x| x.as_bool()) != Some(true) {
        return None;
    }
    let pos = v.get("pos").and_then(|x| x.as_f64())?;
    let dur = v.get("dur").and_then(|x| x.as_f64())?;
    if !pos.is_finite() || !dur.is_finite() || dur <= 0.05 {
        return None;
    }
    Some(SodaMediaClock {
        pos,
        dur,
        paused: v.get("paused").and_then(|x| x.as_bool()).unwrap_or(false),
        ready: v.get("ready").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
    })
}

/// 读取汽水播放器的真实播放时钟。`hint_pos`/`hint_dur` 用于在暂停时从历史媒体元素中
/// 挑出当前曲目（而不是已播完的旧元素）。失败返回 None，由调用方回退到 DOM 进度。
pub fn fetch_media_clock(hint_pos: f64, hint_dur: f64) -> Option<SodaMediaClock> {
    // 快路径：页面里缓存过当前媒体元素（window.__winisleMedia），直接读属性。
    // 只有首次或曲目切换（时长/暂停位置明显不符）时才走下面的 queryObjects 全量扫描。
    let cached_expr = format!(
        r#"(()=>{{
          // 一次性挂 play/load 钩子：汽水切歌或单曲循环时若新建媒体元素，
          // 这里能在它一开始播放时就记住它，避免回退到 6.6 秒的 queryObjects 全量扫描。
          try{{
            if(!window.__winisleMediaHook){{
              window.__winisleMediaHook=1; window.__winisleRecent=[];
              const P=HTMLMediaElement.prototype, op=P.play, ol=P.load;
              const remember=function(el){{try{{
                const r=window.__winisleRecent;
                if(r.indexOf(el)<0){{r.push(el); if(r.length>8)r.shift();}}
                window.__winisleMedia=el;
                if(window.__winisleTrack)window.__winisleTrack(el);
              }}catch(e){{}}}};
              const track=function(el){{try{{
                if(!el||el.__winisleTracked)return; el.__winisleTracked=1;
                const mark=function(){{try{{if(!el.paused&&!el.ended)window.__winisleActive=el;}}catch(e){{}}}};
                el.addEventListener('timeupdate',mark);
                el.addEventListener('playing',mark);
                el.addEventListener('seeking',mark);
                el.addEventListener('seeked',mark);
              }}catch(e){{}}}};
              window.__winisleTrack=track;
              P.play=function(){{remember(this);return op.apply(this,arguments)}};
              P.load=function(){{remember(this);return ol.apply(this,arguments)}};
            }}
          }}catch(e){{}}
          const hd={hd}, hp={hp};
          try{{
            // 事件追踪到的“最后一次真正播放中”的元素最可靠：timeupdate/playing/
            // seeking/seeked 只在真实的播放元素上触发，预加载元素不会误报到。
            try{{
              const act=window.__winisleActive;
              if(act){{
                const d=+act.duration, ct=+act.currentTime, rs=+act.readyState;
                if(isFinite(d)&&d>0.05&&isFinite(ct)&&ct>=0
                   && !(hd>1&&Math.abs(d-hd)>3) && !act.ended){{
                  return {{found:true,pos:ct,dur:d,paused:!!act.paused,ready:rs}};
                }}
              }}
            }}catch(e){{}}
            const cand=[];
            const add=function(a){{if(a&&cand.indexOf(a)<0)cand.push(a)}};
            add(window.__winisleMedia);
            const rec=window.__winisleRecent||[];
            for(let i=0;i<rec.length;i++)add(rec[i]);
            const pool=window.__winislePool||[];
            for(let i=0;i<pool.length;i++)add(pool[i]);
            const now=Date.now();
            let best=null, bestScore=-1e18, fallback=null, fbScore=-1e18;
            for(let i=0;i<cand.length;i++){{
              try{{
                const a=cand[i], d=+a.duration, ct=+a.currentTime, rs=+a.readyState;
                if(!isFinite(d)||d<=0.05||!isFinite(ct)||ct<0) continue;
                if(hd>1&&Math.abs(d-hd)>3) continue;
                // 停在结尾的旧元素永远排除：单曲循环/切歌后它会被误当成当前曲目。
                if(a.paused&&ct>=d-2) continue;
                // 汽水会预加载一批媒体元素，它们的 paused 可能一直是 false 但 currentTime
                // 冻结不动（实测多个元素卡在 12s）。只有两次轮询间 currentTime 真的变化
                // 的元素才是当前播放器；单曲循环/拖动时 currentTime 会跳变，也要认。
                const pc=a.__winisleCt, pt=a.__winisleT;
                const delta=isFinite(pc)?ct-pc:0;
                a.__winisleCt=ct; a.__winisleT=now;
                const moved=isFinite(pc)&&Math.abs(delta)>0.005;
                if(moved){{
                  // 用 |delta| 加权：单曲循环/拖动时当前元素会跳几十秒，远大于
                  // 旁边那些预加载元素每秒不到 1 秒的微小漂移，避免循环瞬间
                  // 被别的元素抢先报一个中间位置（实测会闪一下 11.4s）。
                  const score=2e6+Math.abs(delta)*100.0+ct;
                  if(score>bestScore){{bestScore=score;best={{el:a,pos:ct,dur:d,paused:!!a.paused,ready:rs}};}}
                }}
                // 没有可判定的前进元素时的兜底：优先真正暂停且离提示位置最近的元素，
                // 再退而求其次选离提示位置最近的元素（避免选中远处冻结的预加载元素）。
                const fs=(a.paused?0:-100)-Math.abs(ct-hp);
                if(fs>fbScore){{fbScore=fs;fallback={{el:a,pos:ct,dur:d,paused:!!a.paused,ready:rs}};}}
              }}catch(e){{}}
            }}
            if(!best){{
              // 没有已知时长、也没有任何元素在真正推进时，不要随便拿一个
              // 预加载元素充数；回退到全量扫描，避免把暂停的旧元素锁死。
              if(hd<=1) return {{found:false}};
              best=fallback;
            }}
            if(!best) return {{found:false}};
            try{{window.__winisleMedia=best.el;}}catch(e){{}}
            return {{found:true,pos:best.pos,dur:best.dur,paused:best.paused,ready:best.ready}};
          }}catch(e){{return {{found:false}};}}
        }})()"#,
        hd = hint_dur,
        hp = hint_pos
    );
    if let Some(resp) = evaluate(&cached_expr) {
        if let Some(val) = resp
            .get("result")
            .and_then(|x| x.get("result"))
            .and_then(|x| x.get("value"))
        {
            if let Some(mc) = media_value_to_clock(val) {
                if std::env::var_os("WINISLE_MEDIA_LOG").is_some() {
                    dlog(&format!(
                        "media cached pos={:.3} dur={:.3} paused={} ready={}",
                        mc.pos, mc.dur, mc.paused, mc.ready
                    ));
                }
                return Some(mc);
            }
        }
    }

    let wsurl = page_ws_url()?;
    let mut s = ws_connect(&wsurl)?;
    // queryObjects 在对象多时需要数秒（实测 ~6.6s）才能返回，默认 2.5s 读超时会误判失败。
    // 汽水进程里同时存在几百个 HTMLMediaElement（实测 501 个），Runtime.queryObjects 单次约 5 秒；
    // 若当前没有正在推进的元素、每轮都重扫，会把渲染线程和同进程的其它 CDP 轮询一起饿死。
    // 给"全量扫描"加冷却：扫过一轮后 6 秒内不再重扫，直接回退 DOM 进度（DOM 进度本身可用）。
    static LAST_FULL: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
    if let Ok(g) = LAST_FULL.lock() {
        if let Some(t) = *g {
            if t.elapsed() < std::time::Duration::from_secs(6) {
                return None;
            }
        }
    }
    if let Ok(mut g) = LAST_FULL.lock() {
        *g = Some(std::time::Instant::now());
    }
    let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(15000)));
    let _ = s.set_write_timeout(Some(std::time::Duration::from_millis(5000)));
    // 先清空页面缓存的 console 条目，再 enable；否则 Runtime.enable 会重放上千条
    // consoleAPICalled 事件，既拖慢查询又会把响应埋进事件流里。
    let _ = cdp_send_wait(&mut s, 99, "Runtime.discardConsoleEntries", serde_json::json!({}));
    let _ = cdp_send_wait(&mut s, 1, "Runtime.enable", serde_json::json!({}));
    let proto = cdp_send_wait(
        &mut s,
        2,
        "Runtime.evaluate",
        serde_json::json!({
            "expression": "HTMLMediaElement.prototype",
            "silent": true,
            "returnByValue": false
        }),
    )?;
    let proto_id = proto
        .get("result")?
        .get("result")?
        .get("objectId")?
        .as_str()?
        .to_string();
    let objs = cdp_send_wait(
        &mut s,
        3,
        "Runtime.queryObjects",
        serde_json::json!({
            "prototypeObjectId": proto_id
        }),
    )?;
    let arr_id = objs
        .get("result")?
        .get("objects")?
        .get("objectId")?
        .as_str()?
        .to_string();
    let call = cdp_send_wait(
        &mut s,
        4,
        "Runtime.callFunctionOn",
        serde_json::json!({
            "objectId": arr_id,
            "functionDeclaration": MEDIA_PICK_FN,
            "returnByValue": true,
            "arguments": [ { "value": hint_pos }, { "value": hint_dur } ]
        }),
    )?;
    let v = call.get("result")?.get("result")?.get("value")?;
    let mc = media_value_to_clock(v);
    match mc {
        Some(m) => {
            if std::env::var_os("WINISLE_MEDIA_LOG").is_some() {
                dlog(&format!(
                    "media pos={:.3} dur={:.3} paused={} ready={} hint_pos={:.2}",
                    m.pos, m.dur, m.paused, m.ready, hint_pos
                ));
            }
            Some(m)
        }
        None => {
            if std::env::var_os("WINISLE_MEDIA_LOG").is_some() {
                dlog(&format!("media none hint_pos={:.2} hint_dur={:.2}", hint_pos, hint_dur));
            }
            None
        }
    }
}

const EXPR: &str = r#"(()=>{
 const el=document.querySelector('#app');
 const app=el&&el.__vue_app__;
 if(!app) return {ok:false,why:'noapp',vis:document.visibilityState,focus:document.hasFocus()};
 // 只有播放页挂载 LyricsView。歌单/场景页的 Vue 树巨大，深遍历会拖住渲染线程并把进度轮询饿死，
 // 因此非播放页直接返回，绝不在歌单页做 600 层 vnode 行走。
 const rh=(location.hash||''); if(!(rh==='#/player'||rh.startsWith('#/player?'))) return {ok:false,why:'notplayer',vis:document.visibilityState,focus:document.hasFocus()};
 const root=app._container&&app._container._vnode;
 const seen=new Set(); const list=[];
 (function walk(v,d){
   if(!v||d>600||seen.has(v))return; seen.add(v);
   const c=v.component;
   if(c){ const n=(c.type&&(c.type.__name||c.type.name))||'';
     if(n==='LyricsView'&&c.props&&c.subTree&&c.subTree.el&&c.subTree.el.isConnected){
       const lyr=c.props.lyric||{}, tr=c.props.track||{};
       const arts=(tr.artists||[]).map(a=>a&&a.name).filter(Boolean).join(', ');
       const uc=(tr.album&&tr.album.url_cover)||{};
       let cover='';
       if(uc&&uc.uri){ const base=(uc.urls&&uc.urls[0])||'https://p3-luna.douyinpic.com/img/'; const tpl=uc.template_prefix||'tplv-b829550vbb'; cover=base+uc.uri+'~'+tpl+'-crop-center:400:400.jpg'; }
       let noLyric='';
       try{ const ne=c.subTree.el.querySelector('.no-lyric .text'); if(ne) noLyric=(ne.textContent||'').trim(); }catch(e){}
       list.push({name:tr.name||'',artist:arts,duration:(tr.duration||0)/1000,content:lyr.content||'',cn:(lyr.translations&&lyr.translations.cn)||'',cover:cover,noLyric:noLyric});
     }
     walk(c.subTree,d+1);
   }
   const ch=v.children;
   if(Array.isArray(ch))ch.forEach(x=>walk(x,d+1)); else if(ch&&typeof ch==='object')walk(ch,d+1);
   if(v.suspense)walk(v.suspense.activeBranch,d+1);
 })(root,0);
 // DOM 兜底：汽水把整个播放页栈（当前曲目 + 后续几首）都挂在 DOM 里，每个 .player-box
 // 都带歌名与「纯音乐，请欣赏 / 暂无歌词，请欣赏」占位。用于 LyricsView 未挂载或内容为空时按歌名核对。
 const pages=[];
 try{
   document.querySelectorAll('.player-box').forEach(function(pb){
     const te=pb.querySelector('.info .track .audio-info .title');
     let nm=te?((te.innerText||'').split('\n')[0]||'').trim():'';
     const ne=pb.querySelector('.lyrics-container .no-lyric .text');
     const nl=ne?(ne.textContent||'').trim():'';
     if(nm||nl) pages.push({name:nm,noLyric:nl});
   });
 }catch(e){}
 return {ok:true,vis:document.visibilityState,focus:document.hasFocus(),list:list,pages:pages};
})()"#;

fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// 读取当前在播歌曲的官方歌词。`title`/`artist` 用于防止切歌瞬间读到上一首。
pub fn fetch_current(title: &str, artist: &str) -> Option<SodaLyric> {
    match fetch_current_inner(title, artist, true) {
        SodaProbe::Got(s) => Some(s),
        _ => None,
    }
}

/// 同 `fetch_current`，但绝不切换汽水内部路由（用于后台轮询“正在播放”，避免干扰用户浏览页面）。
pub fn fetch_current_no_nav(title: &str, artist: &str) -> Option<SodaLyric> {
    match fetch_current_inner(title, artist, false) {
        SodaProbe::Got(s) => Some(s),
        _ => None,
    }
}

/// 三态探测（不导航）：供“拿不到歌词时是否有必要继续轮询”的判定使用。
/// `NoLyric` 说明汽水已明确给出“暂无歌词”，可以停止轮询；`Unknown` 才值得再试。
pub fn probe_current_no_nav(title: &str, artist: &str) -> SodaProbe {
    fetch_current_inner(title, artist, false)
}

/// 从底部播放条 `.bottom-player` 读取当前曲目（歌名/歌手/时长）。
/// 该播放条在「正在播放页」「歌单/场景模式」等几乎所有页面都常驻，因此即使
/// LyricsView 未挂载（用户切到歌单/场景模式）也能识别到当前歌曲，不依赖 SMTC。
pub fn fetch_bottom_player() -> Option<SodaLyric> {
    let expr = r#"(()=>{
      try{
        const bp=document.querySelector('.bottom-player');
        if(!bp) return {ok:false};
        const t=bp.querySelector('.track .title');
        const a=bp.querySelector('.track .artist-link');
        const name=t?(t.textContent||'').trim():'';
        if(!name) return {ok:false};
        const tip=bp.querySelector('.indicator')||bp.querySelector('.slider-tip');
        let dur=0;
        if(tip){const m=(tip.textContent||'').match(/(\d+):(\d+)\s*\/\s*(\d+):(\d+)/); if(m){dur=(+m[3])*60+(+m[4]);}}
        const ci=bp.querySelector('.track .cover img');
        const cover=ci?(ci.src||''):'';
        return {ok:true,name:name,artist:a?(a.textContent||'').trim():'',duration:dur,cover:cover};
      }catch(e){return {ok:false};}
    })()"#;
    let v = evaluate(expr)?;
    let o = v.get("result")?.get("result")?.get("value")?;
    if o.get("ok").and_then(|x| x.as_bool()) != Some(true) {
        return None;
    }
    let name = o
        .get("name")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() {
        return None;
    }
    Some(SodaLyric {
        name,
        artist: o.get("artist").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        duration: o.get("duration").and_then(|x| x.as_f64()).unwrap_or(0.0),
        content: String::new(),
        cn: String::new(),
        cover: o.get("cover").and_then(|x| x.as_str()).unwrap_or("").to_string(),
    })
}

/// 汽水当前曲目的「完整时长 + 试听片段」元数据。
/// 汽水给非会员的 VIP 歌曲只放一段 30 秒试听，底部播放条只显示片段内时长，
/// 而官方歌词按整首歌的绝对时间编写；因此需要把片段在整首歌里的起点补回来。
#[derive(Clone, Debug, Default)]
pub struct SodaTrialMeta {
    pub track_name: String, // 元数据对应的曲名（用于避免拿到组件树里残留的旧曲目）
    pub full_duration: f64, // 整首歌时长（秒）
    pub clip_start: f64,    // 试听片段在整首歌里的起点（秒）；0=片段从开头开始
    pub clip_duration: f64, // 试听片段长度（秒）；0=没有片段信息
    pub limited_free: bool, // limited_free_info 非空（更明确的“受限试听”标记）
}

/// 读取汽水当前曲目的完整时长和试听片段元数据（`preview` / `audition_info`）。
/// 只在播放页的 LyricsView 上能拿到完整 track，其它页面返回 None；调用方可沿用上一份值。
pub fn fetch_trial_meta() -> Option<SodaTrialMeta> {
    let expr = r#"(()=>{
      const el=document.querySelector('#app'); const app=el&&el.__vue_app__;
      if(!app) return {ok:false};
      const root=app._container&&app._container._vnode;
      const seen=new Set(); let best=null;
      const hasClip=(x)=>!!(x&&((x.duration&&x.duration>0)||(x.duration_ms&&x.duration_ms>0)||(x.start&&x.start>0)||(x.start_time_ms&&x.start_time_ms>0)));
      (function walk(v,d){
        if(!v||d>600||seen.has(v))return; seen.add(v);
        const c=v.component;
        if(c){
          let tr=null;
          try{tr=(c.props&&c.props.track)||null;}catch(e){}
          if(!tr){try{tr=(c.props&&c.props.playable)||null;}catch(e){}}
          if(tr&&typeof tr==='object'&&typeof tr.duration==='number'&&tr.duration>0){
            if(!best||hasClip(tr.audition_info)||hasClip(tr.preview)) best=tr;
          }
          walk(c.subTree,d+1);
        }
        const ch=v.children;
        if(Array.isArray(ch))ch.forEach(x=>walk(x,d+1)); else if(ch&&typeof ch==='object')walk(ch,d+1);
        if(v.suspense)walk(v.suspense.activeBranch,d+1);
      })(root,0);
      // 底部播放条 DOM 在“试听片段”时会把滑块填充改成 [片段起点..当前位置]：
      // .slider-fill 的 left=片段起点，.slider-partial-track 的 left/width 标出可试听区间。
      const bp=document.querySelector('.bottom-player');
      let domFull=0;
      const ind=bp&&(bp.querySelector('.indicator')||bp.querySelector('.slider-tip'));
      if(ind){const m=(ind.textContent||'').match(/(\d+):(\d+)\s*\/\s*(\d+):(\d+)/); if(m){domFull=(+m[3])*60+(+m[4]);}}
      const num=(el,key)=>{if(!el)return -1;const st=el.getAttribute('style')||'';const m=st.match(new RegExp(key+':\\s*([\\d.]+)%'));return m?parseFloat(m[1]):-1;};
      const fillLeft=num(bp&&bp.querySelector('.slider-fill'),'left');
      const part=bp&&bp.querySelector('.slider-partial-track');
      const partWidth=num(part,'width');
      const active = domFull>1 && partWidth>0.1 && partWidth<99.5;
      if(!best && !active) return {ok:false};
      const tr=best||{}, pf=tr.preview||{}, ai=tr.audition_info||{};
      // 只有底部播放条真的画出“试听区间”时才按试听处理。普通完整歌曲的
      // track 对象也可能带 audition_info/preview（汽水给每首歌都塞了 30 秒
      // 试听元数据），过去误把它当成试听会让媒体时钟拿 30s 去匹配整曲元素，
      // 结果其他歌进度一直闪、暂停播放都不对。
      let start=0, clip=0;
      if(active){ start=fillLeft/100*domFull; clip=partWidth/100*domFull; }
      const full = domFull>1 ? domFull : (tr.duration||0)/1000;
      return {ok:true,name:active?'':(tr.name||''),full:full,start:start,clip:clip,limited:active||!!tr.limited_free_info};
    })()"#;
    let v = evaluate(expr)?;
    let o = v.get("result")?.get("result")?.get("value")?;
    if o.get("ok").and_then(|x| x.as_bool()) != Some(true) {
        return None;
    }
    let full = o.get("full").and_then(|x| x.as_f64()).unwrap_or(0.0);
    if full <= 1.0 {
        return None;
    }
    Some(SodaTrialMeta {
        track_name: o.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        full_duration: full,
        clip_start: o.get("start").and_then(|x| x.as_f64()).unwrap_or(0.0).max(0.0),
        clip_duration: o.get("clip").and_then(|x| x.as_f64()).unwrap_or(0.0).max(0.0),
        limited_free: o.get("limited").and_then(|x| x.as_bool()).unwrap_or(false),
    })
}

/// 只读底部播放条的「播放态 + 实时进度」，不要求有歌名（歌单/场景页底部条常无标题）。
/// 播放态用中间大按钮图标判定：暂停双竖条 `M9 7.8…` = 正在播放；播放三角 `M11 40.407…` = 已暂停。
/// 进度用 `.slider-fill` 的宽度百分比（亚秒级实时；`.slider-tip` 文本是滞后的悬停提示，不能用）。
pub fn fetch_play_state() -> Option<(bool, f64, f64)> {
    let expr = r#"(()=>{
      try{
        const bp=document.querySelector('.bottom-player');
        if(!bp) return {ok:false};
        const tip=bp.querySelector('.indicator')||bp.querySelector('.slider-tip');
        let dur=0;
        if(tip){const m=(tip.textContent||'').match(/(\d+):(\d+)\s*\/\s*(\d+):(\d+)/); if(m){dur=(+m[3])*60+(+m[4]);}}
        const fill=bp.querySelector('.slider-fill');
        let pct=-1;
        if(fill){const w=(fill.style.width||'').replace('%',''); const f=parseFloat(w); if(!isNaN(f)) pct=f;}
        if(pct<0){const dot=bp.querySelector('.slider-dot');
          if(dot){const w=(dot.style.left||'').replace('%',''); const f=parseFloat(w); if(!isNaN(f)) pct=f;}}
        const btn=bp.querySelector('.controls.center .button.large');
        // 找不到大播放键时，不能把"未知"当成"暂停"：歌单/场景页切换瞬间按钮可能不在 DOM，
        // 误判为暂停会让上层把播放态冻结、进度条与歌词定住。这里明确返回 ok:false 交给上层保持原状态。
        if(!btn) return {ok:false, why:'nobtn'};
        const p=btn.querySelector('path'); const d=p?(p.getAttribute('d')||''):''; const playing=/^M9\s*7\.8/.test(d);
        const cur = (dur>0 && pct>=0)? dur*pct/100.0 : -1;
        return {ok:true, dur:dur, cur:cur, playing:playing};
      }catch(e){return {ok:false};}
    })()"#;
    let v = evaluate(expr)?;
    let o = v.get("result")?.get("result")?.get("value")?;
    if o.get("ok").and_then(|x| x.as_bool()) != Some(true) {
        return None;
    }
    let dur = o.get("duration").and_then(|x| x.as_f64())
        .or_else(|| o.get("dur").and_then(|x| x.as_f64()))
        .unwrap_or(0.0);
    let cur = o.get("cur").and_then(|x| x.as_f64()).unwrap_or(-1.0);
    let playing = o.get("playing").and_then(|x| x.as_bool()).unwrap_or(false);
    if std::env::var_os("WINISLE_SODAPROG_LOG").is_some() {
        dlog(&format!("play_state playing={} cur={:.2} dur={:.2}", playing, cur, dur));
    }
    Some((playing, cur, dur))
}

/// CDP 探测结果三态：
/// - `Got`：拿到了可用信息（真实歌词，或“纯音乐，请欣赏”占位；`content` 已填好）。
/// - `NoLyric`：汽水明确表示“暂无歌词，请欣赏”（缺词，但**不是**纯音乐）——已可下结论，无需再轮询。
/// - `Unknown`：此刻还读不到（页面未就绪 / 未挂载 LyricsView 等）——可稍后再试。
pub enum SodaProbe {
    Got(SodaLyric),
    NoLyric,
    Unknown,
}

fn fetch_current_inner(title: &str, artist: &str, allow_nav: bool) -> SodaProbe {
    let want_t = norm(title);
    let name_match = |name: &str| {
        let got_t = norm(name);
        want_t.is_empty() || got_t == want_t || got_t.contains(&want_t) || want_t.contains(&got_t)
    };
    for attempt in 0..4 {
        let mut retry = false;
        let mut hidden = false;
        if let Some(v) = evaluate(EXPR) {
            let r = v.get("result").and_then(|x| x.get("result"));
            let val = r.and_then(|x| x.get("value"));
            if let Some(o) = val {
                hidden = o.get("vis").and_then(|x| x.as_str()) == Some("hidden");
                if o.get("ok").and_then(|x| x.as_bool()) == Some(true) {
                    let list = o.get("list").and_then(|x| x.as_array());
                    let pages = o.get("pages").and_then(|x| x.as_array());
                    let mk = |item: &serde_json::Value, content: &str| -> SodaLyric {
                        SodaLyric {
                            name: item.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                            artist: item.get("artist").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                            duration: item.get("duration").and_then(|x| x.as_f64()).unwrap_or(0.0),
                            content: content.to_string(),
                            cn: item.get("cn").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                            cover: item.get("cover").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                        }
                    };
                    // 1) Vue LyricsView：歌名匹配 + 真实歌词内容
                    if let Some(list) = list {
                        for item in list {
                            let name = item.get("name").and_then(|x| x.as_str()).unwrap_or("");
                            let content = item.get("content").and_then(|x| x.as_str()).unwrap_or("");
                            if name_match(name) && !content.trim().is_empty() {
                                return SodaProbe::Got(mk(item, content));
                            }
                        }
                    }
                    // 2) Vue LyricsView：纯音乐占位（content 空，.no-lyric 为“纯音乐”标记）
                    if let Some(list) = list {
                        for item in list {
                            let name = item.get("name").and_then(|x| x.as_str()).unwrap_or("");
                            let no_lyric = item.get("noLyric").and_then(|x| x.as_str()).unwrap_or("");
                            if name_match(name) && crate::lyrics::is_instrumental_marker(no_lyric) {
                                return SodaProbe::Got(mk(item, no_lyric));
                            }
                        }
                    }
                    // 3) Vue LyricsView：暂无歌词（缺词，非纯音乐）——已可下结论
                    if let Some(list) = list {
                        for item in list {
                            let name = item.get("name").and_then(|x| x.as_str()).unwrap_or("");
                            let no_lyric = item.get("noLyric").and_then(|x| x.as_str()).unwrap_or("");
                            if name_match(name) && crate::lyrics::is_no_lyric_marker(no_lyric) {
                                return SodaProbe::NoLyric;
                            }
                        }
                    }
                    // 4) DOM .player-box 兜底：LyricsView 未挂载时，按歌名核对 .no-lyric 文本
                    if let Some(pages) = pages {
                        for p in pages {
                            let name = p.get("name").and_then(|x| x.as_str()).unwrap_or("");
                            let no_lyric = p.get("noLyric").and_then(|x| x.as_str()).unwrap_or("");
                            if name_match(name) && crate::lyrics::is_instrumental_marker(no_lyric) {
                                return SodaProbe::Got(SodaLyric {
                                    name: name.to_string(),
                                    artist: String::new(),
                                    duration: 0.0,
                                    content: no_lyric.to_string(),
                                    cn: String::new(),
                                    cover: String::new(),
                                });
                            }
                        }
                    }
                    // 5) DOM .player-box：暂无歌词
                    if let Some(pages) = pages {
                        for p in pages {
                            let name = p.get("name").and_then(|x| x.as_str()).unwrap_or("");
                            let no_lyric = p.get("noLyric").and_then(|x| x.as_str()).unwrap_or("");
                            if name_match(name) && crate::lyrics::is_no_lyric_marker(no_lyric) {
                                return SodaProbe::NoLyric;
                            }
                        }
                    }
                    retry = true;
                } else {
                    dlog(&format!(
                        "cdp not ready: {}",
                        o.get("why").and_then(|x| x.as_str()).unwrap_or("?")
                    ));
                    retry = true;
                }
            }
        }
        // 绝不移动/隐藏/显示汽水窗口（任何窗口操作都可能让客户端异常）。
        // 仅在「汽水自己 hidden（最小化到托盘）」时把内部路由切到播放页，
        // 让 Vue 的 LyricsView 挂载；此时用户看不到窗口，不会打扰。
        // 注意：可见但未聚焦时也不要导航——用户打开岛/切到别的程序时，
        // 汽水窗口仍可能露在屏幕上，切路由会让他看到歌词页“翻一下”。
        let background = hidden;
        if retry && (allow_nav || background) && attempt == 0 {
            let _ = evaluate(NAV_PLAYER_EXPR);
            // 后台/隐藏页切到播放页后 Vue 挂载 LyricsView 在 Chromium 节流下约需 2s。
            std::thread::sleep(Duration::from_millis(2200));
        } else if retry && attempt < 3 {
            std::thread::sleep(Duration::from_millis(350));
        }
    }
    let _ = artist;
    SodaProbe::Unknown
}

const NAV_PLAYER_EXPR: &str = r#"(()=>{try{const p={enter_method:'click_player_icon_below',scene_name:'radio'};const h='#/player?et_params='+encodeURIComponent(JSON.stringify(p));const cur=location.hash||'';const onPlayer=(cur.indexOf('#/player?')===0||cur==='#/player');if(!onPlayer){location.hash=h;}return location.hash;}catch(e){return 'err';}})()"#;

/// 汽水窗口隐藏/最小化到托盘时，提前把内部路由留在播放页，确保 LyricsView 常驻挂载，
/// 这样切歌后歌词立即可读，避免每次都等 4×350ms 再回退到昂贵的全量内存扫描。
/// 只在 `document.visibilityState === 'hidden'` 时操作；窗口可见时绝不干预用户当前页面。
pub fn ensure_player_view_when_hidden() {
    const EXPR: &str = r#"(()=>{try{
      const KEY='__winislePrevHash';
      const cur=location.hash||'';
      const onPlayer=(cur.indexOf('#/player?')===0||cur==='#/player');
      if(document.visibilityState==='hidden'){
        if(!onPlayer){
          if(!window[KEY]) window[KEY]=cur;
          const p={enter_method:'click_player_icon_below',scene_name:'radio'};
          location.hash='#/player?et_params='+encodeURIComponent(JSON.stringify(p));
          return 'nav';
        }
      }else{
        // 可见时不要再把路由切回隐藏前那一页：那会在用户打开窗口的瞬间
        // 触发一次可见的页面切换动画（叠加上隐藏时的切页，就像“翻两次页”）。
        // 这里只清掉记录，页面保持当前所在页；用户手动切页也不受影响。
        if(window[KEY]) window[KEY]='';
      }
      return 'ok';
    }catch(e){return 'err';}})()"#;
    let _ = evaluate(EXPR);
}

const PROG_EXPR: &str = r#"(()=>{
  const bp=document.querySelector('.bottom-player');
  if(!bp) return null;
  const tip=bp.querySelector('.indicator')||bp.querySelector('.slider-tip');
  let pct=null;
  // 进度优先取 `.slider-fill` 的 width%：这是播放条真实已播放比例（实测随时间连续增长）。
  // `.slider-dot` 在某些 player-feed/场景页会长期停在旧值（实测 fill 68% 而 dot 36.7% 不动），不能优先使用。
  const fill=bp.querySelector('.slider-fill');
  if(fill){const w=(fill.getAttribute('style')||'').match(/width:\s*([\d.]+)%/); if(w){pct=parseFloat(w[1]);}}
  // 次选 `.slider-dot` 的 left%
  if(pct===null){const dot=bp.querySelector('.slider-dot'); if(dot){const l=(dot.getAttribute('style')||'').match(/left:\s*([\d.]+)%/); if(l){pct=parseFloat(l[1]);}}}
  // 再次退回 thumb/track 的位置比例
  if(pct===null){const th=bp.querySelector('.slider-thumb');const tr=bp.querySelector('.slider-track');
    if(th&&tr){const b=th.getBoundingClientRect();const t=tr.getBoundingClientRect();if(t.width>0){pct=(b.left+b.width*0.5-t.left)/t.width*100.0;}}}
  // 注意：`.slider-tip` 是隐藏的悬停提示，innerText 在 display:none 时为空，必须用 textContent
  // 记录进度条数值「上一次变化」的时间，返回其年龄 age（秒）。
  // 汽水 DOM 进度约 1 秒才跳一次，而读取可能发生在跳变后任意时刻；知道 age 就能
  // 把读到的旧值补偿到“当前真实进度 = pct 对应时间 + age”，避免时间轴慢半拍。
  const st=window.__winisleProgState||(window.__winisleProgState={w:-999,at:performance.now()});
  const now=performance.now();
  if(pct!==null && Math.abs(pct-st.w)>0.0005){st.w=pct;st.at=now;}
  const age=(pct!==null)?Math.max(0,(now-st.at)/1000):0;
  return {pct:pct, text: tip?tip.textContent:'', age:age};
})()"#;

fn mmss(s: &str) -> Option<f64> {
    let mut it = s.split(':');
    let m: f64 = it.next()?.trim().parse().ok()?;
    let sec: f64 = it.next()?.trim().parse().ok()?;
    Some(m * 60.0 + sec)
}

/// 从汽水播放器界面读取「当前播放位置 / 总时长」（秒）。
/// 总时长读播放条 `.indicator` 的 `当前 / 总长` 文本（真实值）；
/// 当前秒数优先用进度条填充比例 `.slider-fill` 的 width%（亚秒精度，避免整秒跳变导致
/// 逐字高亮“唱过去了又退回一两个字”），失败再退回 `.indicator` 的整秒。
/// 通过 CDP 点击汽水播放器上的按钮，实现上一曲 / 播放暂停 / 下一曲。
/// `cmd`: "prev" | "play" | "next"（"play" 即切换播放/暂停）。返回是否点击成功。
pub fn control(cmd: &str) -> bool {
    let pick = match cmd {
        "prev" => "document.querySelectorAll('.bottom-player .controls.center .button')[0]",
        "next" => "document.querySelectorAll('.bottom-player .controls.center .button')[2]",
        _ => "document.querySelector('.bottom-player .controls.center .button.large')",
    };
    let expr = format!(
        "(()=>{{try{{const b={pick};if(!b)return false;b.click();return true;}}catch(e){{return false;}}}})()"
    );
    evaluate(&expr)
        .and_then(|v| v.get("result")?.get("result")?.get("value")?.as_bool())
        .unwrap_or(false)
}

pub fn fetch_progress(_expected_dur: f64) -> Option<(f64, f64, f64)> {
    let v = match evaluate(PROG_EXPR) {
        Some(v) => v,
        None => {
            if std::env::var_os("WINISLE_SODAPROG_LOG").is_some() {
                dlog("prog eval none");
            }
            return None;
        }
    };
    let obj = v.get("result")?.get("result")?.get("value")?;
    if obj.is_null() {
        return None;
    }
    let pct = obj.get("pct").and_then(|x| x.as_f64());
    let age = obj.get("age").and_then(|x| x.as_f64()).unwrap_or(0.0).clamp(0.0, 1.5);
    let text = obj.get("text").and_then(|x| x.as_str()).unwrap_or("");
    if std::env::var_os("WINISLE_SODAPROG_LOG").is_some() {
        dlog(&format!("prog pct={:?} text={:?}", pct, text));
    }
    let toks: Vec<&str> = text.split_whitespace().collect();
    let mut pair: Option<(f64, f64)> = None;
    for w in toks.windows(3) {
        if w[1] != "/" {
            continue;
        }
        if let (Some(cur), Some(total)) = (mmss(w[0]), mmss(w[2])) {
            if total > 1.0 && total >= cur {
                pair = Some((cur, total));
            }
        }
    }
    let (cur0, total) = pair?;
    // 进度以进度条填充比例 pct 为准（亚秒精度、随播放实时推进）。
    // 说明：`.indicator` 里的“当前时间”文本经常会卡住不刷新（实测停在 02:30，
    // 而实际已播到 02:46），只有总时长可信；之前拿它做“合理性校验”会把正确的
    // 实时 pct 误判为异常、退回静止的文本秒数，导致进度永远卡在 2:30/2:31 来回跳。
    // 因此：只要 pct 有效，就一律用 pct×总时长；仅在 pct 缺失/越界时才退回文本秒数。
    let cur = match pct {
        Some(p) if p.is_finite() && p >= 0.0 && p <= 100.5 && total > 1.0 => {
            (total * p / 100.0).clamp(0.0, total)
        }
        _ => cur0,
    };
    Some((cur, total, age))
}
fn dlog(s: &str) {
    use std::io::Write as _;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(crate::paths::file("lyrics_debug.log"))
    {
        let _ = writeln!(f, "{}", s);
    }
}

/// 把汽水官方歌词文本解析成 LrcLine：
///   `[start_ms,dur_ms]<off,dur,?>chars...`（逐字）或 `[mm:ss.xx]text`（整行）。
pub fn parse_content(content: &str) -> Vec<LrcLine> {
    let mut out = Vec::new();
    for raw in content.split('\n') {
        let line = raw.trim_end_matches('\r').trim();
        if line.is_empty() {
            continue;
        }
        if !line.starts_with('[') {
            continue;
        }
        let close = match line.find(']') {
            Some(i) => i,
            None => continue,
        };
        let head = &line[1..close];
        let body = &line[close + 1..];
        // 整行 LRC：[mm:ss.xx]
        if head.contains(':') {
            let parts: Vec<&str> = head.split(':').collect();
            if parts.len() != 2 {
                continue;
            }
            let m: f64 = parts[0].trim().parse().unwrap_or(0.0);
            let sec: f64 = parts[1].trim().parse().unwrap_or(0.0);
            let text = body.trim().to_string();
            if !text.is_empty() {
                out.push(LrcLine { t: m * 60.0 + sec, text, segs: vec![] });
            }
            continue;
        }
        // 逐字：[start,dur] 后面跟 <off,dur,?>文字
        let hp: Vec<&str> = head.split(',').collect();
        if hp.len() < 1 {
            continue;
        }
        let start_ms: f64 = hp[0].trim().parse().unwrap_or(-1.0);
        if start_ms < 0.0 {
            continue;
        }
        let line_t = start_ms / 1000.0;
        let mut text = String::new();
        let mut segs: Vec<Seg> = Vec::new();
        let mut cur_off: Option<f64> = None;
        let mut cur_dur: f64 = 0.0;
        let mut cur_chars: u32 = 0;
        let mut i = 0;
        let b: Vec<char> = body.chars().collect();
        while i < b.len() {
            if b[i] == '<' {
                // 结算上一段
                if let Some(off) = cur_off {
                    if cur_chars > 0 {
                        segs.push(Seg { t: line_t + off / 1000.0, dur: cur_dur / 1000.0, n: cur_chars });
                    }
                }
                if let Some(gt) = b[i..].iter().position(|&c| c == '>') {
                    let inside: String = b[i + 1..i + gt].iter().collect();
                    let p: Vec<&str> = inside.split(',').collect();
                    cur_off = Some(p.get(0).and_then(|x| x.trim().parse().ok()).unwrap_or(0.0));
                    cur_dur = p.get(1).and_then(|x| x.trim().parse().ok()).unwrap_or(0.0);
                    cur_chars = 0;
                    i += gt + 1;
                    continue;
                } else {
                    break;
                }
            } else {
                text.push(b[i]);
                if !b[i].is_whitespace() {
                    cur_chars += 1;
                }
                i += 1;
            }
        }
        if let Some(off) = cur_off {
            if cur_chars > 0 {
                segs.push(Seg { t: line_t + off / 1000.0, dur: cur_dur / 1000.0, n: cur_chars });
            }
        }
        let text = text.trim().to_string();
        if !text.is_empty() {
            out.push(LrcLine { t: line_t, text, segs });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore]
    fn probe_cdp() {
        let sl = super::fetch_current("", "").expect("no lyric");
        let l = super::parse_content(&sl.content);
        println!(
            "name={} artist={} dur={:.1} lines={} cnlen={}",
            sl.name,
            sl.artist,
            sl.duration,
            l.len(),
            sl.cn.len()
        );
        for x in l.iter().take(5) {
            println!("{:7.2} segs={} {}", x.t, x.segs.len(), x.text);
        }
        for x in l.iter().rev().take(3) {
            println!("LAST {:7.2} {}", x.t, x.text);
        }
    }

    #[test]
    #[ignore]
    fn probe_trial_meta() {
        println!("trial_meta={:?}", super::fetch_trial_meta());
    }

    #[test]
    #[ignore]
    fn probe_no_nav_timing() {
        let t = std::time::Instant::now();
        let r = super::fetch_current_no_nav("", "");
        println!(
            "no_nav ok={} ms={} name={:?}",
            r.is_some(),
            t.elapsed().as_millis(),
            r.map(|x| x.name)
        );
    }

    #[test]
    #[ignore]
    fn probe_media_clock() {
        let mc = super::fetch_media_clock(74.0, 273.7);
        eprintln!("MEDIA_CLOCK={:?}", mc);
        assert!(mc.is_some(), "fetch_media_clock returned None");
    }

    #[test]
    #[ignore]
    fn probe_enable_frames() {
        let u = super::page_ws_url().unwrap();
        let mut s = super::ws_connect(&u).unwrap();
        super::ws_send_text(
            &mut s,
            &serde_json::json!({"id":10,"method":"Runtime.enable","params":{}}).to_string(),
        )
        .unwrap();
        for i in 0..20 {
            let start = std::time::Instant::now();
            let r = super::ws_read_text(&mut s);
            let ms = start.elapsed().as_millis();
            match r {
                Some(x) => eprintln!("FRAME {} ms={} len={} head={:?}", i, ms, x.len(), &x[..x.len().min(90)]),
                None => {
                    eprintln!("FRAME {} ms={} NONE", i, ms);
                    break;
                }
            }
        }
    }

    #[test]
    #[ignore]
    fn probe_enable_wait() {
        let u = super::page_ws_url().unwrap();
        let mut s = super::ws_connect(&u).unwrap();
        super::ws_send_text(
            &mut s,
            &serde_json::json!({"id":77,"method":"Runtime.enable","params":{}}).to_string(),
        )
        .unwrap();
        let start = std::time::Instant::now();
        let mut events = 0;
        let mut result = None;
        for _ in 0..20000 {
            let r = super::ws_read_text(&mut s);
            match r {
                Some(x) => {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&x) {
                        if v.get("id").and_then(|q| q.as_i64()) == Some(77) {
                            result = Some(v);
                            break;
                        }
                    }
                    events += 1;
                }
                None => break,
            }
        }
        eprintln!(
            "ENABLE_WAIT ms={} events={} got={}",
            start.elapsed().as_millis(),
            events,
            result.is_some()
        );
    }

    #[test]
    #[ignore]
    fn probe_ws_stack() {
        let u = super::page_ws_url();
        eprintln!("WS_URL={:?}", u);
        if let Some(u) = u {
            let s = super::ws_connect(&u);
            eprintln!("WS_CONNECT={}", s.is_some());
            if let Some(mut s) = s {
                let r = super::cdp_send_wait(
                    &mut s,
                    2,
                    "Runtime.evaluate",
                    serde_json::json!({"expression":"1+1","returnByValue":true}),
                );
                eprintln!("EVAL_RESP={:?}", r);
                let en = super::cdp_send_wait(&mut s, 10, "Runtime.enable", serde_json::json!({}));
                eprintln!("ENABLE={}", en.is_some());
                let proto = super::cdp_send_wait(
                    &mut s,
                    11,
                    "Runtime.evaluate",
                    serde_json::json!({"expression":"HTMLMediaElement.prototype","objectGroup":"probe","returnByValue":false}),
                );
                eprintln!("PROTO={:?}", proto);
                let pid = proto
                    .as_ref()
                    .and_then(|v| v.get("result"))
                    .and_then(|v| v.get("result"))
                    .and_then(|v| v.get("objectId"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if let Some(pid) = pid {
                    let objs = super::cdp_send_wait(
                        &mut s,
                        12,
                        "Runtime.queryObjects",
                        serde_json::json!({"prototypeObjectId":pid}),
                    );
                    eprintln!("OBJS={:?}", objs);
                    if let Some(aid) = objs
                        .as_ref()
                        .and_then(|v| v.get("result"))
                        .and_then(|v| v.get("objects"))
                        .and_then(|v| v.get("objectId"))
                        .and_then(|v| v.as_str())
                    {
                        let call = super::cdp_send_wait(
                            &mut s,
                            13,
                            "Runtime.callFunctionOn",
                            serde_json::json!({"objectId":aid,"functionDeclaration":super::MEDIA_PICK_FN,"returnByValue":true,"arguments":[{"value":74.0},{"value":273.7}]}),
                        );
                        eprintln!("CALL={:?}", call);
                    }
                }
            }
        }
    }
}

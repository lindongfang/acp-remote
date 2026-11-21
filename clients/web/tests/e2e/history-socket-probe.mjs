export function installHistoryProbe() {
  const NativeSocket = window.WebSocket;
  let armed, current;
  const bodies = new Map();
  const stream = bytes => btoa(String.fromCharCode(...bytes.slice(8,24)))
    .replaceAll("+","-").replaceAll("/","_").replace(/=+$/,"");
  window.hnArm = mode => { armed = mode; current = null; };
  window.hnReady = () => Boolean(current?.response && (current.mode === "retry" || current.complete));
  window.hnAttempt = () => current;
  window.hnInvalidations = [];
  window.hnCloseCodes = [];
  window.WebSocket = class extends NativeSocket {
    constructor(...args) {
      super(...args);
      this.addEventListener("close",event=>window.hnCloseCodes.push(event.code));
      const deliver = data => this.dispatchEvent(new MessageEvent("message",{data}));
      this.addEventListener("message", event => {
        if (!event.isTrusted) return;
        const data = event.data;
        if (typeof data === "string") {
          const frame = JSON.parse(data);
          if (frame.type === "history.invalidated") {
            window.hnInvalidations.push(frame.streamId);
            if (window.hnHoldNotice) event.stopImmediatePropagation();
            const body = bodies.get(frame.streamId);
            window.hnReplay = () => {
              if (!body) throw Error("missing invalidated stream body");
              deliver(data);
              deliver(body);
            };
          }
          if (!current || frame.type !== "result" || frame.requestId !== current.id) return;
          const attempt = current;
          if (!frame.ok || !frame.result?.streamId || frame.result.requestId !== attempt.id) throw Error("missing successful view descriptor");
          attempt.stream = frame.result.streamId;
          attempt.response = data;
          attempt.release = (degraded = false) => {
            attempt.degraded = degraded;
            if (!degraded) return deliver(data);
            const changed = JSON.parse(data);
            changed.result.status.syncState = "degraded_capacity_full";
            deliver(JSON.stringify(changed));
          };
          if (attempt.mode === "receive") return;
          event.stopImmediatePropagation();
          if (attempt.mode === "retry") return;
          if (attempt.mode === "request") deliver(JSON.stringify({...frame,ok:false,result:null,error:{code:"not_found"}}));
          // Collect this socket's actual snapshot even when the application never saw the descriptor.
          const credit = new Uint8Array(72);
          credit.set([65,67,80,86,1,2]);
          credit.set(Uint8Array.from(atob(attempt.stream.replaceAll("-","+").replaceAll("_","/")), c => c.charCodeAt(0)),8);
          attempt.credit = seq => {
            new DataView(credit.buffer).setBigUint64(24,seq);
            super.send(credit);
          };
          attempt.credit(0n);
          return;
        }
        if (!(data instanceof ArrayBuffer)) return;
        const bytes = new Uint8Array(data);
        const id = stream(bytes);
        if (current?.degraded && id === current.stream && bytes[56] === 3) {
          const status = JSON.parse(new TextDecoder().decode(bytes.slice(72)));
          if (status.type === "transcript_cache_state") {
            status.syncState = "degraded_capacity_full";
            const payload = new TextEncoder().encode(JSON.stringify(status));
            const patched = new Uint8Array(72+payload.length);
            patched.set(bytes.slice(0,72));
            patched.set(payload,72);
            const header = new DataView(patched.buffer);
            header.setBigUint64(48,BigInt(payload.length));
            header.setUint32(64,payload.length);
            event.stopImmediatePropagation();
            deliver(patched.buffer);
            return;
          }
        }
        if (bytes[56] === 1) {
          bodies.set(id,data);
          if (window.hnWatchMarker && new TextDecoder().decode(bytes.slice(72)).includes(window.hnWatchMarker.text)) window.hnWatchMarker.hits++;
        }
        if (window.hnTail && id === window.hnTail.stream) {
          if (bytes[56] === 5) window.hnTail.starts++;
          if (bytes[56] === 1 && new TextDecoder().decode(bytes.slice(72)).includes("HN live user 13")) window.hnTail.markers++;
        }
        if (!current || id !== current.stream || current.mode === "retry" || current.complete) return;
        const attempt = current;
        const header = new DataView(data);
        if (header.getBigUint64(24) !== BigInt(attempt.frames.length+1) ||
            bytes[5] !== 1 || data.byteLength !== 72+header.getUint32(64)) {
          throw Error("unexpected snapshot packet sequence or length");
        }
        attempt.frames.push(data);
        if (attempt.mode !== "receive" || bytes[56] === 4) event.stopImmediatePropagation();
        if (bytes[56] !== 4) {
          if (attempt.credit) attempt.credit(new DataView(data).getBigUint64(24));
          return;
        }
        if (header.getUint32(64) !== 0 || header.getUint16(6) !== 1) throw Error("invalid snapshot end");
        attempt.complete = true;
        attempt.fail = () => {
          const broken = data.slice(0);
          const header = new DataView(broken);
          header.setBigUint64(24,header.getBigUint64(24)+1n);
          deliver(broken);
        };
        attempt.replay = () => {
          let responses = 0, packets = 0;
          const count = event => {
            if (event.data === attempt.response) responses++;
            else if (attempt.frames.includes(event.data)) packets++;
          };
          this.addEventListener("message",count);
          try {
            deliver(attempt.response);
            for (const packet of attempt.frames) deliver(packet);
          } finally { this.removeEventListener("message",count); }
          return {responses,packets};
        };
      });
    }
    send(data) {
      if (typeof data === "string") {
        const frame = JSON.parse(data);
        if (frame.action === "view.open" && armed) {
          current = {id:frame.requestId, mode:armed, frames:[]};
          armed = null;
        }
        if (frame.action === "view.open" && window.hnOrdinary) {
          window.hnOrdinary = false;
          delete frame.payload.page;
          frame.payload.reason = "reopen";
          data = JSON.stringify(frame);
        }
      }
      super.send(data);
    }
  };
}

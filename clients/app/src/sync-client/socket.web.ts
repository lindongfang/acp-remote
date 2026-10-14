/**
 * `SocketPort` 的 **Web/PWA** 实现。
 *
 * ## 为什么全局 `WebSocket` 只在函数体内引用
 *
 * vitest 跑在 Node 环境，**没有** `WebSocket` 全局；模块求值期访问它会让
 * `import { openWebSocket } from "./socket.web"` 直接抛 `ReferenceError`，连带
 * 依赖本模块的类型被别的测试文件解析时一起失败。因此本文件的所有全局访问都在
 * `connect()` 的函数体内——构造 socket 时才求值，模块导入永远安全。
 *
 * 与 WP5b 的 `*.web.ts` + 端口分离惯例一致：端口在 `ports.ts`（无平台 API），
 * 实现在本文件（唯一触碰 `WebSocket` 的地方）。
 */

import type { SocketHandlers, SocketPort } from "./ports";
import { SYNC_WS_SUBPROTOCOL } from "./connection";

/** 建立一条 WebSocket 连接。 */
export function openWebSocket(input: { readonly url: string; readonly handlers: SocketHandlers }): SocketPort {
  // 函数体内引用全局：模块求值期不触碰它。
  //
  // subprotocol 是升级请求的一部分（`docs/SYNC_PROTOCOL.md` §3.1「客户端必须请求 WebSocket
  // subprotocol `acp-remote.sync.v1.json`」，服务端按它做 upgrade 白名单）：漏传时服务端直接
  // 拒绝升级，连接永远起不来。字面量只有 `connection.ts` 的 `SYNC_WS_SUBPROTOCOL` 一处。
  const socket = new WebSocket(input.url, SYNC_WS_SUBPROTOCOL);
  const handlers = input.handlers;

  socket.addEventListener("open", () => {
    handlers.onOpen();
  });
  socket.addEventListener("message", (event: MessageEvent) => {
    // 只交付文本原文：快照 digest 必须按原始 UTF-8 字节计算，重新序列化会改变字节。
    if (typeof event.data !== "string") {
      handlers.onError("received non-text frame");
      return;
    }
    handlers.onMessage(event.data);
  });
  socket.addEventListener("close", (event: CloseEvent) => {
    handlers.onClose(event.code, event.reason);
  });
  socket.addEventListener("error", () => {
    handlers.onError("websocket error");
  });

  return {
    send(text: string): void {
      socket.send(text);
    },
    close(code: number, reason: string): void {
      socket.close(code, reason);
    },
  };
}
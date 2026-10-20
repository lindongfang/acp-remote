/**
 * 附件连接的生命周期边界（`docs/SYNC_PROTOCOL.md` §3.4；`attachment-connection.ts`）。
 *
 * ## 判别力
 *
 * - **阻断先关后通知**：`onBlocked` 回调里读 `socket.closedWith`，若实现先通知、之后再关（或
 *   根本不关），这里读到 `null`；socket 泄漏正是靠这一条被钉住的。
 * - **旧连接的回调不得触碰新连接**：模拟「阻断后调用方重建连接，旧 socket 才真正关闭」，
 *   断言旧连接的 `onClosed` 不再上抛。若阻断只清引用不关闭 socket，旧连接的 `onClose` 会把
 *   调用方（`AttachmentTransport`）的新连接与在途传输一起 `#teardown()`。
 */

import { describe, expect, it } from "vitest";

import { ATTACHMENT_FEATURE } from "../protocol";
import { AttachmentConnection, attachmentSocketUrl } from "./attachment-connection";
import { CLOSE } from "./blocking";
import { DECLARED_SUBPROTOCOLS, REQUIRED_FEATURES, SUPPORTED_FEATURES } from "./connection";
import {
  FakeHostIdentity,
  FakeIdentity,
  FakeRandom,
  FakeSocketFactory,
  FakeTranscript,
  makeAuthenticated,
  makeChallenge,
  waitUntil,
} from "./testing/harness";

const HOST_ID = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
const CONNECTION_ID = "2de54db7-74ae-4c21-88e3-05df0dc2e637";

interface ClosureRecord {
  readonly code: number;
  readonly retryable: boolean;
  readonly detail: string;
}

interface BlockRecord {
  readonly cause: string;
  readonly code: number;
  readonly reason: string | null;
}

interface ConnectionHarness {
  readonly sockets: FakeSocketFactory;
  readonly closed: ClosureRecord[];
  readonly blocked: BlockRecord[];
  /** 通知阻断那一刻 socket 被关成了什么（`null` 即「先通知、后（或没有）关」）。 */
  readonly snapshot: { socketAtBlock: { readonly code: number; readonly reason: string } | null };
  /** 建一条被测连接（同一工厂可建多条，用于「旧连接不拆新连接」）。 */
  open(): AttachmentConnection;
}

/** 一套共享替身：工厂、回调记录，以及「通知阻断那一刻 socket 被关成了什么」。 */
function harness(): ConnectionHarness {
  const sockets = new FakeSocketFactory();
  const closed: ClosureRecord[] = [];
  const blocked: BlockRecord[] = [];
  const snapshot = { socketAtBlock: null as { readonly code: number; readonly reason: string } | null };

  /** 建一条被测连接（同一工厂可建多条，用于「旧连接不拆新连接」）。 */
  const open = (): AttachmentConnection =>
    new AttachmentConnection({
      url: attachmentSocketUrl("https://host.example:8443"),
      subprotocols: DECLARED_SUBPROTOCOLS,
      hostId: HOST_ID,
      identity: new FakeIdentity(),
      host: new FakeHostIdentity(),
      random: new FakeRandom(),
      transcript: new FakeTranscript(),
      openSocket: sockets.open,
      supportedFeatures: SUPPORTED_FEATURES,
      requiredFeatures: REQUIRED_FEATURES,
      onReady: () => undefined,
      onFrame: () => undefined,
      onClosed: (code, retryable, detail) => {
        closed.push({ code, retryable, detail });
      },
      onBlocked: (cause, code, reason) => {
        // 通知发生的**这一刻** socket 必须已经关闭：泄漏与「关完再通知」都由此区分。
        snapshot.socketAtBlock = sockets.latest.closedWith;
        blocked.push({ cause, code, reason: reason ?? null });
      },
      onRejected: () => undefined,
    });

  return { sockets, closed, blocked, snapshot, open };
}

/** 跑完握手（hello → challenge → proof → authenticated），并让本连接协商到附件能力。 */
async function completeHandshake(context: ConnectionHarness): Promise<void> {
  context.sockets.latest.open();
  await waitUntil(() => context.sockets.latest.types().includes("auth.client_hello"), "发出 clientHello");
  context.sockets.latest.deliver(
    makeChallenge({
      connectionId: CONNECTION_ID,
      selectedFeatures: [...REQUIRED_FEATURES, ATTACHMENT_FEATURE],
    }),
  );
  await waitUntil(() => context.sockets.latest.types().includes("auth.client_proof"), "发出 clientProof");
  context.sockets.latest.deliver(makeAuthenticated({ connectionId: CONNECTION_ID }));
}

describe("附件连接：阻断时先关闭 socket", () => {
  it("子协议未被选中：close 先于 onBlocked，日后旧 socket 的关闭不再上抛", () => {
    const context = harness();
    const connection = context.open();
    // 服务端升级成功但没有选中 subprotocol（§3.1 的「未选中」与断网同形）。
    context.sockets.latest.negotiated = "";
    context.sockets.latest.open();

    expect(context.blocked).toEqual([
      { cause: "version_incompatible", code: CLOSE.versionIncompatible, reason: "subprotocol_not_selected" },
    ]);
    expect(context.snapshot.socketAtBlock).toEqual({
      code: CLOSE.versionIncompatible,
      reason: "subprotocol_not_selected",
    });
    expect(connection.closed).toBe(true);
    expect(connection.link).toBeNull();

    // 阻断之后旧 socket 才真正关闭：它不得再走 onClosed（否则调用方会二次收尾）。
    context.sockets.latest.serverClose(1006, "network_lost");
    expect(context.closed).toEqual([]);
    expect(context.blocked).toHaveLength(1);
  });

  it("认证后收到 device_revoked：关闭码与阻断原因一致，且不再上抛 closed", async () => {
    const context = harness();
    const connection = context.open();
    await completeHandshake(context);
    expect(connection.link?.negotiated).toBe(true);

    context.sockets.latest.deliver({
      protocolVersion: 1,
      type: "error",
      messageId: "dddddddd-1111-4111-8111-000000000001",
      connectionId: CONNECTION_ID,
      connectionSequence: "1",
      body: {
        code: "auth.device_revoked",
        message: "设备已撤销",
        retryable: false,
        correlationId: null,
        details: {},
      },
    });

    expect(context.blocked).toEqual([{ cause: "device_revoked", code: CLOSE.deviceRevoked, reason: null }]);
    expect(context.snapshot.socketAtBlock).toEqual({ code: CLOSE.deviceRevoked, reason: "device_revoked" });
    expect(connection.closed).toBe(true);

    context.sockets.latest.serverClose(1006, "network_lost");
    expect(context.closed).toEqual([]);
  });

  it("旧连接被阻断后新连接已建立：旧 socket 的关闭不拆掉新连接", () => {
    const context = harness();
    const first = context.open();
    context.sockets.latest.negotiated = "";
    context.sockets.latest.open();
    expect(first.closed).toBe(true);

    // 调用方（AttachmentTransport.start）在阻断后重建一条连接。
    const second = context.open();
    expect(second.closed).toBe(false);

    // 旧 socket 现在才关闭：它属于 first，回调已被 first 的 `#closed` 挡住。
    context.sockets.sockets[0]?.serverClose(1006, "network_lost");
    expect(context.closed).toEqual([]);
    expect(second.closed).toBe(false);
    expect(context.blocked).toHaveLength(1);
    expect(context.blocked[0]?.cause).toBe("version_incompatible");
  });
});

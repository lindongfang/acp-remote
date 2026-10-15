/**
 * 连接端点与 wire 权威一致的两条**合同断言**（`plan.md` WP1 的 1.1 与 1.3）。
 *
 * ## 为什么断言在构造结果与机器登记上，而不是源码里的字面量
 *
 * - 出站地址：断言的是 `syncSocketUrl()` **构造出来的** URL（组合根正是调它拿连接地址），
 *   因此路径写错、少拼上一段、或把常量改成 `/sync` 都会变红。断言源码里写着 `/sync/v1` 是自证，
 *   改了代码照样绿。
 * - 子协议：前端必须声明服务端要求的 subprotocol，而它的权威登记在仓库根的机器资产里
 *   （`compatibility/errors/v1/errors.json` 的 `protocols.sync.subprotocol`，与
 *   `crates/server/src/sync/conn/mod.rs` 的 `WS_SUBPROTOCOL` 同源）。浏览器运行期读不到仓库资产，
 *   因此前端只能留一份 TS 字面量（`connection.ts` 的 `SYNC_WS_SUBPROTOCOL`，唯一出处），
 *   由本文件把它钉回登记：**登记改了而前端没跟**，或**前端改了而登记没跟**，都会在这里变红。
 *
 * ## 为什么这份合同测试落在 `src/sync-client/` 而不是 `src/protocol/contract.test.ts`
 *
 * `src/layers.test.ts` 按 `layers.ts` 的 `LAYER_ORDER` 逐条核对跨层 import：`protocol`（7）不得
 * 依赖 `sync-client`（5）。把断言放进 `protocol/contract.test.ts` 就必须让协议层 import 连接层的
 * 常量，那是**一条反向依赖**，会直接让分层用例变红。因此读仓库根资产这套机制照用
 * （`../protocol/paths` 的 `repoRoot`，方向 5 → 7 合法），断言位置放在被断言值的所属层。
 */

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { repoRoot } from "../protocol/paths";
import { DECLARED_SUBPROTOCOLS, SYNC_WS_SUBPROTOCOL, syncSocketUrl } from "./connection";

/** `compatibility/errors/v1/errors.json` —— Sync 子协议标识的机器登记。 */
const ERRORS_REGISTRY_PATH = resolve(repoRoot, "compatibility", "errors", "v1", "errors.json");

/** 读取机器登记并取出 sync 段的 subprotocol。 */
function readRegisteredSubprotocol(): string {
  const registry = JSON.parse(readFileSync(ERRORS_REGISTRY_PATH, "utf8")) as {
    readonly protocols: { readonly sync: { readonly subprotocol: string } };
  };
  return registry.protocols.sync.subprotocol;
}

describe("连接端点：出站地址的路径是权威端点（§3.1）", () => {
  it("由规范化来源推导的连接地址，路径为 /sync/v1", () => {
    const url = new URL(syncSocketUrl("https://host.example:8443"));

    // 这里写**字面量**而不是引用 `SYNC_WS_PATH`：引用常量的话，把常量改成 `/sync`（缺 `/v1`
    // 的旧口径）会让断言跟着一起改，测试永远绿——那正是本次要消解的口径冲突。
    expect(url.pathname).toBe("/sync/v1");
    // 正式连接只允许 `wss://`（§8），且规范化来源的 host 与端口必须原样保留。
    expect(url.protocol).toBe("wss:");
    expect(url.host).toBe("host.example:8443");
    // 来源里不含 path/query/fragment，推导结果也不该凭空多出来。
    expect(url.search).toBe("");
    expect(url.hash).toBe("");
  });

  it("默认端口与显式端口都保留在地址里", () => {
    expect(new URL(syncSocketUrl("https://host.example")).host).toBe("host.example");
    expect(new URL(syncSocketUrl("https://127.0.0.1:8443")).pathname).toBe("/sync/v1");
  });
});

describe("连接端点：子协议标识取自机器登记的同一来源（§3.1）", () => {
  it("前端声明的子协议与登记逐字相等", () => {
    const registered = readRegisteredSubprotocol();

    // 该断言在下面两种情况下都会报红：
    // 1. 登记被改动（`protocols.sync.subprotocol` 换成别的取值）而前端未跟随——
    //    那正是「服务端按 subprotocol 做 upgrade 白名单」的静默失败：升级被拒、连接起不来；
    // 2. 前端把字面量改成别的值（或另抄第二份）而登记未改。
    expect(SYNC_WS_SUBPROTOCOL).toBe(registered);
    // 实际声明出去的就是这一条：声明列表若被改成别的取值，端口侧与登记侧会立刻不一致。
    expect(DECLARED_SUBPROTOCOLS).toEqual([registered]);
  });
});

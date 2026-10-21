/**
 * 连接问题模型的判定（五种变体、动作、路由排除与诊断详情）。
 *
 * 断言的是**判定**：标题/错误码/动作/重连进度都来自同一份纯函数，组件只渲染结果。
 */

import { describe, expect, it } from "vitest";

import type { ConnectionStateName } from "../state";
import {
  buildConnectionProblem,
  networkAvailabilityOf,
  shouldShowConnectionProblem,
} from "./connection-problem";
import type { ConnectionProblemInput } from "./connection-problem";

const AT_MS = Date.parse("2026-10-01T00:00:08.000Z");
const CLOSED = { code: 4500, reason: "snapshot_validation_backlog", atMs: AT_MS } as const;
const RECONNECT = { attempts: 3, nextRetryAtMs: AT_MS + 8_000 } as const;

function problemOf(
  connection: ConnectionStateName,
  overrides: Partial<ConnectionProblemInput> = {},
) {
  return buildConnectionProblem({
    connection,
    hasAuthenticated: true,
    lastClose: CLOSED,
    reconnect: RECONNECT,
    nowMs: AT_MS,
    ...overrides,
  });
}

describe("网络可用性（顶栏只用到的两档）", () => {
  it("在线、离线与未知三组取值逐状态固定", () => {
    for (const state of ["replaying", "online", "incompatible", "identity_changed", "replaced"] as const) {
      expect(networkAvailabilityOf(state), state).toBe("online");
    }
    for (const state of ["disconnected", "reconnecting", "revoked"] as const) {
      expect(networkAvailabilityOf(state), state).toBe("offline");
    }
    // 已认证后的重新认证仍属于离线；首次认证仍未知。
    for (const state of ["connecting", "authenticating"] as const) {
      expect(networkAvailabilityOf(state), state).toBeNull();
      expect(networkAvailabilityOf(state, true), state).toBe("offline");
    }
    for (const state of ["unpaired", "pairing", "connecting", "authenticating"] as const) {
      expect(networkAvailabilityOf(state), state).toBeNull();
    }
  });
});

describe("连接问题的五种变体", () => {
  it("没有问题时返回 null：在线、追平中与尚未认证的握手阶段", () => {
    for (const state of ["online", "replaying", "unpaired", "pairing", "connecting", "authenticating"] as const) {
      const preAuth = state === "unpaired" || state === "pairing" || state === "connecting" || state === "authenticating";
      expect(problemOf(state, { hasAuthenticated: !preAuth }), state).toBeNull();
    }
  });

  it("普通离线：无按钮、带自动重连进度，断开与重连中标题不同", () => {
    const disconnected = problemOf("disconnected");
    expect(disconnected?.kind).toBe("offline");
    expect(disconnected?.title).toBe("连接已断开");
    expect(disconnected?.action).toBe("none");
    expect(disconnected?.errorCode).toBeNull();
    expect(disconnected?.reconnect).toEqual(RECONNECT);
    expect(disconnected?.closeCode).toBe(CLOSED.code);
    expect(disconnected?.description).toContain("自动重试");

    const reconnecting = problemOf("reconnecting");
    expect(reconnecting?.kind).toBe("offline");
    expect(reconnecting?.title).toBe("正在重连");
    expect(reconnecting?.action).toBe("none");
  });

  it("设备被撤销：重新配对，错误码 auth.device_revoked", () => {
    const problem = problemOf("revoked");
    expect(problem).toMatchObject({
      kind: "revoked",
      title: "设备已被撤销",
      errorCode: "auth.device_revoked",
      action: "repair",
      reconnect: null,
    });
  });

  it("版本或能力不兼容：重新加载，错误码 protocol.version_unsupported", () => {
    const problem = problemOf("incompatible");
    expect(problem).toMatchObject({
      kind: "incompatible",
      title: "版本或能力不兼容",
      errorCode: "protocol.version_unsupported",
      action: "reload",
      reconnect: null,
    });
  });

  it("主机身份变化：重新配对，错误码 auth.host_mismatch", () => {
    const problem = problemOf("identity_changed");
    expect(problem).toMatchObject({
      kind: "identity_changed",
      title: "主机身份已变化",
      errorCode: "auth.host_mismatch",
      action: "repair",
      reconnect: null,
    });
  });

  it("被其它标签页顶替：重新加载，错误码 connection.replaced", () => {
    const problem = problemOf("replaced");
    expect(problem).toMatchObject({
      kind: "replaced",
      title: "已被其他标签页顶替",
      errorCode: "connection.replaced",
      action: "reload",
      reconnect: null,
    });
  });

  it("动作只有三种取值，且每个变体恰好一个动作", () => {
    const actions = (["disconnected", "revoked", "incompatible", "identity_changed", "replaced"] as const).map(
      (state) => problemOf(state)?.action,
    );
    expect(actions).toEqual(["none", "repair", "reload", "repair", "reload"]);
    for (const action of actions) {
      expect(["none", "repair", "reload"]).toContain(action);
    }
  });

  it("没有关闭记录时用时钟补上发生时刻，关闭码为 null", () => {
    const problem = problemOf("disconnected", { lastClose: null });
    expect(problem?.closeCode).toBeNull();
    expect(problem?.occurredAtMs).toBe(AT_MS);
    expect(problem?.technicalDetail).toContain("close=none");
    expect(problem?.technicalDetail).toContain("at=2026-10-01T00:00:08.000Z");
  });
});

describe("诊断详情不含秘密", () => {
  it("机读关闭原因原样写入单条诊断", () => {
    const problem = problemOf("revoked");
    expect(problem?.technicalDetail).toBe(
      "error=auth.device_revoked; close=4500; reason=snapshot_validation_backlog; at=2026-10-01T00:00:08.000Z",
    );
  });

  it("必需能力缺失使用协议里对应的错误码", () => {
    const problem = problemOf("incompatible", {
      lastClose: { code: 4406, reason: "required_features_missing", atMs: AT_MS },
    });
    expect(problem?.errorCode).toBe("protocol.feature_required");
  });

  it("已认证后的重新认证产生离线问题；首次认证不产生", () => {
    for (const state of ["connecting", "authenticating"] as const) {
      expect(problemOf(state, { hasAuthenticated: false }), state).toBeNull();
      expect(problemOf(state, { hasAuthenticated: true })?.kind, state).toBe("offline");
    }
  });

  it("夹带密钥/令牌/配对字样的原因被整体抹掉", () => {
    for (const reason of [
      "pairingSecret=placeholder",
      "token placeholder",
      "api_key=placeholder",
      "credential placeholder",
    ]) {
      const problem = problemOf("incompatible", { lastClose: { code: 4406, reason, atMs: AT_MS } });
      expect(problem?.technicalDetail).not.toContain(reason);
      expect(problem?.technicalDetail).toContain("reason=[redacted]");
    }
  });
});

describe("路由排除", () => {
  it("配对入口与主机页不呈现弹层（含查询串、片段与尾斜杠）", () => {
    for (const path of [
      "/pair",
      "/pair/",
      "/pair?kind=sync",
      "/node-link/pair",
      "/node-link/pair/",
      "/node-link/pair#data=x",
      "/hosts",
      "/hosts/",
    ]) {
      expect(shouldShowConnectionProblem(path), path).toBe(false);
    }
  });

  it("业务路径呈现弹层；前缀相同但不是排除项的路由不误判", () => {
    for (const path of ["/", "/dir/work-api", "/session/abc", "/pairing", "/hosts-extra"]) {
      expect(shouldShowConnectionProblem(path), path).toBe(true);
    }
  });

  it("路径缺失时交给调用方决定（这里按允许处理，调用方仍要求已认证）", () => {
    expect(shouldShowConnectionProblem(null)).toBe(true);
  });
});

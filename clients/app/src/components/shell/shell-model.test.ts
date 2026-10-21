/**
 * 共享外壳的判定（`design.md` D1；`spec` 的「共享外壳承担连接状态与全局入口」）。
 *
 * 这里断言的是**判定**，不是呈现：
 *
 * - 顶栏判据只有 `usePathname()` 这一处——配对入口的两条路径都隐藏，其它路径都呈现；
 * - 胶囊档位与短词：健康态（在线/连接中/认证中/追平中）不追加短词，需要注意与阻断态各有一个
 *   非空短词（用**全状态遍历**断言，因此将来新增一个连接状态时，忘了给短词会直接红）；
 * - 明文托管由环境事实给出（`secureContext` 为假 ⇒ 呈现横幅与徽标）。
 */

import { describe, expect, it } from "vitest";

import type { ConnectionStateName } from "../../state";
import {
  buildDeviceCapsule,
  buildShellModel,
  deviceLabelOf,
  isBusinessPath,
  isPairingPath,
  showsGlobalTopBar,
} from "./shell-model";

/** 连接状态机的全部 12 个状态（两点同源：状态机清单）。 */
const ALL_STATES: readonly ConnectionStateName[] = [
  "unpaired",
  "pairing",
  "disconnected",
  "connecting",
  "authenticating",
  "replaying",
  "online",
  "reconnecting",
  "revoked",
  "incompatible",
  "identity_changed",
  "replaced",
];

/** 已认证后按在线呈现的状态（阻断态里三个是「连接过但被拒」，仍算网络在线）。 */
const ONLINE_STATES: readonly ConnectionStateName[] = [
  "replaying",
  "online",
  "incompatible",
  "identity_changed",
  "replaced",
];

/** 已认证后按离线呈现的状态（重新认证仍属于重连，不退回未知）。 */
const OFFLINE_STATES: readonly ConnectionStateName[] = [
  "disconnected",
  "reconnecting",
  "revoked",
  "connecting",
  "authenticating",
];

/** 尚未认证、可用性未知的状态。 */
const UNKNOWN_STATES: readonly ConnectionStateName[] = ["unpaired", "pairing"];

describe("顶栏判据（只有 usePathname() 一处）", () => {
  it("配对入口的两条路径都不呈现全局顶栏（含尾斜杠、查询串与片段的归一）", () => {
    for (const path of ["/pair", "/pair/", "/pair?kind=sync", "/pair#data=x", "/node-link/pair", "/node-link/pair/"]) {
      expect(isPairingPath(path), path).toBe(true);
      expect(showsGlobalTopBar(path), path).toBe(false);
    }
  });

  it("其它路径都呈现顶栏；前缀相同但不是配对入口的路径不误判", () => {
    for (const path of [
      "/",
      "/hosts",
      "/dir/0123456789abcdef0123456789abcdef",
      "/session/abc",
      "/pairing",
      "/pairs",
    ]) {
      expect(isPairingPath(path), path).toBe(false);
      expect(showsGlobalTopBar(path), path).toBe(true);
    }
  });

  it("路径缺失（尚未挂载路由）时不隐藏顶栏", () => {
    expect(isPairingPath(null)).toBe(false);
    expect(showsGlobalTopBar(null)).toBe(true);
  });
});

describe("首屏认证闸门的路由判据", () => {
  it("业务路由：根目录页、目录详情页与会话页（含查询串、片段与尾斜杠）", () => {
    for (const path of ["/", "/?q=1", "/dir/work-api", "/session/abc", "/session/abc/?x=1#frag"]) {
      expect(isBusinessPath(path), path).toBe(true);
    }
  });

  it("配对入口与主机页不是业务路由；前缀相同但不匹配的路由不误判", () => {
    for (const path of ["/pair", "/node-link/pair", "/hosts", "/pairing", "/directory", "/sessions"]) {
      expect(isBusinessPath(path), path).toBe(false);
    }
    expect(isBusinessPath(null)).toBe(false);
  });
});

describe("设备胶囊", () => {
  it("标识取主机地址的 host 段；未知主机不编造设备名", () => {
    expect(deviceLabelOf("https://127.0.0.1:8443")).toBe("127.0.0.1:8443");
    expect(deviceLabelOf("https://pc.example.ts.net")).toBe("pc.example.ts.net");
    expect(deviceLabelOf(null)).toBe("未知主机");
    expect(deviceLabelOf("")).toBe("未知主机");
    expect(deviceLabelOf("不是地址")).toBe("未知主机");
  });

  it("在线时是正常档、状态为「在线」（页面其它位置也没有第二个状态徽标）", () => {
    const capsule = buildDeviceCapsule({ state: "online", hostAddress: "https://pc.example.ts.net" });
    expect(capsule.tone).toBe("ok");
    expect(capsule.availability).toBe("online");
    expect(capsule.deviceLabel).toBe("pc.example.ts.net");
    expect(capsule.statusLabel).toBe("在线");
  });

  it("已认证后只有在线/离线两档（遍历全部状态）", () => {
    for (const state of ALL_STATES) {
      const capsule = buildDeviceCapsule({ state, hostAddress: null, hasAuthenticated: true });
      if (ONLINE_STATES.includes(state)) {
        expect(capsule.availability, state).toBe("online");
        expect(capsule.tone, state).toBe("ok");
        expect(capsule.statusLabel, state).toBe("在线");
      } else if (OFFLINE_STATES.includes(state)) {
        expect(capsule.availability, state).toBe("offline");
        expect(capsule.tone, state).toBe("danger");
        expect(capsule.statusLabel, state).toBe("离线");
      } else {
        expect(UNKNOWN_STATES.includes(state), state).toBe(true);
        expect(capsule.availability, state).toBeNull();
        expect(capsule.tone, state).toBe("warn");
        expect(capsule.statusLabel, state).toBe("连接状态未知");
      }
    }
  });

  it("首次认证前未知；已认证后的重新认证仍显示离线", () => {
    for (const state of ["connecting", "authenticating"] as const) {
      expect(buildDeviceCapsule({ state, hostAddress: null }).statusLabel, state).toBe("连接状态未知");
      expect(buildDeviceCapsule({ state, hostAddress: null, hasAuthenticated: true }).statusLabel, state).toBe("离线");
    }
  });

  it("阻断态只表达在线或离线，具体原因由连接问题弹层承担", () => {
    for (const state of ["revoked", "incompatible", "identity_changed", "replaced"] as const) {
      const capsule = buildDeviceCapsule({ state, hostAddress: null });
      expect(["在线", "离线"]).toContain(capsule.statusLabel);
    }
  });

  it("组合根尚未装配（状态未知）时给一个中性的胶囊，而不是伪造某个连接状态", () => {
    const capsule = buildDeviceCapsule({ state: null, hostAddress: null });
    expect(capsule.state).toBeNull();
    expect(capsule.availability).toBeNull();
    expect(capsule.tone).toBe("warn");
    expect(capsule.statusLabel).toBe("连接状态未知");
  });
});

describe("外壳模型", () => {
  it("明文托管给出 insecure；安全托管不给", () => {
    const insecure = buildShellModel({
      pathname: "/",
      connectionState: "online",
      hostAddress: null,
      secureContext: false,
    });
    expect(insecure.insecure).toBe(true);
    expect(insecure.topBarVisible).toBe(true);

    const secure = buildShellModel({
      pathname: "/pair",
      connectionState: "online",
      hostAddress: null,
      secureContext: true,
    });
    expect(secure.insecure).toBe(false);
    expect(secure.topBarVisible).toBe(false);
  });
});

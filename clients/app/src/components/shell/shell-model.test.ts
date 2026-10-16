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

import { CONNECTION_STATE_LABELS } from "../../features/host-panel-model";
import type { ConnectionStateName } from "../../state";
import {
  buildDeviceCapsule,
  buildShellModel,
  deviceLabelOf,
  isPairingPath,
  showsGlobalTopBar,
} from "./shell-model";

/** 连接状态机的全部 12 个状态（取自状态文案表，两点同源）。 */
const ALL_STATES = Object.keys(CONNECTION_STATE_LABELS) as readonly ConnectionStateName[];

/** 健康态：不追加短词（原型把 online 与 syncing 合并成一个正常态）。 */
const HEALTHY_STATES: readonly ConnectionStateName[] = ["online", "connecting", "authenticating", "replaying"];

describe("顶栏判据（只有 usePathname() 一处）", () => {
  it("配对入口的两条路径都不呈现全局顶栏（含尾斜杠、查询串与片段的归一）", () => {
    for (const path of ["/pair", "/pair/", "/pair?kind=sync", "/pair#data=x", "/node-link/pair", "/node-link/pair/"]) {
      expect(isPairingPath(path), path).toBe(true);
      expect(showsGlobalTopBar(path), path).toBe(false);
    }
  });

  it("其它路径都呈现顶栏；前缀相同但不是配对入口的路径不误判", () => {
    for (const path of ["/", "/hosts", "/dir/alpha", "/session/abc", "/pairing", "/pairs"]) {
      expect(isPairingPath(path), path).toBe(false);
      expect(showsGlobalTopBar(path), path).toBe(true);
    }
  });

  it("路径缺失（尚未挂载路由）时不隐藏顶栏", () => {
    expect(isPairingPath(null)).toBe(false);
    expect(showsGlobalTopBar(null)).toBe(true);
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

  it("在线时是正常档且不追加短词（页面其它位置也不该有第二个状态徽标）", () => {
    const capsule = buildDeviceCapsule({ state: "online", hostAddress: "https://pc.example.ts.net" });
    expect(capsule.tone).toBe("ok");
    expect(capsule.shortWord).toBeNull();
    expect(capsule.deviceLabel).toBe("pc.example.ts.net");
    expect(capsule.statusLabel).toBe(CONNECTION_STATE_LABELS.online);
  });

  it("健康态一律不追加短词", () => {
    for (const state of HEALTHY_STATES) {
      const capsule = buildDeviceCapsule({ state, hostAddress: null });
      expect(capsule.tone, state).toBe("ok");
      expect(capsule.shortWord, state).toBeNull();
    }
  });

  it("需要注意与阻断态都有档位与非空短词（遍历全部状态）", () => {
    for (const state of ALL_STATES) {
      const capsule = buildDeviceCapsule({ state, hostAddress: null });
      if (HEALTHY_STATES.includes(state)) continue;
      expect(capsule.tone, state).not.toBe("ok");
      expect(capsule.shortWord, state).not.toBeNull();
      expect((capsule.shortWord ?? "").length, state).toBeGreaterThan(0);
      // 短词是浓缩，不是完整文案的副本：完整文案另有出处（`statusLabel`）。
      expect(capsule.statusLabel, state).toBe(CONNECTION_STATE_LABELS[state]);
    }
  });

  it("四个阻断态的表态与原型口径一致（撤销 / 身份变化 / 不兼容 / 被顶替）", () => {
    const words = (state: ConnectionStateName): { readonly tone: string; readonly word: string | null } => {
      const capsule = buildDeviceCapsule({ state, hostAddress: null });
      return { tone: capsule.tone, word: capsule.shortWord };
    };
    expect(words("revoked")).toEqual({ tone: "danger", word: "已被移除" });
    expect(words("identity_changed")).toEqual({ tone: "danger", word: "密钥已变化" });
    expect(words("incompatible")).toEqual({ tone: "danger", word: "版本不兼容" });
    expect(words("replaced")).toEqual({ tone: "danger", word: "已在别处接管" });
    expect(words("reconnecting")).toEqual({ tone: "warn", word: "正在重连" });
    expect(words("disconnected")).toEqual({ tone: "warn", word: "离线" });
    expect(words("unpaired")).toEqual({ tone: "danger", word: "未配对" });
  });

  it("组合根尚未装配（状态未知）时给一个中性的胶囊，而不是伪造某个连接状态", () => {
    const capsule = buildDeviceCapsule({ state: null, hostAddress: null });
    expect(capsule.state).toBeNull();
    expect(capsule.tone).toBe("warn");
    expect(capsule.shortWord).toBe("状态未知");
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

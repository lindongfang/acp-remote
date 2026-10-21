/**
 * 连接进度与最近关闭事实（连接问题弹层的数据来源）。
 *
 * 这些字段只是状态事实：`hasAuthenticated` 决定首屏闸门何时放行，`lastClose`/`reconnect`
 * 决定弹层里的码位与重连进度。
 */

import { describe, expect, it } from "vitest";

import { onlineStore, storeWith } from "./testing/fixtures";

const NOW = 1_700_000_000_000;

describe("已认证标记", () => {
  it("初始为未认证；`authenticated` 到达后才为真", () => {
    const { store } = storeWith({ resources: null });
    expect(store.state.hasAuthenticated).toBe(false);

    const { store: online } = onlineStore({});
    expect(online.state.hasAuthenticated).toBe(true);
  });
});

describe("最近一次关闭/阻断", () => {
  it("初始没有关闭事实；`closed` 记录关闭码、对端原因与仓库时钟", () => {
    const { store } = onlineStore({ now: () => NOW });
    expect(store.state.lastClose).toBeNull();

    store.apply({ kind: "closed", code: 4500, retryable: true, detail: "snapshot_validation_backlog" });
    expect(store.state.lastClose).toEqual({
      code: 4500,
      reason: "snapshot_validation_backlog",
      atMs: NOW,
    });
  });

  it("`closed` 没有 detail 时原因为 null（不编造对端说明）", () => {
    const { store } = onlineStore({ now: () => NOW });
    store.apply({ kind: "closed", code: 1006, retryable: true });
    expect(store.state.lastClose).toEqual({ code: 1006, reason: null, atMs: NOW });
  });

  it("`blocked` 记录关闭码与阻断原因（无细化原因时取 cause）", () => {
    const { store } = onlineStore({ now: () => NOW });
    store.apply({ kind: "blocked", cause: "device_revoked", code: 4410 });
    expect(store.state.lastClose).toEqual({ code: 4410, reason: "device_revoked", atMs: NOW });
  });

  it("`blocked` 带细化原因时保留细化原因", () => {
    const { store } = onlineStore({ now: () => NOW });
    store.apply({ kind: "blocked", cause: "version_incompatible", code: 4406, reason: "required_features_missing" });
    expect(store.state.lastClose).toEqual({ code: 4406, reason: "required_features_missing", atMs: NOW });
  });
});

describe("自动重连进度", () => {
  it("安排、开始与清除三步各自只改对应的字段", () => {
    const { store } = onlineStore({});
    expect(store.reconnect).toEqual({ attempts: 0, nextRetryAtMs: null });

    store.noteReconnectScheduled(1, NOW + 1_000);
    expect(store.reconnect).toEqual({ attempts: 1, nextRetryAtMs: NOW + 1_000 });
    expect(store.state.reconnect).toEqual({ attempts: 1, nextRetryAtMs: NOW + 1_000 });

    store.noteReconnectAttemptStarted();
    expect(store.reconnect).toEqual({ attempts: 1, nextRetryAtMs: null });

    store.clearReconnectProgress();
    expect(store.reconnect).toEqual({ attempts: 0, nextRetryAtMs: null });
  });

  it("重复写入相同进度不产生新的状态对象（避免无意义重渲染）", () => {
    const { store } = onlineStore({});
    store.noteReconnectScheduled(2, NOW);
    const state = store.state;
    store.noteReconnectScheduled(2, NOW);
    expect(store.state).toBe(state);
  });
});

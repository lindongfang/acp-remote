import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ManagementConnectionError, ManagementRequestError } from "../src/management/client";
import { ManagementProtocolError } from "../src/management/protocol";
import { ManagementReconnect } from "../src/management/reconnect";

describe("management connection recovery", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("backs off transient failures with one bounded timer and resets after success", () => {
    const retry = vi.fn();
    const reconnect = new ManagementReconnect(retry);
    const error = new ManagementConnectionError("connection_closed", 1006);
    for (const delay of [2_000, 4_000, 8_000, 16_000, 30_000, 30_000]) {
      expect(reconnect.schedule(error)).toBe(delay);
      expect(reconnect.schedule(error)).toBe(delay);
      expect(vi.getTimerCount()).toBe(1);
      vi.advanceTimersByTime(delay);
    }
    expect(retry).toHaveBeenCalledTimes(6);
    reconnect.reset();
    expect(reconnect.schedule(error)).toBe(2_000);
    reconnect.cancel();
  });

  it("cancels pending recovery on page exit or intentional disconnect", () => {
    const retry = vi.fn();
    const reconnect = new ManagementReconnect(retry);
    reconnect.schedule(new ManagementConnectionError("request_timeout"));
    reconnect.cancel();
    vi.advanceTimersByTime(60_000);
    expect(retry).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("does not retry rejected identities or invalid node proofs", () => {
    const retry = vi.fn();
    const reconnect = new ManagementReconnect(retry);
    for (const error of [
      new ManagementConnectionError("revoked", 4410),
      new ManagementConnectionError("connection_closed", 4401),
      new ManagementConnectionError("connection_closed", 4400),
      new ManagementProtocolError("host_proof_invalid"),
      new ManagementProtocolError("node_mismatch"),
      new Error("unclassified failure"),
    ]) {
      reconnect.schedule(new ManagementConnectionError("request_timeout"));
      expect(reconnect.schedule(error)).toBeNull();
      expect(vi.getTimerCount()).toBe(0);
    }
    vi.advanceTimersByTime(120_000);
    expect(retry).not.toHaveBeenCalled();
  });

  it("waits a full rate-limit window before retrying", () => {
    const retry = vi.fn();
    const reconnect = new ManagementReconnect(retry);
    for (const error of [
      new ManagementConnectionError("connection_closed", 4429),
      new ManagementRequestError("rate_limited"),
    ]) {
      expect(reconnect.schedule(error)).toBe(60_000);
      vi.advanceTimersByTime(59_999);
      const previous = retry.mock.calls.length;
      vi.advanceTimersByTime(1);
      expect(retry.mock.calls.length).toBe(previous + 1);
    }
  });

  it("preserves the retry deadline across online events and page suspension", () => {
    const retry = vi.fn();
    const reconnect = new ManagementReconnect(retry);
    reconnect.schedule(new ManagementConnectionError("connection_closed", 4429));
    reconnect.resume();
    vi.advanceTimersByTime(10_000);
    reconnect.cancel();
    reconnect.resume();
    reconnect.resume();
    expect(vi.getTimerCount()).toBe(1);
    vi.advanceTimersByTime(49_999);
    expect(retry).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it("automatically resumes after node restart or temporary storage failure", () => {
    const retry = vi.fn();
    const reconnect = new ManagementReconnect(retry);
    for (const error of [
      new ManagementConnectionError("connection_closed", 1001),
      new ManagementConnectionError("connection_closed", 4500),
      new ManagementRequestError("node_stopping"),
    ]) {
      reconnect.reset();
      expect(reconnect.schedule(error)).toBe(2_000);
      vi.advanceTimersByTime(2_000);
    }
    expect(retry).toHaveBeenCalledTimes(3);
  });
});

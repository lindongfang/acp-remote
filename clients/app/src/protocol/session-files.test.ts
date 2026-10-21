/**
 * 四条会话文件查询的严格解码测试。
 *
 * 样例**只从**仓库根 `fixtures/sync/v1/manifest.json` 解析，不在 `clients/app` 下复制；
 * Rust 与 TS 消费同一批 fixture（`AGENTS.md` §6）。反向样例按 manifest 的 `valid: false`
 * 分类（schema 级拒绝由仓库根 `scripts/check-schema-fixtures.mjs` 负责，本文件验证客户端解码器
 * 同样逐项拒绝）。
 */

import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import {
  MANIFEST_REPO_ROOT,
  readManifest,
  resolveFixtureCase,
  type FixtureCase,
} from "./manifest";
import {
  SESSION_FILES_COMMANDS,
  SESSION_FILES_FEATURE,
  decodeSessionFilesCommandBody,
  decodeSessionFilesResult,
  isSessionFilesCommandName,
  type SessionFilesCommandName,
} from "./session-files";

/** 按 manifest 的 fixture 名取绝对值；不存在直接失败（证明读的是共享资产）。 */
function fixturePath(name: string): string {
  const entry = readManifest().cases.find((candidate) => candidate.fixture.endsWith(`/${name}`));
  if (entry === undefined) throw new Error(`manifest 里没有样例 ${name}`);
  return resolveFixtureCase(entry).fixturePath;
}

/** 读取 fixture 的 JSON。 */
function fixtureJson(name: string): Record<string, unknown> {
  return JSON.parse(readFileSync(fixturePath(name), "utf8")) as Record<string, unknown>;
}

/** 取 fixture 的 `body`。 */
function bodyOf(name: string): unknown {
  return fixtureJson(name)["body"];
}

/** manifest 里某个关键字相关的全部 case。 */
function casesMatching(pattern: RegExp): readonly FixtureCase[] {
  return readManifest().cases.filter((entry) => pattern.test(entry.fixture));
}

describe("session-files：共享 fixture 与 feature 登记", () => {
  it("feature 与四条命令名来自同一份客户端词表", () => {
    expect(SESSION_FILES_FEATURE).toBe("core.session-files.v1");
    expect(SESSION_FILES_COMMANDS).toEqual([
      "session.files.list",
      "session.files.read",
      "session.git.status",
      "session.git.diff",
    ]);
    for (const command of SESSION_FILES_COMMANDS) expect(isSessionFilesCommandName(command)).toBe(true);
    expect(isSessionFilesCommandName("session.list")).toBe(false);
  });

  it("WP1 的合法/非法样例都在仓库根 manifest 中，且没有被复制到 clients/app", () => {
    expect(MANIFEST_REPO_ROOT.replaceAll("\\", "/").endsWith("/clients/app")).toBe(false);
    expect(casesMatching(/command-session-files-|command-session-git-/)).toHaveLength(16);
    expect(casesMatching(/command-result-session-files-|command-result-session-git-/)).toHaveLength(6);
  });
});

describe("session-files：合法命令与结果", () => {
  const commands = [
    ["command-session-files-list.json", "session.files.list"],
    ["command-session-files-read.json", "session.files.read"],
    ["command-session-git-status.json", "session.git.status"],
    ["command-session-git-diff.json", "session.git.diff"],
  ] as const;
  it.each(commands)("%s 解码出 %s", (file, expected) => {
    const decoded = decodeSessionFilesCommandBody(bodyOf(file));
    expect(decoded.ok).toBe(true);
    if (!decoded.ok) return;
    expect(decoded.value.command).toBe(expected);
  });

  const results = [
    ["command-result-session-files-list-completed.json", "session.files.list"],
    ["command-result-session-files-read-completed.json", "session.files.read"],
    ["command-result-session-git-status-completed.json", "session.git.status"],
    ["command-result-session-git-diff-completed.json", "session.git.diff"],
  ] as const;
  it.each(results)("%s 的 result 解码成功", (file, command) => {
    const body = bodyOf(file) as { result: unknown };
    const decoded = decodeSessionFilesResult(command as SessionFilesCommandName, body.result);
    expect(decoded.ok).toBe(true);
    if (decoded.ok && command === "session.files.list") {
      expect(decoded.value).toMatchObject({ path: "", gitState: "available" });
    }
    if (decoded.ok && command === "session.files.read") {
      expect(decoded.value).toMatchObject({ kind: "text", eof: true, previewTruncated: false });
    }
    if (decoded.ok && command === "session.git.diff") {
      expect(decoded.value).toMatchObject({ base: "head", kind: "text", truncated: false });
    }
  });
});

describe("session-files：非法命令显式拒绝", () => {
  const invalidCommands = [
    "command-session-files-list-missing-session-id.json",
    "command-session-files-list-unknown-payload-key.json",
    "command-session-files-read-absolute-path.json",
    "command-session-files-read-parent-path.json",
    "command-session-files-read-double-separator.json",
    "command-session-files-read-trailing-separator.json",
    "command-session-files-read-control-character.json",
    "command-session-files-read-bad-purpose.json",
    "command-session-files-read-negative-offset.json",
    "command-session-files-read-nondigit-offset.json",
    "command-session-git-diff-git-path.json",
    "command-session-git-status-expected-version.json",
  ] as const;

  it.each(invalidCommands)("%s 被拒绝", (file) => {
    const decoded = decodeSessionFilesCommandBody(bodyOf(file));
    expect(decoded.ok).toBe(false);
  });

  it("拒绝原因分类能区分路径/枚举/十进制/未知键", () => {
    const absolute = decodeSessionFilesCommandBody(bodyOf("command-session-files-read-absolute-path.json"));
    expect(absolute.ok === false ? absolute.reason : null).toBe("invalid_path");
    const purpose = decodeSessionFilesCommandBody(bodyOf("command-session-files-read-bad-purpose.json"));
    expect(purpose.ok === false ? purpose.reason : null).toBe("invalid_enum");
    const offset = decodeSessionFilesCommandBody(bodyOf("command-session-files-read-nondigit-offset.json"));
    expect(offset.ok === false ? offset.reason : null).toBe("invalid_decimal");
    const unknown = decodeSessionFilesCommandBody(bodyOf("command-session-files-list-unknown-payload-key.json"));
    expect(unknown.ok === false ? unknown.reason : null).toBe("unknown_key");
  });
});

describe("session-files：非法结果显式拒绝", () => {
  it("list 结果含未知键被拒绝", () => {
    const body = bodyOf("command-result-session-files-list-unknown-key.json") as { result: unknown };
    const decoded = decodeSessionFilesResult("session.files.list", body.result);
    expect(decoded.ok).toBe(false);
  });

  it("read 结果缺 version 被拒绝", () => {
    const body = bodyOf("command-result-session-files-read-missing-version.json") as { result: unknown };
    const decoded = decodeSessionFilesResult("session.files.read", body.result);
    expect(decoded.ok).toBe(false);
    expect(decoded.ok === false ? decoded.reason : null).toBe("missing_key");
  });

  it("合法结果多一个未知键同样被拒绝（不静默降级）", () => {
    const body = bodyOf("command-result-session-git-status-completed.json") as { result: Record<string, unknown> };
    const decoded = decodeSessionFilesResult("session.git.status", { ...body.result, futureField: 1 });
    expect(decoded.ok).toBe(false);
  });

  it("diffLines 与 unavailable 同时出现被拒绝", () => {
    const body = bodyOf("command-result-session-git-diff-completed.json") as { result: Record<string, unknown> };
    const decoded = decodeSessionFilesResult("session.git.diff", {
      ...body.result,
      unavailable: "too_large",
    });
    expect(decoded.ok).toBe(false);
    expect(decoded.ok === false ? decoded.reason : null).toBe("mutually_exclusive");
  });

  it("非法枚举与非法路径不会被当成缺省值", () => {
    const list = bodyOf("command-result-session-files-list-completed.json") as { result: Record<string, unknown> };
    expect(decodeSessionFilesResult("session.files.list", { ...list.result, gitState: "clean" }).ok).toBe(false);
    expect(
      decodeSessionFilesResult("session.files.list", {
        ...list.result,
        entries: [{ path: "../escape", name: "escape", kind: "file", openable: true }],
      }).ok,
    ).toBe(false);
  });
});

describe("Git line counts", () => {
  it("preserves paired counts from the shared fixture and rejects partial/invalid counts", () => {
    const body = bodyOf("command-result-session-git-status-completed.json") as {result: {entries: Record<string,unknown>[]}};
    const result = body.result;
    const decoded = decodeSessionFilesResult("session.git.status", result);
    expect(decoded.ok).toBe(true);
    if (decoded.ok) expect(decoded.value.entries[0]).toMatchObject({addedLines:12,deletedLines:3});
    const first = result.entries[0];
    for (const patch of [{deletedLines:undefined},{addedLines:-1},{addedLines:0.5},{addedLines:4294967296},{addedLines:null},{binary:true},{conflicted:true}]) {
      expect(decodeSessionFilesResult("session.git.status", {...result,entries:[{...first,...patch}]}).ok).toBe(false);
    }
    expect(decodeSessionFilesResult("session.git.status", {...result,entries:[{...first,addedLines:0,deletedLines:0}]}).ok).toBe(true);
  });
});

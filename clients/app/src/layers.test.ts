/**
 * R10 的结构化判定：分层与**单向**依赖。
 *
 * 判据（`specs/pwa-web-client/spec.md` 的「客户端分层与依赖方向」）：
 * 页面与组件、feature 控制器与状态机、应用服务、同步客户端与协议类型、平台端口、平台能力
 * 单向依赖；页面不得直接读写 WebSocket/IndexedDB/密钥存储/本地数据库。
 *
 * 做法：遍历 `src/` 下的全部 `.ts`/`.tsx`，解析每条相对 import，按 `src/layers.ts` 的
 * 序号核对方向。这比人工审查更硬——新增一处反向 import 会直接让本用例失败。
 *
 * 同时断言两条**平台能力禁令**：`domain`/`protocol` 层不得 import 平台 API，
 * 任何层都不得从 `src/platform/` 反向 import 更外层的模块。
 */

import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, posix, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import {
  LAYER_ORDER,
  ORDERED_LAYERS,
  PLATFORM_LAYER,
  isDependencyAllowed,
  layerOfSourcePath,
  type LayerName,
} from "./layers";

// 用 `resolve` 消掉 `..`：否则 `relative(APP_ROOT, file)` 会得到 `../app/src/…` 这类
// 仍含回退层级的相对形式，层级识别随之失效。
const SRC_ROOT = dirname(fileURLToPath(import.meta.url));
const APP_ROOT = resolve(SRC_ROOT, "..");
const REPO_APP_ROOT = resolve(APP_ROOT, "src");

/** 递归收集 `src/` 下的全部 TS 源文件。 */
function collectSourceFiles(dir: string): string[] {
  const result: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      result.push(...collectSourceFiles(full));
      continue;
    }
    if (entry.endsWith(".ts") || entry.endsWith(".tsx")) result.push(full);
  }
  return result;
}

/** 取出一个源文件里的全部静态 import/export-from 说明符。 */
function importedSpecifiers(source: string): string[] {
  const specifiers: string[] = [];
  const pattern = /(?:^|\n)\s*(?:import|export)\b[^;]*?from\s+["']([^"']+)["']/g;
  for (const match of source.matchAll(pattern)) {
    const specifier = match[1];
    if (specifier !== undefined) specifiers.push(specifier);
  }
  return specifiers;
}

/** 把一个相对 import 说明符解析成 `src/` 下的相对路径；非本工程内的返回 `null`。 */
function localRelativeTarget(fromFile: string, specifier: string): string | null {
  if (!specifier.startsWith(".")) return null;
  // `relative` 在 Windows 上给回退斜杠，而下面的 `posix.*` 只认正斜杠——
  // 不先归一化的话 `posix.dirname("src\\protocol\\common.ts")` 返回 `"."`，层级判定整体失效。
  const fromRelative = relative(APP_ROOT, fromFile).replaceAll("\\", "/");
  const target = posix.normalize(posix.join(posix.dirname(fromRelative), specifier));
  if (!target.startsWith("src/")) return null;
  return target;
}

describe("R10 客户端分层与单向依赖", () => {
  const files = collectSourceFiles(REPO_APP_ROOT);

  it("src/ 下确实存在待判定的源文件", () => {
    expect(files.length).toBeGreaterThan(0);
  });

  it("没有任何跨层反向 import", () => {
    const violations: string[] = [];

    for (const file of files) {
      const from = layerOfSourcePath(relative(APP_ROOT, file));
      // `src/layers.ts` 本身不属于任何一层，跳过方向判定。
      if (from === null || from === PLATFORM_LAYER) continue;

      const source = readFileSync(file, "utf8");
      for (const specifier of importedSpecifiers(source)) {
        const target = localRelativeTarget(file, specifier);
        if (target === null) continue;
        const to = layerOfSourcePath(target);
        if (to === null) continue;
        if (to === PLATFORM_LAYER) {
          violations.push(
            `${relative(APP_ROOT, file)}（${from}）import 了平台层 ${specifier}——平台能力只能由更外层装配`,
          );
          continue;
        }
        if (!isDependencyAllowed(from, to)) {
          violations.push(
            `${relative(APP_ROOT, file)}（${from} → ${to}）违反单向依赖：${specifier}`,
          );
        }
      }
    }

    expect(violations).toEqual([]);
  });

  it("序号表覆盖七层且单调", () => {
    expect(ORDERED_LAYERS).toEqual([
      "app",
      "components",
      "features",
      "state",
      "sync-client",
      "domain",
      "protocol",
    ]);
    const values = ORDERED_LAYERS.map((layer) => LAYER_ORDER[layer]);
    expect(values).toEqual([...values].sort((a, b) => a - b));
  });

  it("domain 与 protocol 不得依赖更外层的层级", () => {
    const allowedForDomain: readonly LayerName[] = ["domain", "protocol"];
    const allowedForProtocol: readonly LayerName[] = ["protocol"];
    expect(allowedForDomain.every((layer) => isDependencyAllowed("domain", layer))).toBe(true);
    expect(allowedForProtocol.every((layer) => isDependencyAllowed("protocol", layer))).toBe(true);
    expect(isDependencyAllowed("domain", "state")).toBe(false);
    expect(isDependencyAllowed("protocol", "domain")).toBe(false);
  });

  it("页面层不得直接 import 平台 API 模块（WebSocket/存储/密钥）", () => {
    const platformApis = ["ws", "socket.io-client", "idb", "localforage", "expo-secure-store"];
    const violations: string[] = [];
    for (const file of files) {
      const from = layerOfSourcePath(relative(APP_ROOT, file));
      if (from !== "app" && from !== "components") continue;
      const source = readFileSync(file, "utf8");
      for (const specifier of importedSpecifiers(source)) {
        if (platformApis.includes(specifier)) {
          violations.push(`${relative(APP_ROOT, file)}（${from}）直接 import 了平台 API ${specifier}`);
        }
      }
    }
    expect(violations).toEqual([]);
  });
});

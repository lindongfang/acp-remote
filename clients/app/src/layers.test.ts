/**
 * R10 的结构化判定：分层与**单向**依赖。
 *
 * 判据（`specs/pwa-web-client/spec.md` 的「客户端分层与依赖方向」）：
 * 页面与组件、feature 控制器与状态机、应用服务、同步客户端与协议类型、平台端口、平台能力
 * 单向依赖；页面不得直接读写 WebSocket/IndexedDB/密钥存储/本地数据库。
 *
 * 做法：遍历 `app/`（Expo Router 页面）与 `src/` 两个根下的全部 `.ts`/`.tsx`，解析每条
 * 相对 import，按 `src/layers.ts` 的序号核对方向。这比人工审查更硬——新增一处反向 import
 * 会直接让本用例失败。
 *
 * 覆盖面（三条容易被静默绕过的路径都必须判定，见 `review-wp5a-r1` 的 F2/F3）：
 *
 * - **裸 barrel / 目录目标**（`import x from "../domain"`）：解析结果没有文件名，也必须取到层级；
 * - **无 `from` 子句的副作用导入**（`import "../domain/x"`）与 `await import("…")`；
 * - **页面层**：`app/**` 必须进入扫描集合，否则页面层的方向与平台禁令是空断言。
 *
 * 同时断言两条**平台能力禁令**：`domain`/`protocol` 层不得 import 平台 API，
 * 任何层都不得从 `src/platform/` 反向 import 更外层的模块。
 */

import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
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
/** `clients/app/src`（七层中的六层 + 横切的 `platform/`）。 */
const SRC_LAYER_ROOT = resolve(APP_ROOT, "src");
/** `clients/app/app`（Expo Router 页面，R10 的「页面」）。 */
const APP_LAYER_ROOT = resolve(APP_ROOT, "app");
/** 页面与组件两个「最外层」，平台能力只能由它们之上的装配点接入。 */
const SCAN_ROOTS = [APP_LAYER_ROOT, SRC_LAYER_ROOT] as const;

/**
 * 页面/组件层**禁止直接 import** 的平台通道。
 *
 * 覆盖 `docs/FRONTEND_DESIGN.md` §3 的 `platform/` 计划面（WebSocket / Secure Storage /
 * Local Cache / IndexedDB / SQLite）与 plan WP5b 的 `secure-storage`/`local-cache`/`lifecycle`
 * 端口。`node:*` 另在 `isPlatformApiSpecifier` 里单独拦截。
 *
 * **新增平台通道（新包或新端口目录名）必须同步此表**——否则页面直接读写它的写法会静默通过。
 */
const PLATFORM_API_SPECIFIERS: Record<string, true> = {
  // 传输：WebSocket
  ws: true,
  "socket.io-client": true,
  // IndexedDB / 本地缓存封装
  idb: true,
  localforage: true,
  indexeddb: true,
  "local-cache": true,
  "local-storage": true,
  // 密钥存储 / 本地数据库
  "expo-secure-store": true,
  "secure-store": true,
  "secure-storage": true,
  "expo-sqlite": true,
  sqlite: true,
  sqlite3: true,
};

/** 递归收集一个目录下的全部 TS 源文件。 */
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

/**
 * 取出一个源文件里的全部 import/export 说明符，覆盖三种写法：
 *
 * 1. `import … from "x"` / `export … from "x"`（含跨行）；
 * 2. 无 `from` 的副作用导入 `import "x"`；
 * 3. 动态导入 `import("x")`（含 `await import("x")`）。
 *
 * 只补 `from` 子句会漏掉后两类，使反向依赖可以靠改写写法绕过方向判定（F2）。
 */
function importedSpecifiers(source: string): string[] {
  const specifiers: string[] = [];
  const patterns = [
    // `[^;]` 不跨语句，避免 `export default function …` 一路吞到后面某个 `from`。
    /(?:^|\s)(?:import|export)\b[^;]*?\bfrom\s*["']([^"']+)["']/g,
    /(?:^|\s)import\s*["']([^"']+)["']/g,
    /(?:^|\s)import\s*\(\s*["']([^"']+)["']\s*\)/g,
  ];
  for (const pattern of patterns) {
    for (const match of source.matchAll(pattern)) {
      const specifier = match[1];
      if (specifier !== undefined) specifiers.push(specifier);
    }
  }
  return specifiers;
}

/** 一个源文件在 `clients/app/` 下的相对路径，统一正斜杠（Windows 下 `relative` 给反斜杠）。 */
function projectRelative(file: string): string {
  return relative(APP_ROOT, file).replaceAll("\\", "/");
}

/**
 * 把一个 import 说明符解析成工程内相对路径（`src/…` 或 `app/…`）；工程外的返回 `null`。
 *
 * - `@/*` 按 `tsconfig.json` 的 `paths` 别名解析到 `src/*`；
 * - 相对说明符里的反斜杠先归一化——否则 `posix.join` 会把 `..\\domain` 当作普通文件名，
 *   反向依赖可以靠反斜杠写法绕过（同 F2 的判别力要求）。
 */
function localRelativeTarget(fromFile: string, specifier: string): string | null {
  if (specifier.startsWith("@/")) {
    return posix.normalize(posix.join("src", specifier.slice("@/".length)));
  }
  if (!specifier.startsWith(".")) return null;
  const fromRelative = projectRelative(fromFile);
  const normalizedSpecifier = specifier.replaceAll("\\", "/");
  return posix.normalize(posix.join(posix.dirname(fromRelative), normalizedSpecifier));
}

/** 说明符是否命中平台通道（含 `node:` 前缀、`pkg/subpath` 与 `…/端口目录名` 三种写法）。 */
function isPlatformApiSpecifier(specifier: string): boolean {
  if (specifier.startsWith("node:")) return true;
  const segments = specifier.split("/").filter((segment) => segment.length > 0 && !segment.startsWith("@"));
  return segments.some((segment) => PLATFORM_API_SPECIFIERS[segment] === true);
}

describe("R10 客户端分层与单向依赖", () => {
  const files = SCAN_ROOTS.flatMap((root) => collectSourceFiles(root));

  it("两个扫描根都存在，且扫描集合覆盖页面层与内层", () => {
    expect(existsSync(APP_LAYER_ROOT)).toBe(true);
    expect(existsSync(SRC_LAYER_ROOT)).toBe(true);
    expect(files.length).toBeGreaterThan(0);

    const layerByFile = new Map(files.map((file) => [file, layerOfSourcePath(projectRelative(file))]));
    const appFiles = files.filter((file) => layerByFile.get(file) === "app");
    expect(appFiles.length).toBeGreaterThan(0);
    for (const layer of ["components", "features", "state", "sync-client", "domain", "protocol"] as const) {
      expect(
        files.some((file) => layerByFile.get(file) === layer),
        `扫描集合缺少 ${layer} 层`,
      ).toBe(true);
    }
  });

  it("没有任何跨层反向 import", () => {
    const violations: string[] = [];

    for (const file of files) {
      const from = layerOfSourcePath(projectRelative(file));
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
            `${projectRelative(file)}（${from}）import 了平台层 ${specifier}——平台能力只能由更外层装配`,
          );
          continue;
        }
        if (!isDependencyAllowed(from, to)) {
          violations.push(
            `${projectRelative(file)}（${from} → ${to}）违反单向依赖：${specifier}`,
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

  it("页面层不得直接 import 平台 API 模块（WebSocket/存储/密钥/数据库）", () => {
    const violations: string[] = [];
    for (const file of files) {
      const from = layerOfSourcePath(projectRelative(file));
      if (from !== "app" && from !== "components") continue;
      const source = readFileSync(file, "utf8");
      for (const specifier of importedSpecifiers(source)) {
        if (isPlatformApiSpecifier(specifier)) {
          violations.push(`${projectRelative(file)}（${from}）直接 import 了平台 API ${specifier}`);
        }
      }
    }
    expect(violations).toEqual([]);
  });
});

describe("R10 分层判定的判别力（F2/F3）", () => {
  it("importedSpecifiers 覆盖 from 子句、无 from 的副作用导入与动态导入", () => {
    const source = [
      'import type { A } from "../domain/a";',
      'import "../domain/side-effect";',
      'const m = await import("../state");',
      'export { b } from "../protocol/b";',
      'import "idb";',
    ].join("\n");

    expect(new Set(importedSpecifiers(source))).toEqual(
      new Set(["../domain/a", "../protocol/b", "../domain/side-effect", "idb", "../state"]),
    );
  });

  it("裸 barrel / 目录目标能解析出层级（不被静默跳过）", () => {
    // 目录目标没有文件名，旧实现因「路径无斜杠」返回 null 并 continue。
    expect(layerOfSourcePath("src/domain")).toBe("domain");
    expect(layerOfSourcePath("src/platform")).toBe(PLATFORM_LAYER);
    expect(layerOfSourcePath("app/session/[id].tsx")).toBe("app");
    // `src/layers.ts` 这类非层文件仍不参与判定。
    expect(layerOfSourcePath("src/layers.ts")).toBeNull();
    expect(layerOfSourcePath("src/layers.test.ts")).toBeNull();

    expect(localRelativeTarget(resolve(SRC_LAYER_ROOT, "features", "index.ts"), "../domain")).toBe("src/domain");
    expect(localRelativeTarget(resolve(SRC_LAYER_ROOT, "sync-client", "index.ts"), "../protocol")).toBe("src/protocol");
    expect(localRelativeTarget(resolve(APP_LAYER_ROOT, "session", "[id].tsx"), "../../src/platform/x")).toBe(
      "src/platform/x",
    );
    expect(localRelativeTarget(resolve(SRC_LAYER_ROOT, "domain", "a.ts"), "@/protocol/common")).toBe(
      "src/protocol/common",
    );
    // 反斜杠说明符必须与正斜杠等价，否则可绕过方向判定。
    expect(localRelativeTarget(resolve(SRC_LAYER_ROOT, "protocol", "common.ts"), "..\\domain")).toBe("src/domain");
  });

  it("仓库既有的裸 barrel 形态（features→domain、sync-client→protocol）被解析为层级且方向合规", () => {
    const featuresIndex = resolve(SRC_LAYER_ROOT, "features", "index.ts");
    const syncClientIndex = resolve(SRC_LAYER_ROOT, "sync-client", "index.ts");
    expect(localRelativeTarget(featuresIndex, "../domain")).toBe("src/domain");
    expect(layerOfSourcePath("src/domain")).toBe("domain");
    expect(isDependencyAllowed("features", "domain")).toBe(true);

    expect(localRelativeTarget(syncClientIndex, "../protocol")).toBe("src/protocol");
    expect(layerOfSourcePath("src/protocol")).toBe("protocol");
    expect(isDependencyAllowed("sync-client", "protocol")).toBe(true);
  });

  it("平台 API 禁令覆盖 src/platform/ 计划通道与 node:*（新增通道须同步此表）", () => {
    const forbidden = [
      "ws",
      "socket.io-client",
      "idb",
      "localforage",
      "expo-secure-store",
      "expo-sqlite",
      "node:fs",
      "node:crypto",
      "node:path",
      "expo-secure-store/build/index",
      "@scope/secure-store",
      "foo/local-cache/build/x",
    ];
    for (const specifier of forbidden) {
      expect(isPlatformApiSpecifier(specifier), `未拦截：${specifier}`).toBe(true);
    }

    const allowed = ["react", "react-native", "expo-router", "expo-constants", "vitest", "../domain", "@/protocol"];
    for (const specifier of allowed) {
      expect(isPlatformApiSpecifier(specifier), `误拦：${specifier}`).toBe(false);
    }
  });
});

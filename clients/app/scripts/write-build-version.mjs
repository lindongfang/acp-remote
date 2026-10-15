#!/usr/bin/env node
/**
 * 把「这是哪一次构建」写进 `dist/build-version.json`（`sync-ui-hosting` 变更 WP2，
 * `design.md` D3 的前端侧）。
 *
 * 谁读它：Daemon 的嵌入资源表（`crates/server/src/transport/net/ui/mod.rs` 的 `build_version()`）
 * 从**同一份嵌入资源**里读这个文件，`daemon.status` 再把版本报出来——因此版本与托管的资源必然出自
 * 同一次构建。它回答的是「这个 Daemon 携带的前端是哪个版本」，用于「刷新后仍拿到旧 JS 去连新 Daemon」
 * 这类诊断；Daemon 自己算产物哈希只能证明「资源没被改」，回答不了这个问题。
 *
 * 为什么是构建后钩子而不是构建期常量/环境变量：`expo export` 产出的是静态文件，只有写进 `dist/`
 * 才能被 Daemon 的嵌入资源表读到；环境变量在 Daemon 启动时早已不存在。
 *
 * **本脚本绝不让门禁变红**：`clients/app` 的 `npm run check`（含 `expo export`）是 CI 必需检查
 * 「前端检查（clients/app）」的一步，钩子失败会把那条门禁弄红。因此：
 * - 只用 Node 内建模块（`node:child_process`/`node:fs`/`node:path`/`node:url`），**不新增任何 npm 依赖**；
 * - 不读环境变量、不用 POSIX 专有语法（CI 跑 Linux、开发机是 Windows），全部路径经 `node:path` 拼；
 * - git 元数据不可用（浅克隆、无 tag、无 `.git`、甚至没装 git、或 git 因 dubious ownership 拒绝）时
 *   逐级退化：`git describe` → 短 sha → 时间戳，**任何**异常都以退出码 0 结束。
 *
 * 版本缺失在 Daemon 侧是**显式**的缺失语义（`build_version()` 返回 `None`，不编造版本），因此「写不出来」
 * 不会伪装成某一次构建；这里只把原因打到 stderr 供人排查。
 *
 * 产出形状（Daemon 只读 `version`，其余字段供人排查，允许增删）：
 *   { "version": "0.0.0+v0.1.0-3-gabc1234-dirty", "buildIdentifier": "v0.1.0-3-gabc1234-dirty",
 *     "source": "git-describe", "packageVersion": "0.0.0", "builtAt": "2026-10-15T12:34:56.789Z" }
 *
 * 用法（cwd 任意）：node scripts/write-build-version.mjs
 */

import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const APP_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const DIST_DIR = join(APP_ROOT, "dist");
const TARGET = join(DIST_DIR, "build-version.json");

/** package.json 的版本；读不到就用 `0.0.0`（版本串的骨架，不声称它是真实发布版本）。 */
function packageVersion() {
  try {
    const manifest = JSON.parse(readFileSync(join(APP_ROOT, "package.json"), "utf8"));
    const version = typeof manifest.version === "string" ? manifest.version.trim() : "";
    return version === "" ? "0.0.0" : version;
  } catch {
    return "0.0.0";
  }
}

/** 跑一条 git 命令；任何失败都返回 `null`，由调用方按顺序降级（不抛出、不阻塞门禁）。 */
function git(args) {
  try {
    const output = execFileSync("git", args, {
      cwd: APP_ROOT,
      encoding: "utf8",
      // stderr 丢弃：浅克隆/无 tag 时 git 会打提示，那不是错误，也不该出现在 CI 日志里冒充失败。
      stdio: ["ignore", "pipe", "ignore"],
    });
    const trimmed = output.trim();
    return trimmed === "" ? null : trimmed;
  } catch {
    return null;
  }
}

/** 构建标识与它的来源：git 描述 → 短 sha → 时间戳。 */
function buildIdentifier() {
  // `--always` 让「仓库里一个 tag 都没有」或浅克隆时也退化成本次提交的短 sha（因此这一层几乎总能成功）；
  // `--dirty` 把「构建时工作区不干净」记进版本串——那正是「同一个 sha 却可能是不同产物」的唯一线索。
  const described = git(["describe", "--tags", "--always", "--dirty"]);
  if (described !== null) {
    return { identifier: described, source: "git-describe" };
  }
  const revision = git(["rev-parse", "--short", "HEAD"]);
  if (revision !== null) {
    return { identifier: revision, source: "git-rev" };
  }
  return { identifier: `t${timestamp()}`, source: "timestamp" };
}

/** `20261015T123456Z` 形态的 UTC 时间戳（只含版本串允许的字符）。 */
function timestamp() {
  return new Date().toISOString().replace(/[-:]/g, "").replace(/\.\d+Z$/, "Z");
}

/** 把标识收敛到 `[0-9A-Za-z.+_-]` 之外的字符换成 `-`：tag 名可以含 `/`、`@` 等，而版本串会进 JSON、
 * 日志与命令行。`+` 保留：它是 semver 的构建元数据分隔符（`0.0.0+<短 sha>`）。 */
function sanitize(value) {
  return value.replace(/[^0-9A-Za-z.+-]+/g, "-");
}

function main() {
  const version = packageVersion();
  const { identifier, source } = buildIdentifier();
  const payload = {
    version: sanitize(`${version}+${identifier}`),
    buildIdentifier: identifier,
    source,
    packageVersion: version,
    builtAt: new Date().toISOString(),
  };
  // `expo export` 已经产出 dist/；这里补一次是为了脚本能被单独运行（例如只改版本文件后重跑嵌入）。
  mkdirSync(DIST_DIR, { recursive: true });
  writeFileSync(TARGET, `${JSON.stringify(payload, null, 2)}\n`, "utf8");
  console.log(`build-version.json：${payload.version}（来源 ${payload.source}）`);
}

try {
  main();
} catch (error) {
  // 到这里只可能是写文件失败之类的环境问题（git 失败已在 `git()` 里吸收）。门禁不能被它弄红：Daemon 侧
  // 会把「没有版本文件」如实报成缺失，而不是伪造一个版本。
  console.warn(`写 build-version.json 失败（不影响前端门禁，Daemon 侧将报版本缺失）：${error?.message ?? error}`);
}

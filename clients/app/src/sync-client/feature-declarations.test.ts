/**
 * feature 声明：路径 / 逐层浏览 / 结构化差异三条新增 Sync feature（§5.2 协商是交集）。
 *
 * ## 判别力
 *
 * - **删掉声明就变红**：协商取客户端与服务端 feature 集的交集，客户端不广告 = 永不协商 =
 *   服务端不下发 `workspaces[].path` 与 `file.changed` 的差异行、`workspace.browse` 回
 *   `protocol.feature_required`。因此每条都断言「这个具体 ID 出现在 `SUPPORTED_FEATURES` 里」，
 *   而不是把整个数组抄成期望值——后者会在无关增删时误报，却对「换了个位置的缺失」不敏感。
 * - **与机器登记逐字一致**：ID 反查 `compatibility/features/v1/features.json`，拼错或未登记
 *   的名字在那里找不到 → 断言变红。
 * - **非必需**：三条在登记表里都是 `required: false`，因此不得进入 `REQUIRED_FEATURES`
 *   （进了会让未广告它们的服务端被握手判成 `version_incompatible` 而阻断重连）。
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { repoRoot } from "../protocol/paths";
import { REQUIRED_FEATURES, SUPPORTED_FEATURES } from "./connection";

/** 机器登记表里的一条 sync feature。 */
interface RegistryFeature {
  readonly id: string;
  readonly delivery: string;
  readonly required: boolean;
}

/** 读 `compatibility/features/v1/features.json` 的 sync 段（与 `history-paging.test.ts` 同一来源）。 */
function syncRegistryFeatures(): readonly RegistryFeature[] {
  const registry = JSON.parse(
    readFileSync(join(repoRoot, "compatibility", "features", "v1", "features.json"), "utf8"),
  ) as { protocols: { sync: { features: RegistryFeature[] } } };
  return registry.protocols.sync.features;
}

/**
 * 三条新增 feature 的 ID：目录路径、逐层浏览、结构化差异行。
 *
 * 它们是**客户端广告、服务端可选**的能力，客户端据此在协商命中后才请求/呈现对应数据。
 */
const NEW_SYNC_FEATURES = [
  "core.workspace-browse.v1",
  "core.file-diff.v1",
] as const;

describe("feature 声明：路径 / 逐层浏览 / 结构化差异三条新增 Sync feature", () => {
  it.each(NEW_SYNC_FEATURES)("%s 已在客户端广告且与机器登记一致", (feature) => {
    const entry = syncRegistryFeatures().find((item) => item.id === feature);
    // 反例：ID 拼错或未登记 → undefined → 断言变红。
    expect(entry).toBeDefined();
    expect(entry?.delivery).toBe("mvp");
    // 反例：从 SUPPORTED_FEATURES 里删掉这一条 → 不广告 → 永不协商 → 断言变红。
    expect(SUPPORTED_FEATURES).toContain(feature);
    // 非必需：进了必需列表会让未广告它的服务端把连接判成 version_incompatible。
    expect(entry?.required).toBe(false);
    expect(REQUIRED_FEATURES).not.toContain(feature);
  });
});

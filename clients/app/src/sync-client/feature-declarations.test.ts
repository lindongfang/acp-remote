/**
 * feature 声明：逐层浏览 / 结构化差异两条新增 Sync feature（§5.2 协商是交集），
 * 以及已撤销的 `core.workspace-path.v1`。
 *
 * ## 判别力
 *
 * - **删掉声明就变红**：协商取客户端与服务端 feature 集的交集，客户端不广告 = 永不协商 =
 *   `file.changed` 的差异行不下发、`workspace.browse` 回 `protocol.feature_required`。因此每条都
 *   断言「这个具体 ID 出现在 `SUPPORTED_FEATURES` 里」，而不是把整个数组抄成期望值——后者会在
 *   无关增删时误报，却对「换了个位置的缺失」不敏感。
 * - **与机器登记逐字一致**：ID 反查 `compatibility/features/v1/features.json`，拼错或未登记
 *   的名字在那里找不到 → 断言变红。
 * - **非必需**：两条在登记表里都是 `required: false`，因此不得进入 `REQUIRED_FEATURES`
 *   （进了会让未广告它们的服务端被握手判成 `version_incompatible` 而阻断重连）。
 * - **撤销的 feature 不再协商**：`core.workspace-path.v1` 已从机器登记表删除，客户端也不得再广告；
 *   两条负向断言分别钉住这两个方向。
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { repoRoot } from "../protocol/paths";
import { REQUIRED_FEATURES, SUPPORTED_FEATURES } from "./connection";

/** 已撤销的目录路径 feature：`path` 现在始终随 `workspaces` 条目下发。 */
const REVOKED_WORKSPACE_PATH_FEATURE = "core.workspace-path.v1";

/** 三态会话摘要合同：登记表 `required: true`，客户端必须广告并列入必需集合。 */
const SESSION_ACTIVITY_FEATURE = "core.session-activity.v1";

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
 * 两条新增 feature 的 ID：逐层浏览、结构化差异行。
 *
 * 它们是**客户端广告、服务端可选**的能力，客户端据此在协商命中后才请求/呈现对应数据。
 */
const NEW_SYNC_FEATURES = [
  "core.workspace-browse.v1",
  "core.file-diff.v1",
  "core.session-files.v1",
  "core.session-git-stats.v1",
    "core.session-git-compare.v1",
] as const;

describe("feature 声明：逐层浏览 / 结构化差异两条新增 Sync feature", () => {
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

  it(`${SESSION_ACTIVITY_FEATURE} 已在客户端广告且是必需 feature`, () => {
    const entry = syncRegistryFeatures().find((item) => item.id === SESSION_ACTIVITY_FEATURE);
    expect(entry).toBeDefined();
    expect(entry?.delivery).toBe("mvp");
    expect(entry?.required).toBe(true);
    expect(SUPPORTED_FEATURES).toContain(SESSION_ACTIVITY_FEATURE);
    expect(REQUIRED_FEATURES).toContain(SESSION_ACTIVITY_FEATURE);
  });

  it(`${REVOKED_WORKSPACE_PATH_FEATURE} 已从登记表撤销，客户端也不再广告`, () => {
    // 反例：登记表把它加回来而客户端没跟上，或客户端继续广告一个不存在的 ID → 两条断言之一变红。
    expect(syncRegistryFeatures().some((item) => item.id === REVOKED_WORKSPACE_PATH_FEATURE)).toBe(false);
    expect(SUPPORTED_FEATURES).not.toContain(REVOKED_WORKSPACE_PATH_FEATURE);
    expect(REQUIRED_FEATURES).not.toContain(REVOKED_WORKSPACE_PATH_FEATURE);
  });
});

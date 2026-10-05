/**
 * `clients/app` 的七层结构与**单向**依赖（`AGENTS.md` §5、`docs/FRONTEND_DESIGN.md` §3、`design.md` D8）。
 *
 * 这张表是分层约束的**单一**机器可读定义：测试 `src/layers.test.ts` 遍历 `src/` 下的
 * 全部 `.ts`/`.tsx` 文件，按此表核对每一处 import 的目标层级不小于发起层级的序号
 * （序号越小越靠外层），从而在**结构上**可核对 R10 的「客户端分层与依赖方向」。
 *
 * ```text
 *   1  app/            页面（Expo Router 路由）
 *   2  src/components/ 共享展示组件
 *   3  src/features/   feature 控制器
 *   4  src/state/      连接与命令状态机
 *   5  src/sync-client/  同步客户端（连接、认证、重放、ACK）
 *   6  src/domain/     客户端领域类型与视图模型
 *   7  src/protocol/   Sync DTO（消费仓库根 schemas/ 与 fixtures/）
 *      src/platform/   平台端口实现（WebSocket / Secure Storage / Local Cache / Lifecycle）
 * ```
 *
 * 平台端口是**横切替换面**：它由更具平台性的实现装配到上层，因此没有在链条中占一个序号，
 * 而是单独列为 `PLATFORM_LAYER`，并**禁止**被 `domain`/`protocol` 依赖（端口定义与接线由
 * WP5b/WP6 承担，WP5a 只固定方向约束）。
 */

/** 分层序号：数字越小越靠外层（越靠近页面），依赖只能指向更大的数字。 */
export const LAYER_ORDER = {
  app: 1,
  components: 2,
  features: 3,
  state: 4,
  "sync-client": 5,
  domain: 6,
  protocol: 7,
} as const;

/** 分层名（`LAYER_ORDER` 的键）。 */
export type LayerName = keyof typeof LAYER_ORDER;

/**
 * 平台端口层。
 *
 * 它在链条之外：实现层是平台专有的（Web 用 IndexedDB/WebCrypto，未来原生用 Keychain/SQLite），
 * 因此由组合根装配，而不是被 `domain`/`protocol` 反向依赖。
 */
export const PLATFORM_LAYER = "platform";

/** 参与序号核对的分层名清单。 */
export const ORDERED_LAYERS: readonly LayerName[] = [
  "app",
  "components",
  "features",
  "state",
  "sync-client",
  "domain",
  "protocol",
];

/**
 * 把 `src/` 相对路径映射成层级名。
 *
 * `src/<layer>/…` 取第一段；其余（如 `src/layers.ts` 本身）返回 `null`，
 * 表示该文件不属于任何一层，测试对它不做方向判定。
 */
export function layerOfSourcePath(
  relativePath: string,
): LayerName | typeof PLATFORM_LAYER | null {
  const normalized = relativePath.replaceAll("\\", "/");
  const prefix = normalized.startsWith("src/") ? normalized.slice("src/".length) : normalized;
  const slash = prefix.indexOf("/");
  if (slash === -1) return null;
  const head = prefix.slice(0, slash);
  if (head === PLATFORM_LAYER) return PLATFORM_LAYER;
  if (head in LAYER_ORDER) return head as LayerName;
  return null;
}

/**
 * 建立一条依赖边是否合法。
 *
 * 规则：`from` 可以依赖序号不小于自身的层级。同级依赖必须允许（同一层内多个模块互相引用，
 * 例如 `state` 的两个文件），否则分层会退化成每个目录只能有一个文件。
 */
export function isDependencyAllowed(
  from: LayerName,
  to: LayerName,
): boolean {
  return LAYER_ORDER[to] >= LAYER_ORDER[from];
}

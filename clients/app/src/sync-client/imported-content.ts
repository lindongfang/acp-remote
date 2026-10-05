/**
 * imported 资源的落盘分流（R20「imported 资源不落盘且离线只显示元数据」）。
 *
 * ## 判定的唯一依据是会话来源
 *
 * `SessionSummary.origin.kind` 决定内容能否进入持久存储：`local` 走本节点自有的摘要缓存，
 * `remote`（imported）则一律 no-content-cache。这条分流是**纯函数**，不依赖任何存储实现，
 * 因此「来源为其它节点的正文不进入任何持久浏览器存储」是可单测的判定，而不是运行期约定。
 *
 * ## 为什么必须在这里分流而不是在写入处
 *
 * 平台缓存端口的 `CacheEntry` 已经是判别联合（`local` 只有 `summary`、`imported` 只有
 * `metadata`），但那约束的是**形状**；本模块约束的是**哪个来源的什么内容**可以被请求写入。
 * 两层都必要：只有形状约束时，一次错误的 `putLocalSummary` 调用仍会把 imported 正文写进去。
 */

/** 可持久化的内容类别。 */
export type ContentKind =
  /** 会话摘要（标题/摘要文本）。 */
  | "summary"
  /** 用户输入与 Agent 回复正文。 */
  | "prompt"
  /** 工具调用内容与结果。 */
  | "tool_content"
  /** 差异（diff）。 */
  | "diff"
  /** 终端输出。 */
  | "terminal_output"
  /** 附件。 */
  | "attachment"
  /** ACP 原文（`acp.rawJson`）。 */
  | "acp_raw";

/** 会话来源。 */
export type SessionOrigin = { readonly kind: "local" } | { readonly kind: "remote"; readonly ownerNodeId: string };

/** 分流结果。 */
export type ContentRoute =
  /** 允许进入本节点固定容量的摘要缓存。 */
  | { readonly destination: "local_summary_cache" }
  /** 只允许留在内存（刷新即失效，重连后必须重新获取）。 */
  | { readonly destination: "memory_only" };

/**
 * 本节点自有资源里可持久化的类别。
 *
 * 只有 `summary`：D1「明细一律现取」，摘要缓存的存在是为了离线渲染目录页，
 * 不是明细缓存。正文、工具内容、diff、终端输出、附件与 ACP 原文一律只留内存。
 */
const PERSISTABLE_LOCAL_KINDS: Record<ContentKind, boolean> = {
  summary: true,
  prompt: false,
  tool_content: false,
  diff: false,
  terminal_output: false,
  attachment: false,
  acp_raw: false,
};

/**
 * 判定一份内容能否持久化。
 *
 * `remote`（imported）来源下**一切**类别都只留内存：spec 逐项点名了正文、提示与回复、
 * 工具内容、差异、终端输出、附件与 ACP 原文，因此这里没有任何例外分支。
 */
export function routeContent(origin: SessionOrigin, kind: ContentKind): ContentRoute {
  if (origin.kind === "remote") {
    return { destination: "memory_only" };
  }
  return PERSISTABLE_LOCAL_KINDS[kind] === true
    ? { destination: "local_summary_cache" }
    : { destination: "memory_only" };
}

/** 是否可以把用户输入标记为「已发送」或「已排队」。 */
export function mayMarkInputAsSent(input: { readonly online: boolean; readonly origin: SessionOrigin }): boolean {
  // §9.6：Owner 离线期间不得把远程会话的输入标记为已发送或已接受；
  // 第一阶段也不支持离线排队 prompt，因此本地会话同样要求在线。
  return input.online && input.origin.kind === "local";
}
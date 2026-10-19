/**
 * 协议层的运行期形状判定。
 *
 * `src/protocol/` 过去只描述类型，运行期校验在 `src/sync-client/wire.ts`；四条会话文件查询的
 * 结果要在进入 view model 前严格收窄，因此需要一份**唯一**的对象判定。把它放在这里而不是在
 * 每个解码模块里各写一份，避免同一个 `typeof value === "object"` 条件漂移成多套口径。
 */

/** 普通 JSON 对象（数组与 `null` 都不算）；字段值仍是 `unknown`，由调用方逐键判定。 */
export function isRecord(value: unknown): value is Readonly<Record<string, unknown>> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

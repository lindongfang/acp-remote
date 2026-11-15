# Web 原文与分块解码内核

状态：工程设计，待原独立 reviewer 核定本子阶段可编码范围。不是新页面或产品链路交付，不改变当前 Shared/Native 接口及83个独立用例。依据是 Shared 有效 revision2、prior-v1 assembly §3/5、same-read-rebind 和 replay-failure-v2，以及 structured-history 的原结构保留和内容安全要求。

## 1. 写范围与集成

Root 兼本子阶段 coder，在独占 `session-loop-q003-root-web-core/implementation-v1/workspace` 实现；不写其它作者 workspace。仅新增 `clients/web/src/views/{errors,binary,records,raw-json}.ts`、同目录必要私有实现和开发测试 `clients/web/tests/views/`。现有 Web 源、依赖、锁、入口、页面、WSS/Node/Rust 均不改。原 Shared 作者已确认其写范围仅 Rust，未改任何 Web scanner/renderer。

本阶段是 WP4/SV-web 的非视觉解码内核，不做页面/renderer，不假称实际浏览器或WSS测试已通过。未来真实页面使用时仍须 ui skill、当前来源/权限校验、安全DOM、独立浏览器与完整产品检查。纯编解码模块不持有管理客户端、fetch、WebSocket、DOM、存储、原生操作或审批回调，不能执行收到的文本。

最终交付原字节源清单、Root322前像上的精确新增 patch、实际 tsc/vitest/build、源码前后hash和失败日志；独立 tester/reviewer 同固定版本验证后再合入Root并运行实际Web及最终工程组合。保持原83 IDs/标题/预期，仅关联 SV-067/068/079 的本地子断言；SV-076–078/080–083 的真实页面、Node/WSS、Harness仍未执行。

## 2. 统一静态错误

导出 `ViewDecodeError extends Error`，只含 `code` 闭集：`invalid_header | invalid_frame | invalid_stream | invalid_sequence | invalid_record | invalid_json | invalid_utf8 | invalid_token | length_unrepresentable | allocation_failed | sealed`。message/name/code不含原字节、原生ID、URL或远端错误。不能将所有 RangeError 都解释成系统OOM；只在实际缓冲分配边界捕获可恢复 RangeError 为 allocation_failed，进程fatal OOM不声称可恢复或动态已测。

## 3. Binary API 与精确验证

```ts
export type ViewRecordKind = "original_raw" | "item_projection" | "status" | "snapshot_end";
export interface ViewDataFrame {
  readonly type: "data";
  readonly streamId: string;
  readonly packetSequence: bigint;
  readonly recordId: bigint;
  readonly byteOffset: bigint;
  readonly totalLength: bigint;
  readonly kind: ViewRecordKind;
  readonly last: boolean;
  readonly payload: Uint8Array;
}
export interface ViewCreditFrame { readonly type: "credit"; readonly streamId: string; readonly packetSequence: bigint; }
export interface ViewDetachFrame { readonly type: "detach"; readonly streamId: string; readonly packetSequence: bigint; }
export type ViewBinaryFrame = ViewDataFrame | ViewCreditFrame | ViewDetachFrame;
export function decodeViewFrame(wire: Uint8Array): ViewBinaryFrame;
export function encodeViewCredit(streamId: string, packetSequence: bigint): Uint8Array;
export function encodeViewDetach(streamId: string, packetSequence: bigint): Uint8Array;
```

完全沿已有 ACPV/1 固定72字节格式，不新定义type/flag。所有header u64从 DataView.getBigUint64/readBigInt及setBigUint64写，绝不经Number；u32 payloadLength只是实际最多16384字节局部长度。16字节streamId复用现规范base64url编码，解码要求canonical重编码相等且恰16字节。

逐字段检查magic/version/type/所有reserved/flags/kind、72+n精确长度、payload最大16384、recordId>0、u64加法无溢出、offset+n≤total、last恰等末片。只有snapshot_end可空，且必须total=offset=0、last=true、payload=0；其它Data不许空。Credit/Detach必须72字节、flags/recordId/offset/total/kind/payload全0，packet可0..u64MAX。非法JS负/超u64值在编码前拒绝，不依赖DataView截断。payload返回独立copy，调用者随后改wire不改变已解码frame。这里编码Credit只是字节，不产生服务端发送确认或读取权。

## 4. 连续 record 组装

```ts
export interface CompletedViewRecord {
  readonly streamId: string;
  readonly recordId: bigint;
  readonly kind: ViewRecordKind;
  readonly length: bigint;
  bytes(): Uint8Array;
}
export class ViewRecordAssembler {
  constructor(streamId: string);
  accept(frame: ViewDataFrame): CompletedViewRecord | null;
  seal(): void;
}
```

每实例绑定一个规范streamId；下个packet必须从1精确递增，u64溢出拒绝。每次独立复制payload进该实例私有所有权，再接受下一块；同in-flight record固定recordId/kind/total，offset连续，无interleaving或跨record拼接。最后片才产生不透明完成record。后续新record不要求recordId全局递增：原raw/平台status等本来是独立namespace，不能发明顺序限制；packet仍严格连续。

组装不按header totalLength提前分配巨缓冲，不设record累计quota或TTL，只增长实际到达的chunk集合。length仍BigInt。`bytes()`只有显式调用才分配实际已接收总长度的连续copy；checked转换为Number且与实际chunk总量相等，真实不可表示/分配失败明确报错。原chunk/record私有，不把可变底层数组暴露出去；返回copy修改不污染原物。JSON解码不由accept隐式触发。

seal永久拒绝该assembler的新数据并丢弃半条记录的本地组装责任，不发管理动作、不清节点缓存。实际stream rebind必须创建新实例，结束旧半record；本子阶段不从任意Status自动mint新socket/stream绑定，也不把旧credit转给新流。不同assembler由实际未来socket owner管理，codec自身不宣称当前鉴权。

## 5. 迭代原文 JSON scanner

```ts
export type RawJsonKind = "object" | "array" | "string" | "number" | "true" | "false" | "null";
export interface RawJsonToken {
  readonly kind: RawJsonKind;
  readonly startByte: number;
  readonly endByte: number;
  readonly parent: number | null;
  readonly role: "value" | "key";
}
export class RawJsonDocument {
  static parse(bytes: Uint8Array): RawJsonDocument;
  get tokens(): readonly RawJsonToken[];
  tokenBytes(index: number): Uint8Array;
  tokenLexeme(index: number): string;
  stringValue(index: number): string;
}
```

parse同步复制实际字节到私有拥有数组，严格UTF8验证，然后以平面tokens及显式容器栈单次迭代语法扫描，返回前freeze token记录与token数组。Number byte offset只表示当前实际Uint8Array内地址，不存原生数值。object/array token范围含完整原容器，parent是token索引而非递归JS对象。object键作为role=key/string token保原位置，值token也直接以该object为parent；相邻顺序保键/值关系。保留重复键和键序，不悄悄覆写；通用scanner不是平台封闭DTO解析器，也不凭重复键映射选择执行目标。

完整JSON语法：根恰一个value、只允许JSON空白、object键必须string且有冒号、数组/对象逗号规则、合法字符串转义及\u四hex、原字符串禁止未转义0..31、数字严格minus/int/fraction/exponent、literal精确true/false/null、拒绝尾随垃圾/未闭容器/trailing comma。不能用递归下降、JSON.parse整raw、eval或Function；不新增深度/叶数/正文总预算。巨数、-0、指数词法原样保留，未知结构不转换成普通JS对象或重编码。

只对已验证string token的原JSON literal调用JSON.parse解码；stringValue拒绝非string。tokenLexeme仅严格UTF8解码原切片，number不转Number。tokenBytes返回copy，index需有效整数且在当前tokens内；错误无正文。tokens/getter只能产生原文数据，不产生可信URI、option选择或管理动作。raw跨网络chunk的UTF8边界先按原bytes组装，scanner只在完整record上做严格UTF8，不逐chunk误判；streaming逐字显示另属未来renderer，不能把现完整record scanner写成增量显示已实现。

## 6. 本阶段开发验证与边界

开发测试包括：全部72字段负例、u64大于2^53/边界/溢出、规范stream ID、snapshot空规则、UTF8四字节跨chunk、连续/错offset/跨流/换kind/半record seal、输入及返回copy修改隔离、巨数与-0指数原lexeme、转义/键序/重复键、至少20000层真实深树迭代及大量叶子、非法JSON语法表、非法UTF8、只string literal可JSON.parse。

原Web检查/50用例与build真实运行，不外发npm audit、不新增依赖。私有u64极值用实际codec bytes验证；真实不可分配长度、fatal OOM不伪动态覆盖。静态检查模块无DOM/fetch/WebSocket/management import，只允许原规范base64url工具。开发测试不改独立83预期、不宣称模拟transport就是真实WSS。完整browser安全展示和Node读取撤销继续待对应阶段。

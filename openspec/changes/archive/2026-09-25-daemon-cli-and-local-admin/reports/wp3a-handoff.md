# coder 报告 · WP3a（`server` crate 骨架 + `transport::local` + `local_admin` 信封层，交付单元 DU1）

- task_id: `2.7`、`2.8`、`2.9`（工作包 WP3 前半，记为 WP3a；`2.10`–`2.13` 由后续派发完成）
- role: coder
- phase: implement
- stage: work-package
- agent_context: 独立子 Agent（worker / coder 角色），主 worktree 的分支 `feat/daemon-cli-and-local-admin`；只继承本任务单给出的契约输入（`docs/LOCAL_ADMIN_PROTOCOL.md`、`design.md` D1/D5、两份 spec、`schemas/`+`fixtures/local-admin/v1/`、`docs/MODULE_ARCHITECTURE.md` §4.9/§5、根 `Cargo.toml`、WP2 交接的 wrapper safe API），不继承规划阶段与 WP1/WP2 的实现对话；本变更各波次串行（同一时刻只有一个写入者），因此没有并发写者，也未使用独立 `CARGO_TARGET_DIR`
- target_revision: `c12957ee3c4ffdcab9db8533d23b42e4673107ba`（WP3a 的代码提交）
- base_revision: `2636dbe`（任务单给定的起点，开工前用 `git log --oneline -3` 核对，与任务单一致）
- scope（写入范围，逐字取自任务单）：`crates/server/**`（新建）、根 `Cargo.toml`（仅 `[workspace] members` 增加 `"crates/server"` 一行）、`Cargo.lock`、`openspec/changes/daemon-cli-and-local-admin/reports/`。**未改**任何其它 crate、`docs/`、`vendor/`、规划文件（`plan.md`/`tasks.md`/`verification.md`/`design.md`/`proposal.md`/`specs/`）
- dependencies（上游输入）：`docs/LOCAL_ADMIN_PROTOCOL.md` 全文（§1.1/§2/§3/§3.1/§4/§5/§6/§7）、`design.md` 决策 1/5、`specs/local-admin-channel` 与 `specs/local-admin-methods`、`schemas/local-admin/v1/envelope.schema.json`、`fixtures/local-admin/v1/`（含 manifest 与 README）、`docs/MODULE_ARCHITECTURE.md` §4.9/§5、WP2 冻结的 `windows_local_ipc::{create_pipe_server, current_user_sid, client_user_sid}`
- result: `PASS`（本工作包的检查与交付条件全部满足；**不代表**独立 review、集成或合并已完成）
- issues: 见「已知限制与偏差」与「未执行项与待主 Agent 决策」——需要主 Agent 知悉的有 4 条契约/口径问题（§5.7 与 §4 方法名正则冲突、缺失 `id` 时的响应 `id` 取值、`local.unsupported` 消息回显方法名、Unix 对端凭据校验收窄到 Linux/Android）与 4 条无法在本机闭环的验收项
- evidence_paths: `reports/wp3-server-transport.log`（[PV3] WP3a transport 段）、`reports/wp3-server-envelope.log`（[PV3] WP3a 信封段）、`reports/wp3-server-boundaries.log`（[PV2]）、`reports/wp3-verify.log`（[PV1] 本 WP 的 `npm run verify` 全量轮）、`reports/du1-pv1.log`（[PV1] 追加的 WP3a 轮）、`reports/pv5-windows-ipc.log`（[PV5] 追加的 WP3a 段，WP2 段保留）、本报告

## 提交与交付对应

| 提交 | 类型 | 内容 | 覆盖任务 |
| --- | --- | --- | --- |
| `c12957ee3c4ffdcab9db8533d23b42e4673107ba` | `feat(server)` | `crates/server` 的 18 个源文件 + 5 个集成测试文件 + `Cargo.toml`；根 `Cargo.toml` 的 1 行成员新增；`Cargo.lock` 自然更新（新增 `server` 包） | 2.7、2.8、2.9 |
| 本报告提交 | `docs(server)` | `reports/wp3a-handoff.md`（本文件） | 2.7/2.8/2.9 的交接 |

`git status --porcelain -uall` 在代码提交前只有：`Cargo.lock`、`Cargo.toml`、`crates/server/**`（其余无改动；`reports/*.log` 按仓库约定被 `.gitignore` 排除，不入库）。提交后工作区干净，未暂存任何文件。

## 冻结的公开形状（WP3b 与 WP4 要消费的接口）

### 1. 装配与接受循环（`app::daemon` 用）

```rust
use server::transport::local::{
    AuditHook, EndpointError, InstanceId, LocalConnectionHandlers, LocalEndpoint,
    LocalEndpointConfig, LocalStream, LoggingAuditHook, serve_connection,
};

// 1) endpoint（§2.1）：创建失败即失败关闭；调用方必须先持有单实例锁
let mut endpoint = LocalEndpoint::bind(
    LocalEndpointConfig {
        data_dir: /* daemon.data_dir */,
        instance_id: /* 单实例锁生成的 16 字符小写 hex */ InstanceId::generate(),
        runtime_dir: None,          // Unix：None = 读 $XDG_RUNTIME_DIR；Windows 忽略
    },
    Arc::new(LoggingAuditHook),     // WP3b 换成写 core 审计写集的实现
).await?;
let where_ = endpoint.describe();   // Windows: 完整 pipe 名；Unix: socket 路径（日志用）

// 2) 接受循环：PeerRejected 不是致命错误（本次连接已被拒绝且记了审计），继续接受下一条
loop {
    match endpoint.accept().await {
        Ok(stream) => { /* spawn serve_connection(stream, handlers) */ }
        Err(EndpointError::PeerRejected) => continue,
        Err(error) => return Err(error),   // endpoint 本身不可用 → 失败关闭
    }
}

// 3) 处理一条连接
let handlers = Arc::new(LocalConnectionHandlers::new(Arc::new(router)));  // router: Arc<dyn LocalAdminHandler>
let reason: CloseReason = serve_connection(stream, handlers).await?;      // CloseReason::FacadeUnavailable 等
```

`LocalStream` 是 `cfg` 选定的别名（Windows `NamedPipeServer`、Unix `UnixStream`），调用方不需要任何 `cfg`。

### 2. 方法路由（WP3b 实现 `LocalAdminHandler`）

```rust
#[async_trait::async_trait]
pub trait LocalAdminHandler: Send + Sync {
    async fn handle(&self, request: AdminRequest) -> AdminResponse;   // 恰好一个响应，回带同一 id
}

impl AdminRequest {
    pub fn id(&self) -> &RequestId;      // canonical 小写 UUID
    pub fn method(&self) -> Method;      // v1 方法集枚举（24 个变体，`Method::ALL`）
    pub fn params(&self) -> &JsonObject; // serde_json::Map<String, Value>（开放容器，arbitrary_precision 保真）
}

impl AdminResponse {
    pub fn success(id: RequestId, result: JsonObject) -> Self;
    pub fn failure(id: RequestId, error: AdminError) -> Self;
    pub fn id(&self) -> &RequestId;
    pub fn outcome(&self) -> &AdminOutcome;                      // Success{result} | Failure{error}
    pub fn encode(&self) -> Result<Vec<u8>, EnvelopeEncodeError>; // 只出信封 JSON，framing 由传输层加
    pub fn decode(payload: &[u8]) -> Result<Self, ResponseDecodeError>;  // WP4 的 CLI 复用同一类型
}

impl AdminError {                                   // error{code,message}
    pub fn new(code: LocalErrorCode, message: impl AsRef<str>) -> Self;  // 消息 trim+截断到 1..=512
    pub fn code(&self) -> LocalErrorCode;
    pub fn message(&self) -> &str;
    pub fn unknown_method(method: &str) -> Self;    // 不在 v1 方法集里
    pub fn unsupported_method(method: Method) -> Self; // 在集内但本阶段未实现
    pub fn duplicate_request_id() -> Self;
}
pub enum LocalErrorCode { Unsupported, InvalidRequest, InvalidParams, NotFound, Conflict,
                          Expired, RemoteError, Unavailable, Internal }   // 9 个，`LocalErrorCode::ALL`
```

WP3a 装的是 `UnroutedAdminHandler`（空路由）：集内方法一律 `local.unsupported`。WP3b 替换它即可，传输层不动。

### 3. 传输层的公开常量与类型（漂移测试与 WP3b/WP4 引用）

```rust
server::transport::local::{LENGTH_PREFIX_BYTES, MAX_FRAME_PAYLOAD_BYTES, MAX_IN_FLIGHT_REQUESTS,
                           CHANNEL_LOCAL_ADMIN_BYTE, CHANNEL_ACP_STREAM_BYTE, ChannelKind, Frame,
                           FrameReader, CloseReason, TransportError, FacadeAttachmentId,
                           FacadeAttachmentRegistry, AuditHook, AuthorizationDenied, DeniedReason}
server::local_admin::{Method, LocalErrorCode, decode_request, decode_response, RequestId,
                      RequestDecodeError, RequestDecodeOutcome, InvalidRequestReason,
                      ResponseDecodeError, EnvelopeEncodeError, JsonObject, is_method_name}
```

`AuditHook`（WP3b 接线点）：

```rust
pub trait AuditHook: Send + Sync {
    fn authorization_denied(&self, event: AuthorizationDenied<'_>);
}
pub struct AuthorizationDenied<'a> {
    pub transport: &'a str,                  // "named_pipe" / "unix_socket"
    pub instance_id: &'a str,                // §2.1 的 <instanceId>（不是路径）
    pub reason: DeniedReason,                // PeerIdentityUnavailable | DifferentOsUser
    pub peer_identity: Option<&'a str>,      // SID 字符串或十进制 uid（非 secret）
}
```

`FacadeAttachmentId` / `FacadeAttachmentRegistry`（切片 6 用）：`attach()` 分配新的 16 字符小写 hex、
`detach()` 作废、`route()` 对已作废 attachment 的延迟帧丢弃并记结构化警告（不命中新绑定）。本切片的
`0x02` 分支在 `attach` 之前就关闭连接，因此不分配、不消耗 attachment（§3.1 实现状态注记）。

## `serve_connection` 的行为矩阵（WP3b/WP4 复核用）

| 输入 | 结果 |
| --- | --- |
| 首帧 `0x01` + 合法信封 | 交给 `LocalAdminHandler`，响应按 `id` 写回；连接可继续承载请求 |
| 首帧 `0x01` + 信封非法（`method` 命名非法/`params` 非 object/未知字段/`id` 缺失或非 UUID） | 立刻回 `local.invalid_request`（`id` 能解析就回带，否则 nil UUID），连接保持可用 |
| 首帧 `0x01` + 语法合法但不在 v1 方法集 | 立刻回 `local.unsupported`（消息含方法名），连接保持可用 |
| 信封 `v != 1`（含缺失/非整数） | 关闭连接，**不发任何帧** |
| 同一连接未完成请求的 `id` 重复 | 立刻回 `local.invalid_request`，不占用新的未完成槽位 |
| 第 33 条未完成请求 | 关闭连接，**不发任何帧**（已完成的响应不受影响） |
| 首帧 `0x02` | framing 校验通过后立即关闭，`CloseReason::FacadeUnavailable` + 结构化警告 |
| 后续帧 channel 与首帧不同 | 关闭连接，**不发任何帧** |
| `length == 0` / `length > 1 MiB` / 未知 channel 字节 | 关闭连接，**不发任何帧**（`FrameTooLarge` 时不读 payload） |
| 对端断开（含半个帧后断开） | `CloseReason::PeerEof`；半个帧会在日志里记 `buffered_bytes` |
| 处理器任务 panic | 连接级错误 `TransportError::RequestTask`（无法回带 `id`，不伪造响应） |

连接结束时未完成的处理器任务随 `JoinSet` 的 drop 被中止（链路已结束、响应无法送达）；处理器必须保证被取消
时不留下半提交状态（core 的写集是单事务，取消即回滚）。

## 任务 2.7（骨架 + framing 层）

| 文件 | 内容 |
| --- | --- |
| `crates/server/Cargo.toml` | `publish`/`edition`/`rust-version` 继承 workspace、`[lints] workspace = true`（`unsafe_code = "forbid"` 保持）；依赖只加本切片需要的：tokio（`workspace` + `rt`）、serde、serde_json、thiserror、tracing、sha2、uuid、async-trait；`cfg(windows)` → `windows-local-ipc`（path）、`cfg(unix)` → `nix`；dev-dependencies 显式声明同一批（不依赖「普通依赖对测试目标可见」这一隐式前提） |
| `src/lib.rs` | crate 文档：本切片只落地 `transport::local` 与 `local_admin`；不含业务规则、不依赖 `app` |
| `src/transport/local/framing.rs` | `u32be length | payload` 编解码；`MAX_FRAME_PAYLOAD_BYTES = 1_048_576`；channel 解析；**有缓冲且取消安全**的 `FrameReader::next_frame`（用 `AsyncReadExt::read` 追加缓冲，因此可安全参与 `select!`） |
| `src/transport/local/connection.rs` | `serve_connection`：首帧定 channel、管理请求分派（`JoinSet` 并存未完成请求）、未完成请求上限 32 与 `id` 唯一性、`v != 1` 连接级关闭、`0x02` 即连即关、关闭原因与结构化日志 |
| `src/transport/local/attachment.rs` | `FacadeAttachmentId`（16 字符小写 hex，由 v4 UUID 前 16 位 hex 组成）与 `FacadeAttachmentRegistry`（attach/detach/is_active/route） |
| 根 `Cargo.toml` | `members` 增加 `"crates/server"` 一行 |

测试覆盖（`tests/local_admin_channel.rs`，14 个用例，`tokio::io::duplex` 驱动）：

| 用例 | 断言 |
| --- | --- |
| `legal_admin_frames_are_answered_and_keep_the_connection_usable` | 两帧各自恰好一个响应、`id` 回带、channel 为 `0x01`、断开后 `CloseReason::PeerEof` |
| `empty_frame_closes_the_connection_without_an_error_frame` | `length = 0` → 关闭、**零字节**输出、`CloseReason::EmptyFrame` |
| `oversized_frame_closes_the_connection_without_reading_the_payload` | `length = 1 MiB + 1` → 关闭、零字节输出、`FrameTooLarge`（不读 payload） |
| `frame_at_the_exact_limit_is_accepted_by_framing` | payload 恰好 1 MiB 合法（取等号），信封非法 → `local.invalid_request`（区分 framing 与信封两类错误） |
| `unknown_channel_closes_the_connection_without_an_error_frame` | 首字节 `0x03` → `UnknownChannel { byte: 3 }`、零字节输出 |
| `channel_mismatch_closes_the_connection_after_the_answer` | 首帧 `0x01` 得到响应后收 `0x02` → `ChannelMismatch`、之后零字节输出 |
| `acp_stream_connection_is_closed_immediately_while_facade_is_absent` | 首帧 `0x02` → `CloseReason::FacadeUnavailable`、零字节输出（不分配 attachment） |
| `envelope_version_two_closes_the_connection_without_an_error_frame` | `v = 2` → `ChannelVersionUnsupported`、零字节输出 |
| `rejected_requests_keep_the_connection_usable` | `daemon.status`（集内未实现）/`daemon.doctor`（语法合法不在集内）→ `local.unsupported`；`params: null`、`Daemon.status`、`node.rotate-key.begin` → `local.invalid_request`；四种之后连接仍可用 |
| `invalid_params_from_the_method_layer_travels_as_a_normal_response` | 方法层返回 `local.invalid_params` 时响应信封正确、连接保持可用（R35 的后半） |
| `error_responses_do_not_echo_request_params` | `params` 里的凭据形状值不出现在响应任何位置，`error.message` 非空、≤512、ASCII（§4 规则 6） |
| `thirty_two_in_flight_requests_are_allowed_and_the_limit_recovers` | 32 条未完成请求全部得到响应且 id 集合完整；计数回落、连接仍可用 |
| `the_thirty_third_in_flight_request_closes_the_connection` | 第 33 条 → 关闭、零字节输出、`TooManyInFlightRequests` |
| `duplicate_in_flight_request_id_is_rejected_without_closing_the_connection` | 重复 `id` → `local.invalid_request`，原请求仍得到成功响应，连接可用 |

单元用例（`transport::local::*` 内）：帧头边界（0/恰好 1 MiB/超限/未知 channel）、channel 往返、
`encode_frame` 的长度前缀与上限、attachment 两次连接不同 ID / 作废后不命中新绑定 / 生成 id 形状、
`authorization.denied` 事件带全字段到达注入的 hook。

## 任务 2.8（endpoint 与访问控制）

| 文件 | 内容 |
| --- | --- |
| `src/transport/local/endpoint.rs` | 平台无关部分：`InstanceId`（16 字符小写 hex，形状校验 + 唯一随机来源）、`LocalEndpointConfig`、`EndpointError`、`user_identity_hash`（SHA-256 小写 hex 前 16 字符）、`named_pipe_path`、`unix_socket_path`、`unix_endpoint_directory` |
| `src/transport/local/platform/windows.rs` | 首实例经 `windows_local_ipc::create_pipe_server`（SDDL），后续实例经 tokio `ServerOptions`（与 wrapper 同参数：默认 `PIPE_UNLIMITED_INSTANCES`/65536/字节模式/拒绝远端客户端）；每次 `accept` 先备好下一实例再校验对端 `client_user_sid == current_user_sid`；`ERROR_ACCESS_DENIED` → `EndpointError::AlreadyInUse` |
| `src/transport/local/platform/unix.rs` | `UnixListener` + 目录 `0700`/socket `0600`（设置后核对）、符号链接/非目录/非 socket 一律拒绝、残留 socket 只有在「连不上」时才删除重建（连得上 → `AlreadyInUse`）、`SO_PEERCRED` uid 与本进程 uid 比对 |
| `src/transport/local/audit.rs` | `AuditHook` 接线点 + `LoggingAuditHook`（结构化日志）+ 平台共用的 `deny_connection` |

测试覆盖：

| 环境 | 用例 | 断言 |
| --- | --- | --- |
| 全平台（`tests/local_endpoint_naming.rs`） | `user_identity_hash_is_the_first_16_hex_characters_of_sha256` | 4 个输入独立复算 SHA-256 前 16 hex；附加换行/前缀会改变结果 |
| 全平台 | `named_pipe_path_follows_the_documented_rule` | `\\.\pipe\acp-remote-<16 hex>-<instanceId>`，哈希来自 SID 字符串 |
| 全平台 | `unix_endpoint_uses_xdg_runtime_dir_then_data_dir_run` | XDG 路径、`<data_dir>/run` 回落、显式覆盖优先三档 |
| 全平台 | `instance_id_shape_is_validated` | 大小写/长度/非 hex 拒绝，`generate()` 产出 16 字符 |
| Windows 本机（`tests/local_endpoint_windows.rs`，[PV5]） | `pipe_name_matches_the_endpoint_rule_on_this_host` | 真实 SID → `describe()` 逐字等于规则（本机实测 `sid_hash=772b6a39c8bd48ac`） |
| Windows 本机 | `same_user_client_is_accepted_and_admin_requests_are_answered` | 真实 pipe 上完成「连接 → 凭据校验 → 管理请求 → 响应 → EOF」 |
| Windows 本机 | `subsequent_pipe_instances_are_created_and_accept_same_user_clients` | 连续 3 轮：同名后续实例可创建、可连接、凭据校验通过（**WP2 移交的「后续实例 DACL 继承与否」未核实项的结论：可接受**） |
| Windows 本机 | `a_second_endpoint_on_the_same_name_fails_closed` | 同名第二个首实例 → `EndpointError::AlreadyInUse`（失败关闭） |
| Unix（`tests/local_endpoint_unix.rs`，Linux CI 执行） | `unix_endpoint_creates_socket_with_private_permissions` | socket 路径正确、目录 `0700`、socket `0600` |
| Unix | `unix_endpoint_tightens_a_pre_existing_directory` | 预建 `0777` 目录被收紧到 `0700` 后成功启动 |
| Unix | `unix_endpoint_falls_back_to_data_dir_run_when_xdg_is_unset` | `XDG_RUNTIME_DIR` 未设置时用 `<data_dir>/run`（设置时跳过并说明，规则由纯函数用例覆盖） |
| Unix | `unix_endpoint_refuses_to_replace_a_live_socket` | 有存活 listener → `AlreadyInUse`（拒绝启动） |
| Unix | `unix_endpoint_replaces_a_stale_socket_and_accepts_same_user_clients` | 残留 socket 删除重建，随后真实 `SO_PEERCRED` + 管理请求往返成功 |
| Unix | `unix_endpoint_rejects_unsafe_paths` | 非 socket / socket 位置的符号链接 / 目录位置的符号链接 / 目录位置的非目录 四种 `UnsafePath` |
| Unix（`platform/unix.rs` 内） | `peer_user_id_matches_current_user_for_a_local_pair` | `SO_PEERCRED` 取到的 uid 等于本进程 uid（非 Linux/Android 目标返回「无法确认」） |

## 任务 2.9（管理信封）

| 文件 | 内容 |
| --- | --- |
| `src/local_admin/envelope.rs` | `RequestId`（canonical 小写 UUID，含 nil 哨兵）、`AdminRequest`、`AdminResponse`/`AdminOutcome`、`decode_request`/`decode_response`、`RequestDecodeError`→`RequestDecodeOutcome`（关闭连接 / 回响应）、`ResponseDecodeError` |
| `src/local_admin/method.rs` | `Method`（24 个变体 + `ALL` + `as_str`/`from_name`）、`is_method_name`（`^[a-z][a-z0-9]*(\.[a-z0-9]+)*$` 的手写判定） |
| `src/local_admin/error.rs` | `LocalErrorCode`（9 个 + `ALL` + `parse` + 默认消息）、`AdminError`（消息 trim + 截断到 512、空消息回落默认值） |
| `src/local_admin/handler.rs` | `LocalAdminHandler` 契约 + `UnroutedAdminHandler`（WP3a 空路由） |
| `tests/local_admin_schema_drift.rs` | 常驻漂移测试（6 个用例） |

漂移测试逐条：

1. `framing_constants_match_the_schema`：`lengthPrefixBytes`/`maxPayloadBytes`/`maxInFlightRequests`/`channelLocalAdmin`/`channelAcpStream` 与 Rust 常量相等。
2. `method_set_matches_the_schema_enum`：`Method::ALL` 与 `methodName.enum` **逐项相等**（顺序也相等）+ 正则字面量一致 + 每个名字可反查。
3. `error_codes_match_the_schema_enum`：`LocalErrorCode::ALL` 与 `errorCode.enum` 逐项相等。
4. `envelope_shapes_match_the_schema`：三个分支 `additionalProperties: false`、required 字段、`params`/`result` 为 object、`ok` 的 `const`、`error` 只允许 `code`/`message`、`message` 的 `1..=512`、`v` 恒为 1、uuid 正则字面量。
5. `fixture_table_matches_the_manifest`：Rust 侧 `include_str!` 清单与 `manifest.json` 的 11 条 case **双向**比对（路径、`valid`、`expectedKeyword`）。
6. `fixtures_behave_as_declared`：5 条 valid（请求可解码且方法名/params 字段与文档 §5 一致；响应可解码且再编码后往返相等）+ 6 条 invalid 逐条按声明被拒（`required`/`type` → `local.invalid_request`；`const` → 连接级关闭；`enum` → 方法名被拒；两条响应 fixture → 响应解码失败）。

信封规则与错误码另由单元用例覆盖：闭包对象（未知字段）、`v` 缺失/非 1、`params: null|[]|缺失`、
`id` 大小写/无连字符/带括号/URN 全拒、30 位整数字面量在 `params` 里保真、响应解码拒绝 10 类违反、
`local.unsupported` 的响应 JSON 逐字节断言、错误码文本与默认消息、消息截断。

## checks（逐 Check ID 汇总）

| Check ID | 命令 / 目录 | 配置与环境 | 结果 | 日志 |
| --- | --- | --- | --- | --- |
| [PV3] | `cargo test --locked -p server --all-features`（+ `transport`/`local_endpoint`/`local_admin`/`envelope`/`--test local_admin_schema_drift` 分组轮）@ 仓库根 | Windows x64；cargo/rustc 1.98.1；`--locked` | 退出码全 0；53 个用例通过、0 失败、0 ignored（lib 25 + channel 14 + schema drift 6 + naming 4 + windows 4；unix 文件在 Windows 上 0 用例，属 `#![cfg(unix)]`） | `reports/wp3-server-transport.log` (1)(2)(9)、`reports/wp3-server-envelope.log` (1)(2)(3)(6) |
| [PV3]（跨目标编译） | `cargo clippy --locked -p server --all-targets --all-features --target x86_64-unknown-linux-gnu -- -D warnings` | 已安装 Linux std；**只编译不执行** | 退出码 0（Unix 路径与 `#[cfg(unix)]` 用例全部通过类型检查与 lint） | `reports/wp3-server-transport.log` (5) |
| [PV5] | `cargo test --locked -p server --all-features --test local_endpoint_windows -- --nocapture`（Windows 本机） | Windows x64；真实 Named Pipe；同一时刻仅此一轮 | 退出码 0；4 个用例通过、0 失败；打印真实 `sid_hash=772b6a39c8bd48ac` | `reports/pv5-windows-ipc.log`（WP3a 段 (W1)(W2)(W4)，WP2 段保留） |
| [PV2] | `node scripts/check-crate-boundaries.mjs` @ 仓库根 | Node v24.19.0；`cargo metadata --no-deps` | 退出码 0：`crate boundaries OK: 11 个 crate ...`；成员 10 → 11；`server` 的唯一工作区内依赖边是 `windows-local-ipc`（矩阵 ✓） | `reports/wp3-server-boundaries.log` |
| [PV1] | `npm run verify`（= `npm run check` 的 10 道门禁 + `cargo fmt --check` + `cargo clippy --locked --workspace ...` + `cargo test --locked --workspace --all-features`）@ 仓库根 | Node v24.19.0 / npm 12.0.2；离线 | 退出码 0；workspace 568 个用例通过、0 失败；10 道合同门禁全绿（含 `crate boundaries OK: 11`） | `reports/wp3-verify.log`（全量）、`reports/du1-pv1.log`（WP3a 轮摘要） |
| LC1 | `cargo fmt --all -- --check` | 默认 rustfmt | 退出码 0 | `reports/wp3-server-transport.log` (3) |
| LC2 | `rg -n "unsafe" crates/server/src` | ripgrep | 退出码 1（**零命中**）。对照 `rg -n "unsafe" crates/server` 有 3 处：manifest 注释里引用 workspace lint 名 1 处、Unix 用例的测试函数名（指 `EndpointError::UnsafePath` 语义）2 处，均无 Rust unsafe 代码 | `reports/wp3-server-transport.log` (6) |
| LC3 | `rg` 自检「正常路径无 `unwrap()`/`expect()`/`panic!`」 | 人工核对 `crates/server/src` 的全部命中 | 全部命中都落在各文件的 `#[cfg(test)] mod tests` 内（逐文件核对行号与 `mod tests` 起始行） | 本报告「任务 2.7/2.9」小节的单元用例清单 |
| LC4 | `cargo tree --locked -p server --edges normal --depth 1` | — | 退出码 0；工作区内依赖只有 `windows-local-ipc`，其余 8 个均为 workspace 已登记的第三方依赖 | `reports/wp3-server-transport.log` (7) |

提交后复核轮（`target_revision = c12957e`，工作区干净）：[PV2] 与 [PV3] 的 server 全量测试在提交后各重跑一次，结果与提交前一致（见 `reports/wp3-server-transport.log` (9)、`reports/wp3-server-envelope.log` (6)、`reports/pv5-windows-ipc.log` (W4)）。代码提交本身经 `.husky/pre-commit`（`cargo fmt --check` + `npm run check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`）与 `commitlint` 通过。

## 已知限制与偏差

1. **跨用户拒绝在本机与 Linux CI 都无法真实执行**（平台限制，如实记录）：Windows 本机只有一个可用账号，「另一个用户的进程连接 pipe」构造不出；Linux CI 同样没有第二个账号（需要 root 或额外用户）。本轮可得的是：同用户连接被接受（真实 pipe / 真实 Unix socket）、每次 `accept` 都逐字比较对端 SID/uid、WP2 的静态证据（DACL 只有当前用户一条 ACE、不继承）与失败路径（拿不到对端身份即 `Err`）。**该条按限制登记，不写成 PASS**。
2. **Unix 路径本机未执行**：本机无 WSL/无 Linux 运行环境，`tests/local_endpoint_unix.rs` 与 `platform/unix.rs` 的单元用例只做了 `--target x86_64-unknown-linux-gnu` 的编译 + clippy 检查（[PV3] 的 CI 轮会真正执行）。
3. **Unix 对端凭据校验收窄到 Linux/Android**：nix 的 `sockopt::PeerCredentials`（`SO_PEERCRED`）只在这些目标上存在。其他 Unix 目标（macOS 的等价物是 `LOCAL_PEERCRED`/`XuCred`）上 `peer_user_id` 返回「无法确认」，于是**每次连接都被拒绝**（失败关闭 + `authorization.denied` + 结构化警告），而不是静默放行。这与任务单「Unix 路径用 `cfg(unix)` 编写」的字面写法有差异：`cfg(unix)` 保留了整个模块，只有凭据查询那一层按目标收窄；理由是照字面写在 macOS 上会**编译失败**（`PeerCredentials` 不存在），而失败关闭是更安全的收窄。是否需要改口径由主 Agent 决定。
4. **文档 §5.7 与 §4 的方法名正则冲突（契约问题，未改文档）**：§5.7 写 `node.rotate-key.begin`「返回 `local.unsupported`」，但 §4 的方法名语法 `^[a-z][a-z0-9]*(\.[a-z0-9]+)*$` **不允许连字符**，因此该名字按 §4 规则 4 属「命名非法」→ `local.invalid_request`。本实现按 §4/§6 的显式措辞处理（`node.rotate-key.begin`、`fixture` 里的 `device.rotate-key.begin` 都回 `local.invalid_request`），并把「语法合法但不在集内」的 `local.unsupported` 路径用 `daemon.doctor` 覆盖。若主 Agent 认为 §5.7 才是预期，需要放宽 §4 的方法名正则（同时改 schema 的 `pattern` 与口径），本 WP 不擅自改契约。
5. **信封 `id` 缺失/非法时的响应 `id`**：§4 规则 4 要求此时回 `local.invalid_request`，但 §4 规则 2 要求响应回带请求的 `id`，而 schema 又要求响应 `id` 必须是 UUID——三者无法同时满足。实现取**全零 UUID 哨兵**（`RequestId::NIL`，schema 合法、语义是「无关联」），并在代码注释里写明理由。文档未定义该情形，是否需要补一句由主 Agent 决定。
6. **`local.unsupported` 的消息回显方法名**：为让「新 CLI 连上旧 Daemon」能看出是哪个方法不被支持，`AdminError::unknown_method(&str)` 把方法名写进 `error.message`（长度被收窄到 512 字符，不含 params）。§4 规则 6 只禁止 secret/堆栈/完整敏感路径，方法名不属其中，但若主 Agent 希望错误消息完全不回显对端输入，这是一处需要改口径的点。
7. **R35 的后半（方法级参数校验）属 WP3b**：WP3a 没有方法路由，`params` 缺字段/含未知字段的 `local.invalid_params` 需要各方法的形状知识（`tasks.md` 2.10–2.12）。本轮覆盖的是：信封级「`params` 不是 object → `local.invalid_request`」+ 「方法层返回 `local.invalid_params` 时通道行为正确」的完整编码路径。
8. **R29（attachment 生命周期）由注册表单元用例覆盖，而非端到端 `0x02` 流**：这是 §3.1 实现状态注记的必然结果（facade 未装配 → `0x02` 连接即连即关，不分配 attachment）。切片 6 接入 facade 后可补端到端用例。
9. **`LocalEndpointConfig::runtime_dir` 是测试可判定性所需的最小钩子**：Rust 2024 起修改进程环境变量需要显式的不安全块（workspace forbid 级 lint 不允许），若只能经 `XDG_RUNTIME_DIR` 指定运行时目录，§2.1 的两条路径无法同时被测试覆盖；`app` 正常传 `None`。该字段的文档注释写明了理由。
10. **`server` 仍未依赖 `core`/两个协议 crate**：§5 矩阵允许，但本切片不需要；WP3b 接入方法路由时再加（避免为未来需求提前加依赖）。

## 未执行项与待主 Agent 决策

- 独立 review RV1（WP3）尚未返回（`tasks.md` 3.6），本报告不把它写成 PASS；WP2 的 RV1 也仍为 PENDING（`tasks.md` 3.4，与本 WP 无关）。
- `cargo-deny`（`deps`/`advisories`）与 `gitleaks` 只在 CI 运行，本地无等价物：本轮未执行，不声称通过。
- 待主 Agent 决策/同步文档的 4 项：上面「已知限制与偏差」第 3、4、5、6 条（Unix 目标收窄的口径、§5.7 与 §4 的正则冲突、nil UUID 哨兵、`local.unsupported` 消息是否回显方法名）。
- `tasks.md` 2.7–2.9 的勾选与 `verification.md` 的证据登记由主 Agent 负责（本 WP 不改规划文件）。
- WP3b 的接手点：实现 `LocalAdminHandler` 并替换 `UnroutedAdminHandler`（传输层与信封层已冻结）；按需增加 `core`/`identity-auth` 依赖；`local.invalid_params` 的方法级校验落在方法实现里。

## 资源释放

| 资源 | 归属 | 状态 |
| --- | --- | --- |
| 本机 Named Pipe 实例 | 本 WP | 随用例句柄关闭全部消失（未创建后台进程；`a_second_endpoint_on_the_same_name_fails_closed` 专门覆盖「同名占用」路径并随测试结束释放） |
| 临时目录/文件 | 本 WP | Windows 用例只用 `std::env::temp_dir()` 组装配置（未落盘）；Unix 用例自建 `<temp>/acp-remote-wp3-*` 并在 `Drop` 中删除（Linux CI 执行时生效） |
| `reports/` 下的日志 | 本 WP | 5 个日志文件（`wp3-server-transport.log`、`wp3-server-envelope.log`、`wp3-server-boundaries.log`、`wp3-verify.log` 新增；`du1-pv1.log`、`pv5-windows-ipc.log` 追加），按仓库约定不入库 |
| 仓库根 `target/` | 共享 | 未清空（增量缓存，各执行者自行决定） |
| `git` 工作区 | 本 WP | 提交后 `git status --porcelain -uall` 为空；无临时文件、无 `--no-verify` 提交 |

```yaml
handoff_index:
  - task_id: "2.7"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "c12957ee3c4ffdcab9db8533d23b42e4673107ba"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3a-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP3a 轮：cargo test --locked -p server --all-features 在 Windows x64 本机执行（含 transport/framing/attachment/audit 分组轮与提交后复核轮），lib 25 + channel 14 + schema drift 6 + naming 4 + windows 4 = 53 个用例通过、0 失败、0 ignored；framing 与 channel 绑定场景（R23–R32）逐条可见；工具链 cargo/rustc 1.98.1；日志 reports/wp3-server-transport.log 的 (1)(2)(9) 节。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "c12957ee3c4ffdcab9db8533d23b42e4673107ba"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3a-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP3a 段：crates/server 的 Named Pipe endpoint 用例在 Windows 本机执行（cargo test --locked -p server --all-features --test local_endpoint_windows -- --nocapture，4 passed/0 failed，含同用户真实连接 + 管理请求往返、同名后续实例连续 3 轮、同名第二次创建失败关闭、本机真实 SID 哈希），日志追加在 reports/pv5-windows-ipc.log 的 WP3a 段（(W1)(W2)(W4)，WP2 段保留）。跨用户拒绝本机不可得，已按限制登记。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "c12957ee3c4ffdcab9db8533d23b42e4673107ba"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3a-handoff.md
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "Unix 侧的 endpoint/权限/凭据场景（R17/R19/R20/R21 的 Unix 部分）本机不可执行（无 WSL/无 Linux 环境）：本轮提供的是 (a) 全平台命名规则用例 local_endpoint_naming（4 通过）与 (b) cargo clippy --target x86_64-unknown-linux-gnu -p server --all-targets --all-features -- -D warnings 的编译+lint 通过（退出码 0）。真实执行属 Linux CI 的 [PV3] 轮（tasks.md 3.5/6.3/7.1），本轮不声称其为 PASS；日志 reports/wp3-server-transport.log 的 (2)(5)(9) 节。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.9"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "c12957ee3c4ffdcab9db8533d23b42e4673107ba"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3a-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP3a 轮：local_admin 的 13 个单元用例 + tests/local_admin_schema_drift.rs 的 6 个漂移用例在 Windows 本机执行（cargo test --locked -p server --all-features local_admin / envelope / --test local_admin_schema_drift，提交后再跑一轮），R33–R36 的信封规则逐条可见；漂移测试用 include_str! 读取 schemas/local-admin/v1/envelope.schema.json 与 fixtures/local-admin/v1/manifest.json（sha256 已记录在日志 (4) 节）。R35 的方法级参数校验属 WP3b，已登记。日志 reports/wp3-server-envelope.log 的 (1)(2)(3)(4)(6) 节。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.7"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "c12957ee3c4ffdcab9db8533d23b42e4673107ba"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3a-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "成员新增后的 [PV2] 轮：node scripts/check-crate-boundaries.mjs 在仓库根执行，退出码 0（crate boundaries OK: 11 个 crate ...）；logs 同时记录 cargo metadata 的成员清单（10 → 11）与 server 的依赖边逐条，证明唯一工作区内依赖边是 windows-local-ipc（§5 矩阵 server 行为 ✓，其余为第三方）。日志 reports/wp3-server-boundaries.log。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.7"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "c12957ee3c4ffdcab9db8533d23b42e4673107ba"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3a-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP3a 轮：npm run verify 在仓库根执行（10 道合同门禁 + cargo fmt --check + cargo clippy --locked --workspace --all-targets --all-features -- -D warnings + cargo test --locked --workspace --all-features，568 个用例通过、0 失败），退出码 0；摘要追加在 reports/du1-pv1.log 的 WP3a 轮，完整输出在 reports/wp3-verify.log。代码提交本身也经 .husky/pre-commit 的三步与 commitlint 通过。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.7"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "c12957ee3c4ffdcab9db8533d23b42e4673107ba"
    evidence_type: VALIDATION
    evidence_id: LC1
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3a-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "附加本地检查：cargo fmt --all -- --check（退出码 0）、rg -n \"unsafe\" crates/server/src（零命中，退出码 1；对照 crates/server 的 3 处命中全部是注释文本与测试函数名）、cargo tree --locked -p server --edges normal --depth 1（工作区内依赖只有 windows-local-ipc）。日志 reports/wp3-server-transport.log 的 (3)(6)(7) 节。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.7"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "c12957ee3c4ffdcab9db8533d23b42e4673107ba"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3a-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP3a 交付物：提交 c12957e 的 24 个文件（crates/server 的 manifest + 18 个源文件 + 5 个集成测试文件）、根 Cargo.toml 的 1 行成员新增与 Cargo.lock 更新；对 WP3b/WP4 冻结的形状见本报告「冻结的公开形状」（信封类型与错误码、FacadeAttachmentId/Registry、AuditHook、LocalAdminHandler、LocalEndpoint::bind/accept、serve_connection）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.5"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "c12957ee3c4ffdcab9db8533d23b42e4673107ba"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3a-handoff.md
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "tasks.md 3.5 是 WP3 交付前的 Project Verify，覆盖 WP3 全部（含 2.10–2.13 的方法路由与 WP3b 轮次），本 WP 只完成 2.7–2.9：本轮已给出 [PV3] 的 WP3a 部分与 [PV2]/[PV1]，[PV5] 的 WP3a 段亦已留证；3.5 的完整判定必须等 WP3b 交付后由主 Agent 重新派发，因此本行记 BLOCKED/PENDING。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.6"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "c12957ee3c4ffdcab9db8533d23b42e4673107ba"
    evidence_type: REVIEW
    evidence_id: RV1
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "WP3 的独立 review（按 roles/reviewer.md 检视 framing 关闭规则逐条、对端凭据校验不可绕过、信封 closed object 与 v 规则、凭据不进入日志/错误/Debug、无 unsafe 等）尚未返回；须由不继承本实现对话的执行者对 WP3 冻结版本检视，WP3b 交付后一并复核更经济。本报告不把自检当作其结论。"
    source_evidence: NOT_APPLICABLE
```

# Spec Delta

## ADDED Requirements

### Requirement: 版本化设备短二维码

设备二维码 SHALL 使用 `https://<canonical-origin>/pair#q2=<payload>`，payload 为无填充 base64url 编码的 65 字节数据：1 字节版本 `2`、32 字节扫码取回凭据、32 字节主机公钥指纹。指纹 SHALL 为规范 65 字节未压缩 P-256 公钥的完整 SHA-256 摘要。旧设备 `#data=`、未知版本、非规范编码或错误长度 MUST 明确拒绝，MUST NOT 降级；节点二维码格式不变。

#### Scenario: 短二维码不直接暴露配对秘密

- **WHEN** 解码本次创建的设备短二维码
- **THEN** 只能取得版本、扫码取回凭据、公钥指纹与外层地址来源，不含配对秘密、完整公钥或配对标识

#### Scenario: 真实长度与二维码容量

- **WHEN** 使用 `https://faraday.tail74842e.ts.net:8765/pair` 为前缀编码任意合法凭据与指纹
- **THEN** 新 URL 为 134 ASCII 字节且不超过 150 字节；相同 L 级纠错下二维码连同至少四模块白边不超过 58 列、29 行实心半块字符

#### Scenario: 拒绝旧格式且不影响节点

- **WHEN** 设备扫码消费方收到旧设备完整资料地址、未知版本或错误长度，而节点配对仍收到原节点二维码
- **THEN** 设备消费方明确报告格式不支持且不发请求，节点配对仍按原协议处理

### Requirement: 扫码资料取回端点

`POST /sync/v1/pairing/qr-lookup` SHALL 仅接受 `protocolVersion: 1` 与 `qrToken`（32 字节无填充 base64url）。有效且未认领时 SHALL 返回与配对码 lookup 同形的完整资料；重复请求返回相同绑定、秘密及到期时间，不消耗网页配对码，不创建信任。请求 MUST 携带与本机及配对绑定一致的 Origin，Host 必须合法；失败校验与并发状态检查 MUST 在交付资料之前完成。

#### Scenario: 响应丢失后取回相同资料

- **WHEN** 相同 qrToken 在有效且未认领期间重复提交
- **THEN** 每次返回相同 protocolVersion、pairingId、hostId、hostPublicKey、canonicalOrigin、pairingSecret 与 expiresAt，状态保持未认领

#### Scenario: 与认领并发时有明确先后

- **WHEN** 取回与任一入口认领并发
- **THEN** 取回成功必须能归于认领生效之前；认领生效之后开始的取回失败，不因内存仍有秘密而再次交付资料

#### Scenario: 原生请求必须声明目标来源

- **WHEN** 原生消费方发送请求时未带 Origin、来源不符或 Host 不合法
- **THEN** 服务端不交付任何配对资料；正确 Origin 本身不代替凭据与后续身份认证

### Requirement: 扫码取回的统一失败与资源边界

扫码取回格式、凭据、状态、过期或来源绑定失败 SHALL 统一返回 `401 pairing.qr_invalid`，MUST NOT 泄露具体差异或回显材料。非 JSON、请求体超限、方法错误、接入层 Host 拒绝和限流仍使用既有 HTTP 边界。全部响应 MUST 带四个既有安全头与 no-store；请求体上限为 16 KiB。限流独立于配对码与 claim，失败不增加配对证明失败次数。

#### Scenario: 不存在与已认领不可区分

- **WHEN** 分别使用不存在、已过期、已拒绝、已认领、已失效的扫码凭据或错误请求体
- **THEN** 均返回相同 401 错误信封与 pairing.qr_invalid，不含任何绑定或秘密

#### Scenario: 预处理错误同样不缓存

- **WHEN** 请求因大小、方法、Host 或限流在进入配对处理前被拒绝
- **THEN** 仍携带四个安全响应头与 no-store；429 含可用的 Retry-After

### Requirement: 扫码消费方先固定公钥再认领

原生扫码消费方 SHALL 固定二维码地址的 canonical origin，禁止跨来源重定向，先匹配取回公钥的完整指纹，再接受其余配对绑定并认领。取回资料的 canonicalOrigin MUST 与目标一致，字段与有效期 MUST 合法；主机证明、批准响应身份一致性、本机确认及每次 WSS 完整认证 MUST 沿用现有规则。已配对来源的公钥变化 MUST 明确失败，不覆盖原记录。

#### Scenario: 换公钥与换来源均停止

- **WHEN** 响应公钥指纹不符、返回另一来源或请求被重定向到另一来源
- **THEN** 消费方停止，不发送 claim、不建立信任、不自动改用配对码或旧二维码

#### Scenario: 指纹正确仍必须验签和本机确认

- **WHEN** 返回公钥符合二维码指纹，但 claim 主机签名错误或本机尚未批准
- **THEN** 前者明确失败，后者等待批准；均不能仅凭指纹匹配进入已认证状态

### Requirement: 扫码消费方的有限重试

扫码消费方 SHALL 对断网、超时与暂时服务故障进行有限重试，429 按 Retry-After 等待；已知过期时间之后 MUST 停止，任何重试 MUST NOT 续期。身份、来源、格式、凭据校验失败 MUST 立即停止且不降级。取回前尚未知 expiresAt 时 MUST 使用有限次数和总时限；重试耗尽 SHALL 提示重新扫码或重新生成。重试规则 MUST NOT 改变网页配对码的一次性语义。

#### Scenario: 有限重试可恢复丢失响应

- **WHEN** 首次响应丢失，下一次取回发生在认领前且有效期内
- **THEN** 消费方取得相同资料并继续，不创建第二次配对

#### Scenario: 未取得有效期也不能无限等待

- **WHEN** 从未成功取得资料且服务一直不可达，或 Retry-After 超出等待预算
- **THEN** 达到有限尝试或总时限后停止并给出恢复指引；不能因未取得 expiresAt 而无限重试

#### Scenario: 重试不改变已认领命令

- **WHEN** 消费方已经取得资料并成功认领，但认领响应丢失
- **THEN** 重试原认领内容并沿用现有幂等规则，不重新取回或生成另一设备身份

## MODIFIED Requirements

### Requirement: claim 端点的原子校验

`POST /sync/v1/pairing/claim` SHALL 原子地依次检查：该 pairing 存在、未过期、仍处于 `created`、请求的来源与 Host 与该配对登记时的绑定一致、以及 `pairing-proof` 域的 HMAC 校验正确。任一项不成立时 MUST NOT 写入任何状态。claim 成功后 SHALL 进入等待本机确认的状态，并 SHALL 只成功一次：已进入 `claimed`/等待确认或更后状态时，对同一 `pairingId` 的再次 claim MUST 被拒绝；内容完全相同的重复 claim SHALL 按幂等返回首次结果，内容不同的重复 claim MUST 返回冲突。认领载荷 MUST 携带对端公钥本身而非仅指纹。设备用于计算 HMAC 的配对秘密可以来自扫码资料取回成功响应，也可以来自配对码 lookup 的成功响应；两条入口进入同一套 claim 校验，不得出现第二套证明规则。

#### Scenario: 首次 claim 成功

- **WHEN** 设备用经扫码资料取回取得的配对秘密对 claim 载荷计算 HMAC 并提交，来源与 Host 均与配对登记一致，且该配对仍未被认领
- **THEN** 服务端返回该配对的请求标识与等待确认状态，配对进入等待本机确认，且尚未创建设备信任

#### Scenario: 配对码入口的 claim 与二维码入口等价

- **WHEN** 设备先用正确配对码取得配对绑定与配对秘密，再用该秘密对 claim 载荷计算 HMAC 并提交
- **THEN** claim 通过同一套原子校验并得到与二维码入口相同的状态转换，MUST NOT 因为秘密来自 lookup 而放宽任何校验

#### Scenario: 重复 claim 的两种结果

- **WHEN** 同一配对收到第二个 claim，其内容与首次完全相同
- **THEN** 服务端返回首次结果而不创建第二条配对请求；若第二个 claim 的内容与首次不同，则 MUST 返回冲突

#### Scenario: 校验失败不写入状态

- **WHEN** claim 的 HMAC 无效、或来源与配对绑定不一致、或该配对已过期
- **THEN** 服务端返回对应错误且 MUST NOT 改变配对状态、MUST NOT 创建信任记录或写出任何授权

### Requirement: 配对端点的固定限流

配对端点 SHALL 执行固定限流：claim 按来源地址限制为每分钟 10 次，lookup 按来源地址限制为每分钟 10 次，扫码资料取回按来源地址独立限制为每分钟 10 次，status 按配对标限制为每分钟 60 次；超限 MUST 返回 429。扫码资料取回、lookup 与 claim MUST 使用各自独立的额度，不得因为一次成功或失败的 lookup 消耗 claim 的额度，反之亦然。限流计数 MUST 以对端真实连接地址为准，不可信来源的转发头 MUST NOT 影响计数。屡次 claim/status 证明校验失败 MUST 使该配对失效；lookup 与扫码资料取回的失败不指向某一配对，不增加该配对的证明失败计数。

#### Scenario: claim 超限返回 429

- **WHEN** 同一来源地址在一分钟内发起超过 10 次 claim
- **THEN** 超出的请求返回 429，且 MUST NOT 被当作未限流的正常 claim 处理

#### Scenario: lookup 与 claim 额度互不占用

- **WHEN** 同一来源地址在一分钟内先做一次成功 lookup，随后发起一次 claim
- **THEN** claim 仍在其独立的 10 次/分钟额度内，MUST NOT 因 lookup 已被使用而被 429

#### Scenario: 不可信转发头不影响计数

- **WHEN** 直连对端（不在可信代理清单内）携带伪造的转发来源头并反复发起 claim 或 lookup
- **THEN** 限流按其真实连接地址计算，伪造头 MUST NOT 让它绕过限制

#### Scenario: 屡次失败使配对失效

- **WHEN** 同一配对的证明校验失败累计达到 5 次
- **THEN** 该配对失效，后续 claim 与状态查询按失效处理，MUST NOT 仍可被认领

### Requirement: 配对码 lookup 端点

`POST /sync/v1/pairing/lookup` SHALL 用配对码换取一个仍可认领的配对的公开绑定与 pairing secret。请求体 SHALL 只含 `protocolVersion` 与 `pairingCode`；`pairingCode` SHALL 是 8 位 Crockford base32（大小写不敏感，可带一个 `-` 分隔符，服务端先归一化再比较）。成功响应 SHALL 返回 `protocolVersion`、`pairingId`、`hostId`、`hostPublicKey`、`canonicalOrigin`、`pairingSecret` 与 `expiresAt`。lookup MUST NOT 推进配对状态、MUST NOT 创建信任，也 MUST NOT 返回任何授权集合。

#### Scenario: 正确配对码取得配对绑定

- **WHEN** 浏览器在配对有效期内提交一个仍可认领配对的正确配对码
- **THEN** 服务端返回该配对的标识、主机身份材料、规范化来源、配对秘密与过期时间，且配对状态仍为可认领

#### Scenario: lookup 不推进配对状态

- **WHEN** 一次正确的 lookup 成功返回
- **THEN** 该配对仍可被 claim 一次，lookup 本身 MUST NOT 把它标记为已认领或已批准

#### Scenario: 主机公钥与二维码载荷一致

- **WHEN** 同一配对既有二维码载荷、又被配对码 lookup 成功返回
- **THEN** lookup 与扫码资料取回返回的 `hostPublicKey`、`hostId` 逐字节相同，且该公钥的完整指纹与二维码固定值相同，MUST NOT 出现两套主机身份

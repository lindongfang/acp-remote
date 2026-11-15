# 请求身份/纯时钟代码独立检视

2026-10-31。独立reviewer `/root/baseline_reviewer`，只读项目、未参与产品修改。

结论：**限定静态代码检视通过，当前六文件未发现纯§3A范围阻断。** 独立request_tester的公开API执行另报；本报告不使用开发8测试或fmt/clippy代签验收，不签任务2.2整体完成。

## 受检版本

Git基线`f6d1f8df63673eeebd6109d4ddb3e95e5b66b906`加候选；六文件实际SHA-256均与交接一致：

| 文件 | SHA-256 |
| --- | --- |
| app/src/domain/requests.rs | 74373f501d5f012fbcd643218f0ae154235d248bcaf51938dc21a88c24036cb1 |
| app/src/adapters/product_requests.rs | 0e75f0d1dff02fa25b76f5e2fdab5c75dd2f38e76ef63115b34e11f150f5f3ea |
| app/src/adapters/product_requests/tests.rs | 4d93c1d06ab586c758a452ec8648cfaccb57f42e3882c2024c0707b869349141 |
| app/src/jws.rs | e41cf3fce86b89e519ff4d47c04201c3e29650209d067d363725febb5a45a872 |
| app/src/domain/mod.rs | 283a1c48233d6f30f0f2c667ebc44d0c95ec1514a917ee0511f8b197657596f2 |
| app/src/adapters/mod.rs | acc0267caab5567e9512cb3ab79db4dddaf98e0399201173c53acb076832ac5a |

依据为合同§3/3A、现行执行/审计spec、统一CL-018/079/082及独立请求阶段定义最终12行P矩阵（定义摘要`1560d9d0f737b44d8cc418f83f5d65c84751e406a54f986e07e21e128fad0df0`）。

## 静态核对

- 业务RequestIntent、身份、时间与JWS DTO分开；枚举封闭，不接受未知action/targetKind；平台ID使用原有规范128位编码，nodeId/摘要严格为小写64hex。
- JWS只新增独立ProductRequest用途；验证入口限定此用途并先验固定公钥签名，再校验精确claims。旧六类用途解析逻辑没有放宽。节点ID实际与给定固定公钥字节的SHA-256绑定；生产签名端口复用NodeIdentity，不读取远程私钥。
- Claims拒绝未知字段，原有ParsedJws严格JSON路径拒绝重复键；u64解码与安全整数上界、未来issuedAt、非正期限及超过24小时窗口均拒绝。默认签发checked加24小时，合法短签名期限保留，now等于exp即过期，不改写原身份。
- 请求代际与expected业务意图逐字段比较；verified包装不是dispatch permit。requestId及期限保留在返回身份，签发和验证均无SQL、网络、原生副作用或自动续签。
- RequestClock先检查启动墙钟不低于导入水位，墙钟/单调缺失或回退、加法和安全整数越界均置sticky故障；后续合法采样不能解除。有效时间取只增候选、墙钟和启动锚点加单调经过时间最大值，导入持久水位和候选水位分别返回，不伪造落库。
- 正常路径无unwrap/expect；错误文案不带token/正文/私钥，SignedProductRequest Debug仅显示编码长度。模块声明没有接WSS/派发，旧管理接口用途未改。

## 边界

RequestTime是可复制的本地采样，不是新鲜认证或持久事实。它的API文档要求调用者在每个issue/admission/dispatch边界重新采样；后续应用不能复用旧sample或在clock已fault后绕过重新采样取得危险资格。该调用约束尚未接线，本阶段不签其实际实现。

action对应目标映射及字段数组摘要尚未冻结；此阶段只验证已给定digest的形状与绑定，不构成完整产品摘要合同。去重、截止后不派发、持久水位CAS/COMMIT未知、容量、正常重启与隔离恢复都留待后续SQL/应用阶段。纯时间或验签成功不能替代认证、审批、基础重放和资源条件。

实际执行仅为代码/开发测试/diff读取与摘要核对，未运行测试或真实Harness。独立执行与完整工程回归需同版另取证；当前无新增静态阻断问题。

## 同版独立执行证据补充

随后收到request_tester证据，已实际读取`public-api-acceptance.md`、`final-manifest.json`、`public-api-test.log`及`execution.json`，不是仅凭报告路径或coder自评判断。六份受测源码与上表逐一一致；最终公开API探针12项全部ok、0失败/忽略，cargo实际exitCode=0，doc-tests为0不计功能覆盖。固定合成key、主产品path依赖、独立target及offline/locked执行边界与阶段定义一致，没有读取用户身份或启动SQL/WSS/Harness。

独立结果覆盖已审12行P矩阵，包括六用途双向隔离、规范claims、原始数字形式、精确期限及sticky时钟。其定义/代码/执行在同一已列快照下，故本纯§3A子阶段已有静态检视及独立API证据，无未关闭阶段问题。完整任务2.2的持久水位/容量/正常重启，以及CL-018/079/082的完整L5仍未验；不得据此勾为整体完成。

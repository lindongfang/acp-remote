# Chromium JWS 变异与动态互操作开发记录

日期：2026-10-30。产品源码固定在 implementation-r4 清单；本轮没有修改产品或测试定义，不能代签独立 NA-003 验收。

## 实际环境与结果

- ego-browser 独立 TaskSpace 6；Windows Chrome/156.0.0.0，isSecureContext=true。页面为本仓库 Vite 提供的 http://127.0.0.1:4176/tests/browser/jws.html。
- 初次在沙箱4175启动Vite无法从宿主浏览器访问，出现真实连接超时；停止该服务器，改在宿主4176启动后通过。未修改浏览器证书设置、系统代理或用户资料。
- 原始页面7项全部PASS：Rust固定签名、WebCrypto固定签名、high-S、浏览器签验、篡改拒绝、不安全整数拒绝、非法SEC1拒绝。
- 对实际 src/protocol/jws.ts 的浏览器导入再执行25项负向变异，全部按预期抛出JwsError：顶层/嵌套重复键、非对象、孤立代理项、浮点表示、指数表示、不安全整数、非法UTF-8；未知header/jku/crit/none算法/跨用途；5种不规范base64；压缩/截短/错误前缀/不在曲线公钥；DER、零标量与r越界。
- 使用仓库公开固定fixture的测试私钥，在本轮Chromium生成新ES256签名；真实Rust `claims::verify_auth_client` 对该新证明验证成功。不是仅重复验证历史静态浏览器签名。
- 浏览器空间已finish，两个自有Vite进程均已停止；不触及用户30447、其他Agent的浏览器空间或节点。

## 复核资产与边界

- .tmp/jws-matrix-20261030-01.mjs 至 -03.mjs 为真实执行的逐轮脚本；末轮已结束空间，不可直接重用原space ID。
- .tmp/jws-matrix-20261030.json 为实际浏览器状态与25项逐项结果。
- .tmp/jws-browser-generated-20261030.txt 仅含公开测试fixture生成的证明，不含生产私钥。
- .tmp/jws-interop-20261030 为本次Rust验证器：`cargo run --manifest-path .tmp/jws-interop-20261030/Cargo.toml --offline -- .tmp/jws-browser-generated-20261030.txt`，退出0，输出当前Chromium签名由Rust验证通过。

这是协议编码/密码学边界验证，未使用真实产品TLS、DPAPI或SQLite，不替代管理认证完整变异矩阵。独立reviewer已获原始资产路径，最终采纳范围由其判断。

## 补齐标准编码与标量边界

依据独立检视指出的具体缺口，另执行标准编码合同验证。使用Rust现有P-256库生成真正DER签名、有效的33字节压缩曲线点及normalize_s得到的低S签名，而非以全零数组代表这些合法编码。

实际Chromium新增13项全部通过：库归一化低S与固定high-S可用；合法DER签名因协议仅接受P1363而拒绝；有效压缩点因协议只接受未压缩SEC1而拒绝；r/s分别为0、精确曲线阶n、n+1时均拒绝；合法节点签名payload正对照通过，带未知字段的同节点有效签名payload被Web验证器拒绝；Rust也拒绝有效客户端签名中的未知payload字段。节点proof时间仅为此边界检查注入fixture时钟并在finally恢复，不宣称线上新鲜性认证通过。

原始资产：.tmp/jws-extra-20261030（Rust向量生成器）、.tmp/jws-extra-20261030.json、.tmp/jws-extra-browser-20261030-01.mjs/-02.mjs、.tmp/jws-extra-browser-20261030-result.json。独立浏览器空间7已结束、自有4176服务器已停止，未修改产品源码或身份。此13项仍是开发执行证据，交独立角色复核后才能决定整例状态。

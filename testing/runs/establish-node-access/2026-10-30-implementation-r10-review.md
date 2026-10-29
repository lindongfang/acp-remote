# implementation-r10 独立定向检视

日期：2026-10-30。结论：**PASS（已重建并校验的r10固定副本及限流/permit子断言）**。未发现需修产品问题，不代签全部NA005/002或整体待归档。

## 版本异常与独立固定副本

清单 `2026-10-30-implementation-r10.sha256` 自身摘要独立核对为 `C2B1B9B6C021991952F7C2DE27696D8557874B31493C10E6584C679E58B967A1`，83条。初次工作区核验为**82匹配、1不匹配**：trust.rs清单值 `2084AAB9DBE3919149918F120BBC86863DA9284D9D45030ADF7CD39EEA621849`，当前实际为 `CABDFB0166D1E4DFEE646A347039B9EB1F5DD69F06413441141D1C72DD6CFBB7`。已即时报告，不将混合工作区写成r10全匹配，未改动该文件或旧清单。

从现有 `.tmp/current-source-scan-r7-20261030/app/src/trust.rs` 找到摘要与r10清单逐字一致的原文件，在reviewer自有 `.tmp/reviewer-implementation-r4-target/r10-snapshot` 重建全部83项，每一项复制后重新计算SHA256，**83/83匹配**。其余82项取自当时匹配的当前文件。编译所需Web dist另由当前构建副本提供，不称其属于83项冻结集；本轮限流/permit断言不依赖其页面字节。所有源码以此独立副本编译，不使用当前已漂移trust.rs。

本轮改动目标摘要：pairing_http.rs `95B2BF85049FADE8A47102EC5607BD7412434517882D9A42E917649A44A861D5`；management_ws.rs `4B3FCF3B98B9D75D7CB20FB3C9CDD9061BA87CA61A6A1A791270447B28389A90`。原工作区、用户30447/身份及正常浏览器未操作；未提交推送归档。仅新增本报告，不修改产品/规格/其他报告/清单/plan/tasks。

## 实现检查

HTTP新增两项位于cfg(test)模块。四端点使用同一真实RateLimiter.check_at：每来源额度耗尽、59999ms仍拒绝且Retry-After=1、60000ms按原始deadline恢复，计数归1；拒绝没有滑动延长期限。它是注入Instant的同步真实限流器断言，不是实际等候60秒或真实1024个TCP来源。

`source_bucket_capacity_cannot_evict_active_sources_and_prunes_at_five_minutes` 建立1024个真实map项，允许初始部分调用因全局额度拒绝，但随后在60秒全局窗口重置后测试第1025来源拒绝；明确检查global count=1和新IP未插入，**不是把global429误算成桶满**。旧IP继续有额度且被耗尽后仍受限，尝试新IP不能清掉旧配额；299秒仍满，300秒清掉闲置旧桶，保留60秒时访问过的旧IP加新IP，共2项。桶容量与最后访问时刻处理符合现实现。

Management新增握手桶测试采用相同隔离：60秒global count=1时拒绝1025来源；旧IP预算保留，299/300秒精确闲置边界。握手/CommandWindow测试明确59999ms拒绝、60000ms恢复；普通accept委托accept_at(Instant::now())，原判断、saturating加法及计数顺序未变，无生产可控时钟开关。

Outbound测试用真实serde序列化、mpsc和Semaphore：32个32768字节JSON先用完1MiB而非128帧预算；释放一帧后归还32768 permit并可重入队；drop receiver后全部permit回收。另一组小帧先达帧上限且仍有字节余量；失败try_send归还暂借permit，取出一帧后槽可复用，drop全部队列后permit全回。不是只检查常量或把帧预算误认为字节预算。

## 独立执行

在上述83项已验证副本，以reviewer独有 `r10-build` target离线锁定构建：

```text
cargo test --manifest-path <r10-snapshot>/app/Cargo.toml --target-dir .tmp/reviewer-implementation-r4-target/r10-build --offline --locked --lib pairing_http::tests -- --nocapture
9 passed; 0 failed; 127 filtered out

<同一测试exe> management_ws::tests --nocapture
7 passed; 0 failed; 129 filtered out
```

两组均实际exit0；第一组由真实用户执行以避免SQLite隔离目录权限失真。没有复用主方target、没有把开发Clippy或全check记录为独立执行。没有重跑全部产品测试，原RV001/002及CON/LIFE/SC/I01关闭结论保持。

## 有限范围与下一包交接

本次支持固定窗口、1024桶不驱逐活跃来源、五分钟闲置回收、帧/字节独立预算及permit回收子断言。网络来源伪造、真实WSS超限关闭码、并发队列压力等按当前用例现有证据复用或保留具体缺口，不从本次同步限流器测试自动推导全部线上通过。

NA002整体供应链/密码库维护许可平台漏洞及合同§1–7映射是下一独立包，本报告不提前签署。现有官方扫描工具执行、真实提交lint、Chromium/JWS和旧安全检视应按锁文件/作用字节/原始证据明确复用；522文件零命中只绑定当时副本，不能称扫描过后来新增资产。未查询yanked、未作全部许可证法律意见属于范围限制，不能无依据变成新门禁；真正缺口须定位当前NA002必要断言和未取得的具体来源/证据。工作区trust.rs漂移本身已被精确副本解决，不据此要求回滚用户后续修改。

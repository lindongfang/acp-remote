# P0 v1可空messageId独立代码检视

2026-10-31。只读产品及开发资产；未改任何产品/独立断言。依据根833c1124合同及4a16aa62独立七组定义，检视根`2026-10-31-p0-v1-message-id-source.json`全部7项，重新SHA256均一致。

**P0限定交付通过，未发现本轮具体阻断。** 已完成静态审查及同版独立七组/原17+12原始证据复核；下文阶段等待记录仅保留过程归属，最终结论见末节。不以开发ACP36/clippy通过替代独立执行。

| 文件 | SHA256 |
| --- | --- |
| strategy.rs | 72ad187e25cf6ba4c530be4fb8eaa6c482fcca9bab32c3a3220b4fdd42842e4c |
| tests.rs | b5b2c180e2d46b00101eb16da6fe0cce0783da89a065bbf839274399bdf212e9 |
| tests/message_id.rs | 0f257a391a9f9154b3bf2cafc9f5337c651976e2c9aa0498f84032c050fed2d4 |

model a14f33cf、transport93bd37e4、Cargo.lock87f27b61和合同833c1124仍原样。实际完整读取新增开发测试及策略消息分支。为确认没有别处生产修改，在内存中仅撤回新`.filter(|value| self.version() != Version::V1 || !value.is_null())`及其机械换行，重算整个文件恰为旧cb91ed1d；仅删除新增`mod message_id;`后tests.rs恰为原b161e952。没有写临时还原到根、没有Git索引操作。

该filter只过滤实际selected Version::V1中的Value::Null：v1省略本来None，字符串仍经原MessageId::parse，数字/布尔/对象/数组与4097字符串仍错误；v2所有存在值仍经原parse，省略粗投影None保持原状，不顺便收紧或放宽。没有raw复制/重写、协商重试、聚合或连接状态副作用。model/transport不变，因此原类型化session/RPC/origin错误边界未被本次实现改写。

开发测试经真实duplex initialize/Connection::next覆盖三tag基本矩阵、offer2 selected1、UTF8边界/错误类型、v2现行为及真实v1 replay窗口后继续读取；这些只作源码输入证据。独立定义还包含更完整ASCII/UTF8、float、DuringResume/other-session、raw空白等边界，需原tester实际执行，不把开发四函数当七组完整验收。

本报告只覆盖P0窄修，不依赖/关闭VPROJ-PLAN-002/003，不签完整v2结构适配、P1/P2、可信轮次、Domain额外API或真实node/Harness/WSS。新独立证据到齐后另追加同版最终限定结论。

## 独立证据阶段补读

已完整亲读原tester外部p0-message-probe/tests/message_id.rs的6函数与公开双工辅助代码，以及p0-message-tests-final.log和test-final-exit.json：新6+原发现17+原权限12=35/35，实际exit0。三个v1 tag的省略/null/普通及空串、ASCII/多字节UTF8 4096/4097、整数/小数/布尔/数组/对象独立错误连接、offer2选1、raw空白/字段及顺序、同目标Live/DuringReplay/DuringResume和另一session Live、响应后继续均有实际断言。v2保留粗省略None、合法字符串/空串与错误形状边界，不把该回归解释为完整v2内容校验。

阶段见过第二次夹具失败：ordinary strategy.resume实际session/resume却被新fixture期待session/load。已亲读源码实际request选择、原不变Strategy.resume和§6A，确认普通resume与replay本来分离；当前fixture只把mode1严格期待load、mode2严格期待resume。第一轮另缺少resume广告，由tester补合法既有capability。没有产品修复或降低origin/raw预期，原attempt1/attempt2及exit101保留；最后35重新真实执行通过。其余前后source/锁、clippy/完整报告尚待交，当前不提前改最终交付状态。

## 最终同版限定结论

已实际完整读取原tester正式`p0-v1-message-id-acceptance.md`并重算SHA256为`d38493fc56e0a10301fc69c65d0d53cf1850ce7bca5e755f5edad433abd2b10c`。新6函数的矩阵及原17发现/12权限共35/35、0失败/0忽略；另根开发ACP36/36、普通依赖严格all/pedantic/nursery clippy与新资产fmt检查通过。已亲读最终运行/lint/开发回归日志和对应exit JSON，均实际0，不从PS正常stderr包装推断。

实际p0-after7项与冻结清单一致；新公开测试73580d598dd79b7b43c7a33262511e2df57c691240198debfe5b1a458ef1e8c4，原17与12副本分别重算仍48ac5cb99352caafa01d1294304e67a0c46d81cb98a71daa349ff739d123cbca、cccda47e932366e486ec54400e320cf9973a9d8ede5f4778d425bc21aaf2b276。源码唯一filter/单mod的整文件还原摘要证明见前文，生产没有因两次fixture错误再修改。

本reviewer实际解析两Cargo.lock按name/version/source/checksum比较：产品261，probe238，共237项完全一致，只多probe自身并少24未用开发图项。probe直接tokio full/serde_json与产品现有配置相同，无新第三方版本或强制cfg(test)。新资产一处冗余closure改同方法引用的lint修正不改变预期，最终35及lint已在最终源重跑；旧错误日志保留。

据此仅v1合法null消息ID入口子阶段可收口。新独立6项当前仍外部可持续probe，若随后入仓须按实际根集成再核数量/差异/严格检查，不冒称根完整入口已含它们。此结论不签完整v2 required-ID结构校验、P1/P2、VPROJ-PLAN-002/003、整任务3.2、真实owner/WSS/Harness或完整L5/L6。

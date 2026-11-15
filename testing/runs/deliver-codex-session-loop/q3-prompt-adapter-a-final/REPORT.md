# WP4-A 正文数据适配独立验收：限定通过

仅签候选 `cbc8120e4a0386f55a78852668c3b2cc3cfd8b3a3d432d958c5fad2a72878c14` 的PA01–08数据/协议/R3子范围。新资产正式源452项：候选450中lib仅加获准三行cfg(test)挂载，另两真实测试叶。没有产品修复、Root合入或完整CL通过声明。

结论依据：公开7/7（同源复用本tester先前实际运行），最终私有5/5；固定旧448与候选在**精确产品依赖锁**下各70条真实握手/Strategy观察完全相同，其中每库8次实际prompt wire成功、62次预期错误；普通依赖7负例均在预定行/错误码拒绝，3次正控制成功。默认/全features alltargets check和严格clippy、fmt最终实际均0。

## 版本、独立性与挂载

- 已亲核作者freeze全部545成员、450源码及4path补丁。自有448副本真实patch check/apply均0，后450逐SHA同。产品改动仅原mod/strategy与新prompt_input、开发叶；完整上传parse、SHA、Retain以实际生产Arc adapter执行，未借旧devFixture validator。
- 本tester新增 `app/tests/prompt_document_acceptance.rs`、`app/tests/acceptance/prompt_document_private.rs`，以及原lib的三行cfg(test)挂载。普通正负probe从默认普通dependency验证，不依赖cfg(test)导出。当前workspace实际452文件，见effective-source-before及source-after-verified.json，产品候选545成员与旧设计468成员结束时仍exact。
- formal-v2相对首执行452仅private资产一项变化，见delta-from-first.json：原出站builder起的四JSONWS观察范围，以及通过原driver真工作key+failure观察补Direct前提。原7公开测试字节不变，故其7/7及普通类型证据复用；没有为了形式重复运行。
- 原442定义、777编码门、a766第一次精度门、387363第二精度附录及4faf门均保原件。原v1源码/首日志和本包first-phase副本永久保留，不将旧3/5改成5/5。

## 逐组实况

|组|限定结果|实际证据与限制|
|---|---|---|
|PA01|通过|普通data/validator/retainer/native关联类型正控制；私有构造E0451、字段/摘要E0616、缺方法E0599、错误Raw与grant混用E0308、Body构造E0624。7份负例共8个准确诊断；其中actor/proof两行各一。见first-phase/ordinary-type-probe。不能据此说所有public generic factory都无法选假validator|
|PA02|通过|完整上传含外围WS、独立全bytes SHA与root SHA区别、同字不同WS/转义/键序摘要不同；parse移动Vec指针、Body/Retain原根同地址、Drop后仍可读。实际生产adapter参与R3；不把地址观察冒称最终物理释放或上传上游来源证明|
|PA03|通过|空array/五已知block/未知、resource原text/blob宽度、必需字段错型、UTF8/JSON/Unicode/decoded重复键及未知深层；512层未知与大数词法保真。错误仅InvalidMessage，不生成sealed数据后进入R3|
|PA04|通过|旧库/候选各70真实握手向量；sessions、extra、array、逐block tag、媒体cap先于字段、早block先报错、V1未知拒绝/V2未知保留、offer2选1。字段同类InvalidMessage访问顺序和tag单次解码另作逐句提取审读，未虚报动态区分|
|PA05|通过|新PromptDocument经真实Service validate与生产Retain送入原Strategy/Connection/writer，V1及V2响应实际关联、IO joined；V2文字载荷33MiB，实际完整wire断言>32MiB且原结构/数值/转义/正文逐byte对照。未逐帧打印具体长度，不能伪造动态度量；出站仅允许四JSONWS变换，原上传/摘要/Retain不变。无真实Harness|
|PA06|通过|真实独占SQLite与原QueueService register/consume/query/inspect；同ID同完整bytes重试原事实、不同外围WS/转义/正文Conflict且原队列/版本不变，新ID主动追加同文字独立；无Run伪造。parse失败时R3调用计数及登记行均0。authority/environment为合成端口，不是Node授权|
|PA07|通过|原prepare_capture→select_capture真实旧编辑拒绝/原值保留/重新准备/选择后取消拒绝；Direct由原真实driver key和同owner失败观察建立Q002前提后，真正DirectSelected及capture_direct保digest/origin/原根地址。未手造资格/token，不冒真实Native失败或撤销链|
|PA08|通过|最终fmt/default及allfeatures check/strict实际0；原450差异白名单及静态错误来源审读；实际RawJsonError各类别映射与无正文错误。Allocation只静态enum转换，未执行物理分配失败/系统OOM|

## 命令和证据

所有实际命令、cwd、起止时间、真实exit在execution/commands.json与execution-fixed-lock-2/commands.json；Windows 1.98.1，既有CargoHome，离线，无下载/新产品依赖，独占formal-v1/cargo-target仅本tester复用。精确环境见environment.json；formal/probe继承原产品rust-toolchain.toml，不改全局默认工具链。

最终执行：`cargo fmt --manifest-path app/Cargo.toml --all --check`；`cargo check/clippy --manifest-path app/Cargo.toml --all-targets --offline --locked [--all-features]`（clippy追加`-- -D warnings`）；`cargo test ... --lib prompt_document_acceptance:: -- --test-threads=1 --nocapture`。公开7、外部普通正负编译的实际记录来自first-phase同源资产。精确锁probe先离线check写入仅probe自身包条目，逐包验收后 `cargo run --offline --locked`；测试源相同，各70条JSON逐byte相同，见execution-fixed-lock-2/differential.json。

原开发/回归证据依法核同复用，不冒独立新执行：4开发、ACP37、IO13、queue23、RawJSON33、capwire5。其日志/exit/二进制SHA和每组前后450源集合已逐项核；部分JSON清单只因键序造成文件hash不同，未说JSON原字节同。见first-phase/reused-evidence.json。其它Native/Shared运行时红不关闭。

## 首红、修正与依赖图警示

|ID|原结果|本次关闭界限|
|---|---|---|
|WPA-ENV001|普通sandbox ring构建时MSVC lib.exe启动1114，未到验收代码|同命令/同target Windows owner有限一次复核实际0；后续同owner执行。没有清target/改系统设置|
|WPA-STYLE001|新tester资产8项严格style lint|仅等价iterator/let-else/四WS字节串；一个仅测试长I/O用例局部too_many_lines说明，保完整cleanup；最后strict0，无产品allow/断言变化|
|WPA-OBS001|原private PA05在Request.params要求格式WS原文而失败；旧448同原probe也实际101|原raw_model::object到encode的完整既有出站链由4faf门明确；新独立四WS scanner+正确锁旧/新70实际同，Retain全bytes责任不变。原观察probe也用了下述错误Tokio配置，仅保历史复现，不作为最终固定依赖证据|
|WPA-FIX001|Direct fixture普通Ready首项实际Queued|原Q002只有失败挂起后新输入直派。用真driver key/failure补前提，最终强DirectSelected/capture原断言实际通过，不改产品|
|WPA-LOCK001|首两组70虽全同，但tester把Tokio钉成toml下限1.53.3而非实际锁1.54.1|首结果不计固定依赖证据，原件保留；仅probe manifests按锁实际版本重建，原测试源/预期不变，最终各70重跑0、共享237依赖精确同。中间metadata --no-deps未写probe自身锁条目导致--locked在运行前拒绝，亦保原件；无产品或功能红|

最终probe版本为**Tokio1.54.1**，SHA2 0.10.9、serde_json1.0.154、futures-util0.3.34。两seq probe及普通类型probe共享237个name/version/source/checksum条目均同产品锁，只加各自probe包；没有把锁文件字节不同说成依赖漂移。首wrong graph与未启动的locked阶段不得并入通过数。

## 资源与未验

所有最终私有用例都实际断言queue/IO owner joined，测试进程已正常结束；Windows owner只读CIM核本tester独占target可执行路径残留0，不扩大为整机/真实Native进程树证明。首次sandbox CIM查询失败的原remaining空表不作证据，见cleanup-sandbox-correction.json及最终cleanup-owner-scope.json。关闭的合成SQLite有意保留供证据核对，未递归删除用户或target目录。

当前Root额外15输入中仅plan在本轮前后变化，已亲diff为其它阶段状态及本A门槛/进度，不改本A规则；原快照和当前副本分别保留。原CL/EP与已冻结用例未编辑。

**未验**：真实分配失败/系统OOM恢复、上游实际分块上传的来源/取消/摘要接线、Node装配与受理转交/权限撤销、WSS/浏览器、真实Harness/Windows原生树，以及完整CL的L5/L6。公开PromptDocument只封闭该Vec数据，不提供资格或保上传来源；不把本A通过当Root或整体WP4通过。Root/作者/旧冻结均未写。SAFE_FINAL。

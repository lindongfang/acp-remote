# C开发测试维护候选 v2

只修改四个开发测试叶，799个其他源输入逐字保持。将类型检查函数提到模块、两处match改let-else、保持gate跨finish_catalog后显式drop、直接Vec::as_slice和字符pattern、提取固定CatalogFact构造并把profile与target两组原断言分成两个测试。生产实现、独立用例、恢复guard、锁/取消/close语义不变。

实跑cargo离线/locked：C开发22通过（原21拆成22，断言保留）；all-targets/all-features check和fmt check退出0。源码前后清单同ac1290c60f5c995664887d7c93394b909554bafb6b7512984a4e09368c812d19。严格clippy仍101，695→687，精确诊断比较新增0/移除8；剩原603加84未装配核心dead_code，不宣称strict通过。首次105行函数告警和此前日志保留。

维护patch 04a2b4fbbecd3de2ffa4c9d52f934dd874af0455dc3cf06f56882da83762bd90，在冻结v1前像以--whitespace=error-all执行apply-check退出0；此前误用error=0无效选项退出128，没有应用。候选仍隔离803源，Root639未改。不是独立C46/F/schema9/D或完整产品通过；清理/启动问题保持Open。

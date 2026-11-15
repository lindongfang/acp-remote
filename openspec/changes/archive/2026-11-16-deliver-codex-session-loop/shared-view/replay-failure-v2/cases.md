# 可执行断言（设计，执行0）

| ID | 原83映射 | 实际路径/断言 |
| --- | --- | --- |
| RF01 | SV038/046 | 真RemoteError早于flush终态；退view candidate、旧H Stale可鉴权读；原Ticket未安全终态不发第二RPC。 |
| RF02 | SV038/046/049 | 原reservation真实Zero/原write fence拒；probe确认，failure不造Received seq，下一安全op可开始。 |
| RF03 | SV038/046/049 | 真部分写后IO错/Unknown且无完成；保accepted/Unknown及失败段，退view不等于释放协议read资格。 |
| RF04 | SV041–046 | 双版本真Replayed早到、无gap prefix后flushErr；完成段/原request保持Complete，freshness可Stale。 |
| RF05 | SV011/018/023/029/038 | 旧/failed/后续Live三段重复相同native ID/文本；真auth+真peer逐段原文散列，旧anchor不被跨load更新，Live立即可读。 |
| RF06 | SV033/034/038/046 | 无旧current首次失败，部分/空内容及限制可读，不造旧cache/丢唯一raw/伪Complete。 |
| RF07 | SV006/029/039/041 | 失败后新op只一次RPC/provider；新Complete仅原RPC窗口，旧failed/live不清assignment混回。 |
| RF08 | SV004/006/011/041 | reserve→bind间真Replay queued帧和更早Live/Resume；原reader cut/pending窗口匹配，旧观测另段首开可见。 |
| RF09 | SV005/029/039 | 旧scope/op迟failure与新bind交错，只更新原段，不退新candidate或按当前指针猜窗口。 |
| RF10 | SV030/031/050/051/064 | 撤销/换scope/SourceSeal及通知缝；逐块/最后send拒旧资格，段metadata不铸grant，旧H/Prepared不复活。 |
| RF11 | SV054–056/059 | 新段映射/marker/metadata/chunk受控失败及取消，原raw/op/任务槽仍归owner，无quota/TTL/RPC兜底，不声称物理OOM。 |
| RF12 | SV032/038/039/049 | 多client join/desired_refresh/取消，首次pin不漂移；显示零provider/idle/RPC，新实际op重新取预算且旧原物先安全终态。 |

所有输入从真实ACP窗口/Reservation/Received/WriteProbe/真实认证夹具取得，无Scope/Actor/Loaded/flush/完成flag setter。RF08新增原reader窗口端口须对照真实receipt-cut/pending accepted，不拿手写Raw或作者map冒window。L4/L5未装配分支如实未执行。

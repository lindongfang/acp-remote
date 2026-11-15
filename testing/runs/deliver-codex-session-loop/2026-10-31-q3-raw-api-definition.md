# Raw JSON 基础层独立验收定义（编码前）

2026-10-31。仅定义：未编码、未运行、未签产品通过。WP2 R1/R2/R3 正式验收优先。唯一接口合同为 D:/Project/acp-remote/openspec/changes/deliver-codex-session-loop/acp-raw-json-contract.md，SHA256 9f2b236f325a0cceed3c1c34274d73ca953d9c7dfec571d22745b2b7ef70cce6。关联 CL-095、CL-103 及当前 acp-platform-constraints / structured-history；并非这两条的完整产品验收。

本阶段只检验公开 acp_remote::adapters::acp::raw_json 基础层。不接 RawMessage/transport/缓存/writer/真实 Harness，不声称实际32MiB帧入口已移除。旧API和独立资产保持原字节；管理认证 strict_json 八层规则不改。

## 执行办法与证据边界

所有内容确定性生成，不用私人数据。小样本比较固定原字节/固定目标，大样本比较完整bytes、片段、元素顺序及全长，不只比较摘要或截断快照。不以 serde_json::Value 递归重解未知子树作为oracle。深层成功/失败、clone/drop在自有子进程执行；子进程内用线程Builder指定256KiB栈，正常返回并join才通过。超时负责收敛测试进程，不成为产品容量或性能门槛。崩溃/溢栈/超时均不是通过。

线性扫描复杂度、平面节点、共享索引、无递归拥有结构需固定源码静态核对；耗时比值不能证明复杂度。公开API无法可靠且安全制造真实Allocation失败：动态仅检查该错误变体及静态文案；源码核对显式try_reserve映射、checked计数，真实分配失败动态证据单列not-run。不制造OOM或生产setter。

## 全公开接口映射

| API/trait | 必须核验 | 用例 |
| --- | --- | --- |
| RawDocument::parse(Vec<u8>) | 任一完整JSON值、UTF8/语法、顺序、Unicode/重复键、规模 | RJ01–04、RJ12–26 |
| bytes/root/root_owned/Clone/Debug | 原字节及根片段，文本共享、owned存活、脱敏 | RJ01、RJ07–10、RJ25、RJ28 |
| RawValueRef Copy/Clone/kind/fragment/object/array | 类型、原片段边界、不匹配None | RJ01–02、RJ05–06、RJ29 |
| string/boolean/integer/unsigned/is_null | 仅按正确类型读取、精确整数词法/范围、字符串解码 | RJ02、RJ11–17 |
| RawValueRef::to_owned/compact/Debug | 共享存活、四种JSON空白、词法保真、脱敏 | RJ08–10、RJ25、RJ28 |
| RawObject Copy/Clone/len/is_empty/get/iter/Debug | 解码key、原顺序、精确双端迭代及生命周期、缺键 | RJ05、RJ18–20、RJ28–29 |
| RawArray Copy/Clone/len/is_empty/get/iter/Debug | 原顺序、精确双端迭代、越界和空数组 | RJ06、RJ21、RJ28–29 |
| OwnedRawValue Clone/as_ref/Debug | 脱离原变量后仍保完整结构，深clone/drop | RJ07–08、RJ25、RJ28–29 |
| RawKind 六变体；Clone/Copy/Debug/Eq/PartialEq | 精确类型和编译trait检查 | RJ01–02、RJ29 |
| RawJsonError 五变体；Clone/Copy/Debug/Eq/PartialEq/Error/Display | 错误分类/优先顺序/静态文案、不泄密 | RJ03–04、RJ15–20、RJ26–29 |
| 两种iter: impl ExactSizeIterator+DoubleEndedIterator+'a | next/next_back/len/size_hint；临时句柄不缩短文档借用 | RJ05–06、RJ29 |

## 固定用例

以下30行是稳定定义ID，均待执行。循环样本不虚增测试数；未来实际Rust函数名、分支与结果另表映射。

| ID / 拟定测试名 | 精确操作与输入 | 固定预期 |
| --- | --- | --- |
| RJ01 six_roots_and_exact_original_bytes | object/array/string/number/true/null各加space/TAB/CR/LF首尾空白；包含中文/emoji | bytes全等原Vec；fragment仅去值周围空白；六RawKind对应；根不强制object。 |
| RJ02 typed_accessors_never_coerce | 六根分别调用全部类型读取器，另false、"123"、"true" | object/array/bool/int/uint不匹配None；string非字符串WrongType；仅Null的is_null真；不转字符串/浮点到整数。 |
| RJ03 invalid_syntax_utf8_and_trailing_data | 空/仅空白、01/+1/.1/1./1e/NaN/Infinity；缺分隔符、尾逗号、不闭合、raw控制字符、非法escape、两根或合法值后垃圾；0xff/截断UTF8/overlong/UTF8 surrogate；字符串外NBSP/U+2028 | 全InvalidJson，无部分文档；Unicode空白不能当JSON空白；错误不带原文。 |
| RJ04 full_syntax_validation_precedes_scan_errors | {"x":1,"x":2,}；["\uD800",]；重复key后第二根；Unicode问题加非法UTF8尾 | 完整UTF8/库语法校验优先InvalidJson，不因前面发现扫描类问题抢先返回。 |
| RJ05 ordered_decoded_object_and_exact_double_ended_iter | { "z":1,"\u0061":[2],"é":3,"":4 }及{}；get a和缺键；混合next/next_back并逐步len/size_hint | 顺序z/a/é/空串，key解码且不排序；值fragment精确；空len0/is_empty真；反向序固定、双端不重复不遗漏。 |
| RJ06 ordered_array_bounds_and_exact_double_ended_iter | [1,{"k":2},[3],"x"]与[]；get0/3/4/usize::MAX；双端混合 | len4、顺序不变；越界None；空数组正确；每次len和size_hint精确直到0。 |
| RJ07 document_clone_and_root_owned_share_storage | 大document clone，root_owned及其clone；比较bytes与fragment指针；先drop两个document再读owned | document文本地址/长度相同，owned根完整可读且共享源文本；索引共享另静态核对。 |
| RJ08 nested_owned_outlives_every_original_handle | {"outer":[{"未知":[1,2,{"x":3}]}],"other":0}深取未知子树to_owned，离开document作用域，再get/iter/compact | 原子树原片段/结构保留，源片段地址一致；不转字符串/丢未知字段；owned明确延长整文档存活。 |
| RJ09 compact_only_four_json_whitespaces_outside_strings | 多行/TAB/CR/space；字符串含escaped n/r/t、literal space/NBSP/U+2028、反斜杠/quote/括号；数字-0、1E+09 | 与固定逐字节目标全等；只去字符串外space/TAB/CR/LF；保escape、数字词法、字段序；无新literal LF。 |
| RJ10 compact_nested_fragment_roundtrip_keeps_input | 取父对象内数组子片段compact后重新parse，再查原bytes | 不带父层分隔符/周围空白；原层级与内容完整，原输入未改；不经Serialize或全树Value重编码。 |
| RJ11 strings_all_escapes_and_scalar_pairs | 合法quote/backslash/slash/b/f/n/r/t/u0000；BMP边界；literal Unicode；D800+DC00、DBFF+DFFF及emoji pair | string返回正确完整scalar及NUL等解码值；fragment/compact仍保原escape；引号/反斜杠/括号不误改结构。 |
| RJ12 unknown_numbers_keep_arbitrary_lexical_size | 4096位整数、负大整数、1e999999、-1.234567890123456789e-999999，根与未知成员 | 成功且Number，fragment/compact全词法原样；不强转f64、不近似/截断/变null或string。 |
| RJ13 signed_integer_exact_boundaries | 0/-0/1/-1、i64 MIN/MAX及越界1、u64 MAX、1.0/1e0/1E+0 | integer只在十进制整数词法且i64范围Some；-0=Some(0)，浮点/指数/超界None；原Number片段保留。 |
| RJ14 unsigned_exact_boundaries_and_negative_zero | 0/1/u64 MAX/MAX+1、-0/-1、1.0/1e0、超长整数 | unsigned只接受无负号且u64范围整数；-0明确None，其余不匹配None。 |
| RJ15 invalid_unicode_values_and_unknown_descendants | 根和未知深层中的孤立D800/DBFF/DC00/DFFF；high后普通字符/转义反斜杠/high；low+high | 完整语法已通过时InvalidUnicode；未调用string也必须拒绝，不返回部分document。 |
| RJ16 invalid_unicode_keys_and_pair_boundaries | key位置孤立surrogate；合法pair key与literal同scalar key；字面反斜杠后uD800对照 | 孤立key InvalidUnicode；合法pair解码；相同解码key DuplicateField；字面反斜杠文本合法。 |
| RJ17 first_scanner_error_follows_source_order | {"a":1,"a":2,"bad":"\uD800"}；交换bad到重复成员之前；key Unicode错误对照 | 前者DuplicateField，后者InvalidUnicode；完整语法校验已通过后按扫描首错。 |
| RJ18 duplicate_decoded_keys_at_every_depth | 根/深层/数组内对象a与escaped a、é与escaped é、空key重复、字面key重复 | 每object独立解码key集合；全部DuplicateField，未知深层不能跳过。 |
| RJ19 sibling_keys_and_no_unicode_normalization | [{"a":1},{"\u0061":2}]；同object预组合é与e+组合音符；A/a | 兄弟同名合法；不normalization/大小写折叠，get按完整解码键精确比较，保序。 |
| RJ20 escaped_delimiters_and_quotes_in_keys | key含escaped quote/backslash、[]{}:,、NUL、pair；解码等价重复对照 | 正确key和查找，不误进结构；等价重复拒绝。 |
| RJ21 exceed_old_structural_token_budget | 数组262145个0，完整遍历计数/首尾/len；object131073个互异key | 超131072叶节点仍完整成功，无token/成员门槛、截断或淘汰；bytes与生成输入全等，成员顺序精确。 |
| RJ22 body_above_thirty_two_mib | 未知字符串有32*1024*1024+257个ASCII字符，尾含Unicode标记，另未知大数 | parse成功，完整bytes/string/尾标记与生成目标全等，compact全等；不代签实际帧入口。 |
| RJ23 twenty_thousand_arrays | 20000层数组包0；另96/128/129层；循环get0到底 | 各层Array/len1、叶0；完整成功；小栈子进程正常join，无旧深度门槛。 |
| RJ24 twenty_thousand_mixed_objects | 20000层object {"k":...}与array交替，叶含escaped括号/quote，循环走全部路径 | 不将未知结构扁平化；跨层同key合法；完整边界/顺序/解码；无逐层重解整子树。 |
| RJ25 deep_clone_owned_and_drop | RJ23/24 clone文档与根/深owned各64次，交错drop；最后owned可读叶，再全部drop | 共享文本地址、不递归clone/drop；小栈子进程正常退出；源码佐证平面节点只含下标、索引共享。 |
| RJ26 deep_failure_drop_is_nonrecursive | 20000层底部重复key、孤立surrogate；另缺闭括号/尾逗号；失败后正常parse | 分别DuplicateField/InvalidUnicode/InvalidJson；失败清理不溢栈/挂起/污染后续，不产生半document。 |
| RJ27 errors_never_include_sensitive_payload | secret key/value/长数字canary构造三类parse错误，含secret对象string触发WrongType；公开构造全部error含Allocation | Display/Debug/to_string及source链均无输入/键/值/数字/附近片段；文案静态；Allocation不计真实OOM动态证据。 |
| RJ28 all_public_handle_debug_redacted | 含不同key/string/number canary的Document/Ref/Owned/Object/Array及clone逐一Debug | 五类句柄均实现Debug，不能出现canary及敏感前缀；只允许类型/种类/字节/节点元数据；iter不要求Debug。 |
| RJ29 public_traits_and_temporary_lifetimes | 编译检查Document/Owned Clone，Ref/Object/Array Copy+Clone，Kind/Error Copy+Clone+Debug+Eq+PartialEq，Error:Error；从临时object/array句柄取iter后继续用 | iter绑定文档'a，ExactSize+DoubleEnded，逐步len/size_hint正确；不要求Document/Owned Copy或iterator Debug；静态核对无Deserialize/Serialize和可变索引入口。 |
| RJ30 management_eight_layers_and_old_assets_unchanged | 既有strict_json/调用点哈希；同crateparse_object 8层有效object与9层；既有JWS/ACP/API定向回归；Cargo.lock前后 | 管理8层成功/9层TooDeep、常量仍8；原资产不改断言；Cargo.lock原字节；仅既有serde_json加raw_value feature不增包。 |

## 正式交付门槛

固定源清单后执行，保存实际命令/退出码、逐API/定义ID到真实测试名映射及源/资产前后SHA。使用仓库原rust-toolchain.toml、独占target、离线锁定依赖，不安装工具链/增加包。fmt、Clippy all/pedantic/nursery及-D warnings、check/build在实际集成源检查；开发自测和独立结果分列。

静态审查还须检查：授权写范围、未接旧API；完整语法优先；显式平面栈和每object解码key集合；无递归子节点拥有或Value回退、无逐层全子树重解析；无固定深度/token/正文阈值；无unchecked边界/unsafe/正常路径unwrap/expect/panic；计数checked及try_reserve错误映射；无正文日志。

30行全部通过也只代表本基础层；协议来源/唯一响应、NDJSON/outbox迁移、独立reader/writer、无上限共享缓存、真实Harness/WSS仍属后续完整gate，CL-095整体不因此勾选。

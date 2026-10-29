# R15资源嵌入修复及NA002范围最终结论

日期：2026-10-30。结论：**NA-SC-003 / Medium / Closed**，由原提出reviewer独立真实产品动态复核。**NA002供应链、密码库和合同源码检视范围PASS（固定R15）**；原NA002报告BLOCKED由此更新闭环，不回写历史失败。不是全部33例或未来R16前端身份协调的验收签署，不宣布可归档。

## 版本与feature

R15清单 `6C72DB1E3710D419F11ED7C10C3679FE5EDAB407341FE4E969451F6CC735E3B9`独立计算，与用户最终值一致。用户frozen副本83项逐文件匹配；再复制到reviewer自有 `.tmp/reviewer-implementation-r4-target/r15-source`，再次**83/83逐字SHA匹配**。generated Web dist另行复制作为编译asset，不说是83项原内容。

app/Cargo.toml明确rust-embed8.13.0启用debug-embed，DEPENDENCIES同步说明debug/release均编译期嵌入。独立 `cargo tree --locked --offline -e features -i rust-embed` 确认解析为default+debug-embed，实际普通构建重新编译其utils/impl。Cargo.lock保持 `28639ACDBA0A6C445FCA88F9191E3E1484DEECA21E63F0421BD2E2DFD9AB6B5A`，未引入新锁定包或provider。

主方140→142库+4向量/Web46完整check/build为交接开发证据，不冒称reviewer全量执行。主方ordinary binary摘要 `751EF07FC7B50D6E55E83F778EA7263736463CB630CDCE45E9DE2B0A50F6C215`为交接标识；本轮真实动态执行的是reviewer自己从同83项与单列dist构建的ordinary debug产物，摘要 `BA62D15E55BE2439F82F4260D7810BFF309EC662D268FB73A398B68184FF31B0`，不要求含构建路径的不同构建exe逐字相同，不冒称执行主方该hash。

## 原失败的独立动态闭环

实际独立 `cargo build --manifest-path <reviewer-r15-source>/app/Cargo.toml --target-dir <reviewer>/r10-build --locked --offline --bin acp-remote`，**普通dev/debug构建exit0，无release替代**。

随后执行自有验收：

1. 从自有dist保存index及其引用的JS/CSS完整bytes和两份assets；复制真实产品exe到独立copied-bin目录，该运行目录没有dist。
2. 先新建本轮目录并通过真实平台API设置owner-only保护，再生成localhost临时证书/私钥，私钥另收紧DACL；使用真实当前用户DPAPI启动复制exe的普通daemon。端口由OS分配的本轮空闲loopback端口，不占30447，未必使用建议30460。
3. 在启动前将**reviewer自己源码副本的dist**重命名为不可见的hidden目录。真实HTTPS客户端只信任本轮证书，验证localhost名称，不使用rejectUnauthorized=false、不安装根证书；index和所引用两个JS/CSS均HTTP200且bytes与构建时完全一致（比仅hash断言更强）。
4. 继续修改自有hidden index及两个assets为不同文本，再次请求，仍返回原bytes。证明不是偷读已隐藏的构建树，也不是只验证链接能打开。
5. 用同owner普通CLI daemon stop，要求exit0；等待节点exit0、重新绑定原端口证明释放。还原仅自有副本dist及其原bytes。节点退出后核实待删目录真实绝对路径在reviewer专属根内，再清理本轮证书/密钥/数据库/复制exe夹具；最终目录不存在。没有强杀作为成功清理，没有修改用户原clients/web/dist或冻结源dist，也未操作用户数据/节点。

实际结果：`pass=true, ordinaryDebugBuild=true, assets=2, indexWithoutDist=true, assetBytesExact=true, sourceMutationIgnored=true, ownerStop=true, nodeExit=0, portReleased=true`，执行exit0。原始汇总 `.tmp/reviewer-implementation-r4-target/r15-asset-result.json` SHA256 `A339FCC527C82969B123D54110CBE00AEFC3DF0099500BBC0F0734EDDA5661F5`；测试源经stdin执行，未写入产品。自有fixture目录不存在，原自有dist恢复，hidden目录不存在。主方751EF0…产物也已独立Get-FileHash匹配；不混作本轮执行的BA62…产物。

这直接覆盖原SC003普通debug运行时依赖源dist的问题，故**关闭**。不把只换feature或cargo check通过当动态闭环；也不把本轮localhost自信任HTTPS请求说成真实Chromium/Tailscale重新验收。

## NA002最终限定结论

同日 `2026-10-30-na002-security-supplychain-review.md` 的依赖元数据33项、Windows目标238包与锁261包区分、官方上游/许可证/平台/漏洞审查和合同§1–7映射继续有效。唯一新增阻断SC003现关闭；此前SC001/002、RV001/002、CON/LIFE/I01不重开。可采纳：

- 同锁官方cargo-audit0.22.2/RustSec固定快照1386公告、261包0漏洞warnings空；根/Web npm审计同锁0。没有再说扫描下载失败；未yanked/非实时最新/未知漏洞属于如实限制，不引入笼统法务门禁。
- 精确Gitleaks allowlist正反控制、历史331 commits0及本轮590可见文件0命中，和主方522扫描范围分开。后来仅报告文字/metadata新增不冒称属于已扫副本；最终交付清单/产物仍需注明实际范围，不需要重跑无关全套产品。
- 真实可解析BASE5777067→HEAD7a6e9ec commitlint本轮独立exit0，未提交源码不假造commits；官方维护/许可来源已列，宽松许可保留NOTICE义务，无已知必须替换依赖问题。
- 真实Chromium/Rust JWS原字节互操作按相同负责bytes的独立报告复用，reviewer自有Rust/Node补验明确边界；本次不伪称自己再跑浏览器。
- R13定义checker经独立正向与两个错定义负向控制，严格限定33例/三个字段/28场景引用，不复活旧繁琐流程，不以静态PASS代签行为通过。

NA002的本次reviewer审查交付已完成，不因“没有全部库的形式化证明/完整法律鉴证”继续阻断。整体change仍需tester当前最终执行结论；新发现的多tab撤销/旋转key竞态属于待R16前端runtime修复与独立复测，不由R15静态资源PASS覆盖，合同总体符合性对未来新字节不得提前承诺。

仅新增本报告和授权的R14报告，不修改产品/旧报告/manifest/plan/tasks，不提交推送归档。

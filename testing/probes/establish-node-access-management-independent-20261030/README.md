# 独立管理验收（NA027/028/029）

2026-10-30执行：R16仅夹具准备与浏览器配对；最终节点/嵌入Web均切换到已核对R17普通binary。同日正式报告 `2026-10-30-r17-management-independent-acceptance.md` 记录原始证据、已失败夹具及复用边界。

入口顺序：setup.mjs（仅新自有data，原生owner helper实际批准A/B，停机植入67个合成分页记录后重启）→ browser-open/reset-owned-origin及browser-pair-session/browser-pair（一个ego空间、一份真实Web身份，第70条）→ upgrade-r17（正常stop后复制已核对新binary，保留data）→ browser-page-through（真实按钮32/32/6）→ readonly-cursors（真实59/60秒）→ revocations（失败/成功/自撤销）→ audit-output → browser-final-observe → browser-cleanup → stop-nodes/精确native pipe与锁检查。

原生helper源码位于相邻 `establish-node-access-management-native-20261030`。本轮构建时复制源码/Cargo到自有.tmp/native-build，仅生成Cargo中app路径指向冻结R16 app；helper只作普通公开owner IPC客户端，实际节点为R17复制exe，不用helper嵌入页面。运行时支持--address精确推导对应产品pipe。

root和30451/30452为本次tester独占。所有data/身份/配对码/target/log均在.tmp，借用的受信证书/私钥只读引用，没有复制/修改。private测试key使用公开合成seed，仅用于独立客户端fixture；真正Web私钥由Chromium产生并保持不可导出，从不传给Node或节点。

不要直接重放已finish的ego空间13，也不要将默认root改成用户data。新复现应使用新自有root，先验证manifest/binary与owner-only ACL，再配置脚本对应root、固定可用端口和自己的TaskSpace。不要运行旧失败的CDP全页自动分页假设作为产品判据；产品分页必须点击实际“载入下一页”。所有失败准备步骤仅按报告归因，不算产品成功或失败。

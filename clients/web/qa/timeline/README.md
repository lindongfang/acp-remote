# 本地时间线组件独立回归

本目录只包含测试驱动，不接main、Node/WSS、权限或原生控制。正式来源为独立Timeline TL01–10、v3原件读取及M1–M6资产；通过本组件不等于完整CL-023。数据51个case及原130/257字节向量沿原测试路径引用，不复制向量或修改原130。`npm.cmd run check`、`npm.cmd test`、`npm.cmd run build`仍使用工程原入口/配置/锁，无新增依赖。

在`clients/web`运行已安装的本机工具；脚本不会安装、下载、执行npm audit或访问外站：

```powershell
$env:TIMELINE_PLAYWRIGHT='C:/path/to/existing/playwright/index.mjs'
$env:TIMELINE_EDGE='C:/path/to/existing/msedge.exe'
$env:TIMELINE_QA_OUT='C:/path/to/new-owned-output/browser'
node qa/timeline/browser.mjs
$env:TIMELINE_QA_OUT='C:/path/to/new-owned-output/zoom'
node qa/timeline/zoom.mjs
$env:TIMELINE_QA_OUT='C:/path/to/new-owned-output/types'
node qa/timeline/types.mjs
```

每个输出目录必须尚不存在，防止覆盖首红；省略OUT会在工程`.tmp`创建唯一目录。PW入口/Edge路径必须是已有受信本机安装，环境配置不是来自Native正文。已验证工具为Playwright1.64.1/本机Edge，具体实际文件摘要/版本写入每次environment.json；换工具版本须重新运行，不借旧结论。普通浏览器运行59组，zoom另两主题；类型驱动正常全工程编译为正控制，六个预期语义诊断与negativeExit单列，不接受缺依赖失败冒负例通过。

浏览器只绑定127.0.0.1随机端口与构建测试页；`/fixtures`直接只读既有`tests/independent-projection/fixtures/rust-v7`，不向QA包再拷257文件。普通测试创建新的非持久Context；真实200%使用launchPersistentContext及**本次新建的私有临时profile**和仅本地工具扩展的chrome.tabs.setZoom/getZoom=2，绝非复用用户profile。没有CSS/DSF/pinch替代。输出目录新建检查与cleanup绝对边界检查后，只删除自己创建的tmp/profile；报告、原始日志、截图和扩展保留，browser/context/server在finally关闭。受限环境Edge若早退应保存首错，并按既有本机测试授权运行相同脚本的owner环境，不更改系统策略或接管用户profile。

查看实际report中的case状态、退出与source摘要，检查图片及键盘/尺寸/焦点证据；零用例、仅启动、只有截图都不算通过。错误驱动包括合成同步内部Range/DOM故障，不是真实全局OOM。Native及真实资源/授权/审批/全会话链路、真实读屏语音仍不在此组件回归范围。

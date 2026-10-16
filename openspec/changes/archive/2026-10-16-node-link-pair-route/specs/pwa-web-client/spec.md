## ADDED Requirements

### Requirement: 两类配对二维码都能落到可用入口

前端 SHALL 让本机生成的两类配对二维码内容都能打开一个**可用的配对入口**：`/pair#data=…`（sync 设备配对）与 `/node-link/pair#data=…`（node-link 配对，`NODE_LINK_PROTOCOL.md` §13.1）。打开后者 MUST NOT 落在未匹配路由或空页面上。

#### Scenario: node-link 二维码打开可用入口

- **WHEN** 浏览器打开服务端生成的 node-link 配对二维码内容（`/node-link/pair#data=…`）
- **THEN** 页面 SHALL 落在可用的配对入口（可读的地址/主机信息与可提交的输入），而不是未匹配路由；该断言 MUST 能在路径被改错时变红

#### Scenario: 两类入口互不混淆

- **WHEN** 分别打开 sync 设备二维码与 node-link 二维码
- **THEN** 两者 SHALL 各自进入与自身协议对应的配对流程，MUST NOT 把其中一类的内容交给另一类的流程解析

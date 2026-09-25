# Local Admin Channel v1 Fixtures

本目录是 [`schemas/local-admin/v1/`](../../../schemas/local-admin/v1/) 的正反固定向量，由 [`manifest.json`](./manifest.json) 驱动。

- `valid/`：请求、成功响应与失败响应三种信封各自至少一条；另含带完整 `params` 的方法请求，以及连字符方法名 `node.rotate-key.begin`（§5.7 登记、字段未定义时回 `local.unsupported`）的无参数请求。
- `invalid/`：每条在 manifest 里声明 `expectedKeyword`，用于固定「信封类错误必须被哪条约束拒绝」。版本号非 1、未知方法、方法名段首连字符、`params: null`、未知本地错误码、`ok: true` 缺 `result` 都在覆盖范围内。

边界：

- 这里只覆盖 channel `0x01` 的 envelope。channel `0x02` 的 ACP 字节流不在此表达（[`docs/LOCAL_ADMIN_PROTOCOL.md`](../../../docs/LOCAL_ADMIN_PROTOCOL.md) §3.1）。
- 各方法的 `params`/`result` 具体形状不在这里复制：schema 把它们当作开放容器，形状以文档 §5 为权威。
- manifest 必须列出本目录下的每个 JSON 文件；`npm run check` 的 `check:schemas` 会拒绝未登记的夹具。

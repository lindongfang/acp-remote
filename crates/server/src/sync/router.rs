//! 认证后消息的路由组合（变更 `sync-server-commands` 的任务 6.1）。
//!
//! [`SyncConn::with_route`](crate::sync::conn::SyncConn::with_route) 只有**一个**路由槽，而 Sync 的入站
//! 面按家族拆成两条路由：订阅面
//! ([`SubscribeRoute`](crate::sync::subscribe::SubscribeRoute)：`sync.subscribe`/`sync.snapshot_request`/
//! `sync.ack`) 与命令面 ([`CommandRoute`](crate::sync::command::CommandRoute)：`command`)。本模块是它们的
//! 组合件，也是组合根注册进 `/sync/v1` 的那一条路由。
//!
//! 两条不变量：
//!
//! - `route` **按注册顺序询问**，第一个 [`RouteOutcome::Claimed`] 的胜出；全部未认领时返回
//!   [`RouteOutcome::Unclaimed`]，`conn` 因此对未知或尚无实现的 v1 类型照旧回
//!   `protocol.type_unsupported`（对端可见的显式拒绝，绝不静默丢弃）。组合件不承载任何业务规则，也不
//!   重算任何判定。
//! - `on_authenticated` **转发给每一个子路由**：这是本次协商结果的唯一交接点（wire 上没有第二个承载它
//!   的字段，见 [`MessageRoute::on_authenticated`]）。漏转发不会报错，只会让门控静默取到空集（失败关闭
//!   的方向），因此这里按「一个不落」实现，而不是「谁需要谁自己拿」。
//!
//! 与 `server::node_link` 的组合件**同形而各自定义**（`AGENTS.md` §5/§6：平级的入站适配器互不调用、
//! 类型各自定义），差别只有一处：Node Link 的连接面今天不交接 feature，那条组合件因此没有
//! `on_authenticated` 这一腿。

use std::sync::Arc;

use sync_protocol::envelope::Envelope;

use crate::sync::conn::{ConnectionHandle, MessageRoute, RouteOutcome};

/// 多路复用：按注册顺序询问每个路由，第一个认领的胜出；`on_authenticated` 一个不落地转发。
///
/// 组合根的注册顺序固定为**订阅面 → 命令面**：两面的 type 词表不相交（`sync.*` 与 `command`），顺序因此
/// 不改变判定结果，但固定顺序让询问次数可预期，也让用例能钉住「订阅面不被命令面抢先」。
pub struct SyncRoutes {
    parts: Vec<Arc<dyn MessageRoute>>,
}

impl SyncRoutes {
    /// 装配（顺序即询问顺序，也是 [`MessageRoute::on_authenticated`] 的转发顺序）。
    ///
    /// 入参是订阅面与命令面的两条路由：接受 `Vec` 只为与 `server::node_link` 的组合件同形——本端点今天
    /// 只有这两个家族。
    pub fn new(parts: Vec<Arc<dyn MessageRoute>>) -> Self {
        Self { parts }
    }
}

#[async_trait::async_trait]
impl MessageRoute for SyncRoutes {
    async fn route(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        for part in &self.parts {
            if part.route(session, message).await == RouteOutcome::Claimed {
                return RouteOutcome::Claimed;
            }
        }
        RouteOutcome::Unclaimed
    }

    fn on_authenticated(&self, session: &ConnectionHandle, features: &[String]) {
        for part in &self.parts {
            part.on_authenticated(session, features);
        }
    }
}

#[cfg(test)]
mod tests {
    //! [`SyncRoutes`] 的行为用例（任务 6.1）。
    //!
    //! 前三条用记账替身钉住组合件自身的两条不变量：询问顺序（第一条认领即止、全部未认领仍返回
    //! `Unclaimed`）与 `on_authenticated` 一个不落。
    //!
    //! 其余三条用**真实**的 `SubscribeRoute` + `CommandRoute` 跑同一条连接，证据只取对端可见的帧：
    //! `sync.snapshot_begin.chunkCount`（目录资源在不在）与 `command.result` 里的会话摘要（`workspace`
    //! 键、imported 行）。其中 `without_negotiation_both_faces_gate_the_optional_resources` 是
    //! `both_faces_answer_on_one_connection_with_the_negotiated_features` 的**反例方向**：漏转发
    //! `on_authenticated` 时，「协商之后」的那组断言会掉回未协商时的结果（已实测变红，见任务 6.1 的报告）。

    use std::sync::Mutex;

    use acp_core::model::NodeId;
    use identity_auth::Authority;

    use super::*;
    use crate::local_admin::test_support::{FakeClock, FakeEntropy, FakeKeystore};
    use crate::sync::command::CommandRoute;
    use crate::sync::conn::SyncConfig;
    use crate::sync::conn::registry::Outbound;
    use crate::sync::snapshot::{LOCAL_CATALOG_FEATURE, REMOTE_ORIGIN_FEATURE};
    use crate::sync::subscribe::OwnerReachability;
    use crate::sync::subscribe::fixture::{
        self, CONNECTION, DEVICE, EPOCH, Frame, HOST, REMOTE_SESSION, Reachability, SESSION,
        SyncDeliveries, World, bodies, connection_with_limits, cursor, envelope, frames,
    };

    /// 本机身份状态机的 key handle（与 `local_admin`/命令面用例同一个登记名）。
    const NODE_KEY: &str = "node-identity";
    /// 一条查询命令的 `requestId`。
    const REQUEST: &str = "4c4dafda-dd98-442e-8d55-252b75bac72d";

    // -----------------------------------------------------------------------------------------
    // 记账替身（组合件自身的两条不变量）
    // -----------------------------------------------------------------------------------------

    /// 只认领一个 type 的子路由替身；记录每次被询问的 type 与 `on_authenticated` 的入参。
    #[derive(Default)]
    struct Stub {
        /// 认领的 type（`None` = 什么都不认领）。
        claims: Option<&'static str>,
        /// 被询问的 type（顺序即询问顺序）。
        asked: Mutex<Vec<&'static str>>,
        /// 末次 `on_authenticated` 交接的 feature。
        features: Mutex<Option<Vec<String>>>,
    }

    impl Stub {
        fn claiming(kind: &'static str) -> Self {
            Self {
                claims: Some(kind),
                ..Self::default()
            }
        }

        fn unclaiming() -> Self {
            Self::default()
        }

        fn asked(&self) -> Vec<&'static str> {
            self.asked.lock().expect("替身锁").clone()
        }

        fn features(&self) -> Option<Vec<String>> {
            self.features.lock().expect("替身锁").clone()
        }
    }

    #[async_trait::async_trait]
    impl MessageRoute for Stub {
        async fn route(&self, _session: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
            let kind = message.message_type().as_str();
            self.asked.lock().expect("替身锁").push(kind);
            if self.claims == Some(kind) {
                RouteOutcome::Claimed
            } else {
                RouteOutcome::Unclaimed
            }
        }

        fn on_authenticated(&self, _session: &ConnectionHandle, features: &[String]) {
            *self.features.lock().expect("替身锁") = Some(features.to_vec());
        }
    }

    /// 把替身按给定顺序装成组合件。
    fn chain(stubs: Vec<Arc<Stub>>) -> SyncRoutes {
        SyncRoutes::new(
            stubs
                .into_iter()
                .map(|stub| stub as Arc<dyn MessageRoute>)
                .collect(),
        )
    }

    // -----------------------------------------------------------------------------------------
    // 真实组合件（`SubscribeRoute` + `CommandRoute`）
    // -----------------------------------------------------------------------------------------

    /// 一条连接上真实组合件的固定装配。
    struct Harness {
        routes: SyncRoutes,
        handle: Arc<ConnectionHandle>,
        receiver: tokio::sync::mpsc::Receiver<Outbound>,
        sequence: u64,
    }

    impl Harness {
        /// `features` 是本次「握手协商结果」（用例直接交接，不经真实握手）。
        fn new(world: &World, features: &[String]) -> Self {
            let owner: Arc<dyn OwnerReachability> = Arc::new(Reachability(true));
            let command = CommandRoute::new(
                Arc::clone(&world.core),
                Arc::clone(&world.broker),
                Arc::new(Authority::new(
                    NodeId::new(HOST).expect("host id"),
                    identity_auth::port::KeyHandle::new(NODE_KEY).expect("key handle"),
                    Arc::new(FakeKeystore::default()),
                    Arc::new(FakeEntropy::default()),
                    Arc::new(FakeClock::new()),
                )),
                Arc::clone(&world.registry),
                Arc::clone(&owner),
            );
            let routes = SyncRoutes::new(vec![
                Arc::new(world.route(owner)) as Arc<dyn MessageRoute>,
                Arc::new(command) as Arc<dyn MessageRoute>,
            ]);
            let (handle, receiver) =
                connection_with_limits(DEVICE, CONNECTION, &SyncConfig::default());
            world.registry.register(Arc::clone(&handle));
            routes.on_authenticated(&handle, features);
            Self {
                routes,
                handle,
                receiver,
                sequence: 0,
            }
        }

        /// 发一条消息并把新产生的帧取回。
        async fn send(&mut self, kind: &str, body: serde_json::Value) -> Vec<Frame> {
            self.sequence += 1;
            let message = envelope(kind, self.sequence, body);
            assert_eq!(
                self.routes.route(&self.handle, &message).await,
                RouteOutcome::Claimed,
                "{kind} 必须被组合路由认领"
            );
            frames(&mut self.receiver)
        }
    }

    /// 一条 `sync.subscribe` 的 body（首次同步：`cursor` 为 null）。
    fn subscribe_body() -> serde_json::Value {
        serde_json::json!({ "cursor": null, "scope": "machine" })
    }

    /// 一条查询命令的 body（`command.schema.json` 的必填三键）。
    fn command_body(command: &str) -> serde_json::Value {
        serde_json::json!({
            "requestId": REQUEST,
            "command": command,
            "payload": {},
        })
    }

    /// 一次查询命令的 `completed` 结果体（查询命令只回一条 `command.result`）。
    fn completed(out: &[Frame], command: &str) -> serde_json::Value {
        let results = bodies(out, "command.result");
        assert_eq!(results.len(), 1, "{command} 必须只回一条结果");
        let body = results[0];
        assert_eq!(
            body.get("status").and_then(serde_json::Value::as_str),
            Some("completed"),
            "{command} 的首次响应必须是 completed"
        );
        assert_eq!(
            body.get("command").and_then(serde_json::Value::as_str),
            Some(command)
        );
        body.get("result")
            .cloned()
            .expect("completed 必须带 result")
    }

    /// `command.session.list` 结果里的会话摘要。
    fn sessions_of(result: &serde_json::Value) -> Vec<&serde_json::Value> {
        result
            .get("sessions")
            .and_then(serde_json::Value::as_array)
            .expect("sessions 数组")
            .iter()
            .collect()
    }

    /// 带一条本地会话与一条 imported 索引行的世界（两个可选资源各有一个来源）。
    fn world_with_optional_resources() -> World {
        let world = World::new();
        world.seed_device(DEVICE);
        world.store.set_head(cursor(EPOCH, 7));
        world.seed_workspace("project", "Project", &fixture::workspace_path());
        world.store.push_session(fixture::core_session(
            SESSION,
            "本地会话",
            Some(("project", "Project")),
        ));
        world.deliveries.push(SyncDeliveries::record("远端会话"));
        world
    }

    // -----------------------------------------------------------------------------------------
    // 判据一：`route` 按序链式委托
    // -----------------------------------------------------------------------------------------

    /// `route`：按注册顺序询问、第一条认领即止；订阅面的 type 不被命令面抢先，反之亦然。
    #[tokio::test]
    async fn the_first_child_that_claims_a_message_wins_and_the_rest_are_not_asked() {
        let subscribe = Arc::new(Stub::claiming("sync.subscribe"));
        let command = Arc::new(Stub::claiming("command"));
        let routes = chain(vec![Arc::clone(&subscribe), Arc::clone(&command)]);
        let (handle, _receiver) = fixture::connection(DEVICE);

        // ① 订阅面的 type：第一条认领，命令面根本不被问到。
        let message = envelope("sync.subscribe", 1, subscribe_body());
        assert_eq!(routes.route(&handle, &message).await, RouteOutcome::Claimed);
        assert_eq!(subscribe.asked(), vec!["sync.subscribe"]);
        assert!(
            command.asked().is_empty(),
            "第一条认领之后不得继续询问（同一封信封只被处理一次）"
        );

        // ② 命令面的 type：订阅面如实未认领，命令面接住。
        let message = envelope("command", 2, command_body("session.list"));
        assert_eq!(routes.route(&handle, &message).await, RouteOutcome::Claimed);
        assert_eq!(
            subscribe.asked(),
            vec!["sync.subscribe", "command"],
            "订阅面先被问到且如实未认领"
        );
        assert_eq!(command.asked(), vec!["command"]);
    }

    /// `route`：两条子路由都不认领时仍是 `Unclaimed` —— `conn` 因此照旧回
    /// `protocol.type_unsupported`，未知 type 的既有行为不变（绝不静默丢弃）。
    #[tokio::test]
    async fn a_message_no_child_claims_stays_unclaimed() {
        let subscribe = Arc::new(Stub::claiming("sync.subscribe"));
        let command = Arc::new(Stub::claiming("command"));
        let routes = chain(vec![Arc::clone(&subscribe), Arc::clone(&command)]);
        let (handle, _receiver) = fixture::connection(DEVICE);

        // 两个都是服务端→客户端的方向：两条路由都不认领。
        for (sequence, kind) in [(1_u64, "sync.caught_up"), (2, "command.result")] {
            let message = envelope(kind, sequence, serde_json::json!({}));
            assert_eq!(
                routes.route(&handle, &message).await,
                RouteOutcome::Unclaimed,
                "{kind} 不该被任何一条子路由认领"
            );
        }
        assert_eq!(subscribe.asked(), vec!["sync.caught_up", "command.result"]);
        assert_eq!(
            command.asked(),
            vec!["sync.caught_up", "command.result"],
            "全部未认领时必须问完每一条（不得提前返回）"
        );
    }

    // -----------------------------------------------------------------------------------------
    // 判据二：`on_authenticated` 转发给每一个子路由
    // -----------------------------------------------------------------------------------------

    /// `on_authenticated`：协商结果转发给**每一个**子路由，且每次交接都是同一份清单（不是缓存第一次的
    /// 结果，也不是只发给「需要它的那一个」）。
    #[tokio::test]
    async fn the_negotiated_features_reach_every_child() {
        let subscribe = Arc::new(Stub::unclaiming());
        let command = Arc::new(Stub::unclaiming());
        let routes = chain(vec![Arc::clone(&subscribe), Arc::clone(&command)]);
        let (handle, _receiver) = fixture::connection(DEVICE);

        let features = vec![
            LOCAL_CATALOG_FEATURE.to_owned(),
            REMOTE_ORIGIN_FEATURE.to_owned(),
        ];
        routes.on_authenticated(&handle, &features);
        assert_eq!(subscribe.features(), Some(features.clone()));
        assert_eq!(command.features(), Some(features));

        // 空清单同样转发：门控失败关闭的取值必须真的送达每个子路由。
        routes.on_authenticated(&handle, &[]);
        assert_eq!(subscribe.features(), Some(Vec::new()));
        assert_eq!(command.features(), Some(Vec::new()));
    }

    /// 组合根注册的那条真实路由：同一条连接上订阅面与命令面都可达，且**两个面都拿到**本次协商结果。
    ///
    /// 证据只取对端可见的帧：
    ///
    /// - `sync.snapshot_begin.chunkCount` = 3（目录资源两块在场，`core.local-catalog.v1` 起作用）；
    /// - `command.session.list` 的结果里有 imported 会话（`resource.remote-origin.v1` 起作用）且本地会话
    ///   摘要带 `workspace` 键（`core.local-catalog.v1` 起作用）。
    #[tokio::test]
    async fn both_faces_answer_on_one_connection_with_the_negotiated_features() {
        let world = world_with_optional_resources();
        let features = vec![
            LOCAL_CATALOG_FEATURE.to_owned(),
            REMOTE_ORIGIN_FEATURE.to_owned(),
        ];
        let mut harness = Harness::new(&world, &features);

        // 订阅面：首次同步 → 快照 → 3 块（sessions + workspaces + agents）。
        let out = harness.send("sync.subscribe", subscribe_body()).await;
        assert_eq!(
            bodies(&out, "sync.reset_required")[0]["reason"],
            "initial_sync"
        );
        let out = harness
            .send(
                "sync.snapshot_request",
                serde_json::json!({ "reason": "initial_sync" }),
            )
            .await;
        assert_eq!(
            bodies(&out, "sync.snapshot_begin")[0]["chunkCount"],
            3,
            "协商了 core.local-catalog.v1 → 目录资源两块在场"
        );
        assert_eq!(bodies(&out, "sync.snapshot_chunk").len(), 3);
        assert_eq!(bodies(&out, "sync.snapshot_end").len(), 1);

        // 命令面：同一连接上的一条查询命令。
        let out = harness.send("command", command_body("session.list")).await;
        let result = completed(&out, "session.list");
        let sessions = sessions_of(&result);
        let ids: Vec<Option<&str>> = sessions
            .iter()
            .map(|summary| summary.get("sessionId").and_then(serde_json::Value::as_str))
            .collect();
        assert!(ids.contains(&Some(SESSION)), "本地会话必须可见");
        assert!(
            ids.contains(&Some(REMOTE_SESSION)),
            "协商了 resource.remote-origin.v1 → imported 会话必须可见"
        );
        let local = sessions
            .iter()
            .find(|summary| {
                summary.get("sessionId").and_then(serde_json::Value::as_str) == Some(SESSION)
            })
            .expect("本地会话摘要");
        assert_eq!(
            local
                .get("workspace")
                .and_then(|workspace| workspace.get("alias"))
                .and_then(serde_json::Value::as_str),
            Some("project"),
            "协商了 core.local-catalog.v1 → workspace 键在场"
        );
    }

    /// 同一条真实路由在**未协商**时两个面都门控关闭。这是上一条用例的反例方向：漏转发
    /// `on_authenticated` 会让「协商之后」的那组断言掉回这里的结果。
    #[tokio::test]
    async fn without_negotiation_both_faces_gate_the_optional_resources() {
        let world = world_with_optional_resources();
        let mut harness = Harness::new(&world, &[]);

        let _ = harness.send("sync.subscribe", subscribe_body()).await;
        let out = harness
            .send(
                "sync.snapshot_request",
                serde_json::json!({ "reason": "initial_sync" }),
            )
            .await;
        assert_eq!(
            bodies(&out, "sync.snapshot_begin")[0]["chunkCount"],
            1,
            "未协商 core.local-catalog.v1 → 目录资源整个缺席，只剩 sessions 一块"
        );

        let out = harness.send("command", command_body("session.list")).await;
        let result = completed(&out, "session.list");
        let sessions = sessions_of(&result);
        assert_eq!(sessions.len(), 1, "未协商时 imported 行一行都不出现");
        assert_eq!(
            sessions[0]
                .get("sessionId")
                .and_then(serde_json::Value::as_str),
            Some(SESSION)
        );
        assert!(
            sessions[0].get("workspace").is_none(),
            "未协商 core.local-catalog.v1 → workspace 键整个缺席"
        );
    }

    /// 真实的 `command` 帧经组合件到达命令面（订阅面如实未认领），并得到终态 `command.result`。
    #[tokio::test]
    async fn a_real_command_frame_reaches_the_command_face() {
        let world = world_with_optional_resources();
        let mut harness = Harness::new(&world, &[]);
        let out = harness.send("command", command_body("session.list")).await;
        assert!(
            bodies(&out, "error").is_empty(),
            "已授权设备的一条查询命令不该得到 error"
        );
        let _ = completed(&out, "session.list");
    }
}

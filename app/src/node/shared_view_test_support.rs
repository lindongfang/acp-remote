//! Real Node pairing/authentication under a registered fixture task. No actor constructor.
#[cfg(feature = "shared-view-acceptance-fixtures")]
use super::shutdown_observation::NodeShutdownProbe;
#[cfg(feature = "shared-view-acceptance-fixtures")]
pub use super::shutdown_observation::{
    NodeShutdownDisposition, NodeShutdownFailure, NodeShutdownFailureFlags, NodeShutdownPhase,
    NodeShutdownSnapshot, NodeShutdownStage, NodeStageOutcome,
};
use super::{
    ApplicationPolicy, LocalIpcAddress, NodeConfig, NodeRuntime, PairingService, SHUTDOWN_TIMEOUT,
    ipc,
};
use crate::{
    application::shared_views::local_authority::{
        AuthenticatedLocalViewClient, LocalViewAuthority,
    },
    claims::{AuthProof, Id128, Nonce256, PairClaim, PublicKeySec1},
    domain::shared_view::ViewError,
    jws::{self, JwsType},
    platform::DpapiProtector,
    shared_view_test_support::{FixtureSigningClient, FixtureTaskOwner, OwnedTestArea},
};
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};
use tokio::time::Instant as TokioInstant;
#[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
pub(crate) mod auth_close_observation;
#[cfg(feature = "shared-view-acceptance-fixtures")]
pub use auth_close_observation::{
    AreaObservation, ClosePhase, ErasureSite, FirstCloseObservation, OwnerObservation,
};

pub type LocalFixtureViewAuthority = LocalViewAuthority;
pub struct LocalViewAuthFixture {
    #[cfg(feature = "shared-view-acceptance-fixtures")]
    node_shutdown_probe: NodeShutdownProbe,
    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
    close_observer: auth_close_observation::FirstCloseObserver,
    area: Mutex<OwnedTestArea>,
    owner: FixtureTaskOwner,
    pairing: Arc<PairingService>,
    policy: Arc<ApplicationPolicy>,
    authority: Arc<LocalFixtureViewAuthority>,
    address: LocalIpcAddress,
    clients: Mutex<Vec<(PublicKeySec1, Id128)>>,
}
struct Started {
    #[cfg(feature = "shared-view-acceptance-fixtures")]
    node_shutdown_probe: NodeShutdownProbe,
    pairing: Arc<PairingService>,
    policy: Arc<ApplicationPolicy>,
    authority: Arc<LocalFixtureViewAuthority>,
    address: LocalIpcAddress,
}
impl LocalViewAuthFixture {
    /// Current/last same-Node observation at this read; not a first-close-time snapshot.
    #[cfg(feature = "shared-view-acceptance-fixtures")]
    #[must_use]
    pub fn node_shutdown_observation(&self) -> NodeShutdownSnapshot {
        self.node_shutdown_probe.snapshot()
    }
    /// Reads only the original first explicit close observation; never starts cleanup.
    #[cfg(feature = "shared-view-acceptance-fixtures")]
    #[must_use]
    pub fn first_close_observation(&self) -> FirstCloseObservation {
        auth_close_observation::first_close_snapshot(self)
    }
    /// # Errors
    /// Uses only the supplied owned child and its preinstalled TLS. Startup remains supervised
    /// if the caller stops waiting, and all errors are static categories.
    pub async fn start(area: OwnedTestArea) -> Result<Self, ViewError> {
        area.validate().map_err(|_| ViewError::ReadFailure)?;
        let config = NodeConfig {
            data_dir: area.path().join("data"),
            listen: ([127, 0, 0, 1], 0).into(),
            canonical_origin: "https://shared-view.fixture.test".to_owned(),
            tls_cert: area.path().join("server.pem"),
            tls_key: area.path().join("server-key.pem"),
        };
        let owner = FixtureTaskOwner::new(&area, tokio::runtime::Handle::current())
            .map_err(|_| ViewError::ReadFailure)?;
        let handle = owner.handle();
        let mut stop = handle.subscribe_stop();
        let (ready, started) = oneshot::channel();
        handle.spawn(async move {
            let runtime=NodeRuntime::start(config,std::sync::Arc::new(DpapiProtector)).await.map_err(|_|crate::shared_view_test_support::FixtureError::Io)?;
            let parts=if let (Some(pairing),Some(policy))=(runtime.pairing.as_ref(),runtime.application_policy()) {
                Started {
                    #[cfg(feature = "shared-view-acceptance-fixtures")]
                    node_shutdown_probe:runtime.shutdown_observation_probe(),pairing:Arc::clone(pairing),authority:Arc::new(policy.local_view_authority(Vec::new())),policy,address:runtime.ipc_address().clone()}
            } else {runtime.control.request_stop();let _=runtime.wait_for_stop().await;return Err(crate::shared_view_test_support::FixtureError::Io);};
            let control=runtime.control.clone();
            if ready.send(parts).is_err()||*stop.borrow() {control.request_stop();}
            let running=runtime.wait_for_stop();tokio::pin!(running);
            tokio::select! {
                result=&mut running=>result.map_err(|_|crate::shared_view_test_support::FixtureError::Io),
                _=stop.changed()=>{control.request_stop();running.await.map_err(|_|crate::shared_view_test_support::FixtureError::Io)}
            }
        }).map_err(|_|ViewError::ReadFailure)?;
        let started = started.await.map_err(|_| ViewError::ReadFailure)?;
        Ok(Self {
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            close_observer: auth_close_observation::FirstCloseObserver::new(),
            #[cfg(feature = "shared-view-acceptance-fixtures")]
            node_shutdown_probe: started.node_shutdown_probe,
            area: Mutex::new(area),
            owner,
            pairing: started.pairing,
            policy: started.policy,
            authority: started.authority,
            address: started.address,
            clients: Mutex::new(Vec::new()),
        })
    }
    #[must_use]
    pub fn authority(&self) -> Arc<LocalFixtureViewAuthority> {
        Arc::clone(&self.authority)
    }
    async fn register_client(&self, client: &FixtureSigningClient) -> Result<Id128, ViewError> {
        let public = client.public_key();
        // Serialize this synthetic client's registration, never native data or display IO.
        let mut clients = self.clients.lock().await;
        let id = if let Some((_, id)) = clients.iter().find(|(key, _)| *key == public) {
            *id
        } else {
            clients
                .try_reserve(1)
                .map_err(|_| ViewError::AllocationFailure)?;
            let (mut session, start) = ipc::begin_pairing(&self.address)
                .await
                .map_err(|_| ViewError::ReadFailure)?;
            let pairing_id =
                Id128::parse_base64url(&start.pairing_id).map_err(|_| ViewError::ReadFailure)?;
            let request_id =
                Id128::new(crate::random::bytes().map_err(|_| ViewError::RandomUnavailable)?);
            let lookup = self
                .pairing
                .lookup(&start.code, request_id)
                .await
                .map_err(|_| ViewError::ReadFailure)?;
            let claim = PairClaim {
                v: 1,
                node_id: lookup.challenge.node_id,
                origin: lookup.challenge.origin,
                request_id,
                challenge_id: lookup.challenge.challenge_id,
                nonce: lookup.challenge.nonce,
                iat: lookup.challenge.iat,
                exp: lookup.challenge.exp,
                pairing_id,
                client_public_key: public,
                display_name: "View fixture".to_owned(),
            };
            let compact = jws::sign(
                JwsType::PairClaim,
                &serde_json::to_vec(&claim).map_err(|_| ViewError::ReadFailure)?,
                &client.key,
            )
            .map_err(|_| ViewError::ReadFailure)?;
            self.pairing
                .claim(&compact)
                .await
                .map_err(|_| ViewError::ReadFailure)?;
            if session
                .next_event()
                .await
                .map_err(|_| ViewError::ReadFailure)?
                .event
                != "pending_confirmation"
            {
                return Err(ViewError::ReadFailure);
            }
            session
                .approve()
                .await
                .map_err(|_| ViewError::ReadFailure)?;
            let approved = session
                .next_event()
                .await
                .map_err(|_| ViewError::ReadFailure)?;
            if approved.event != "approved" {
                return Err(ViewError::ReadFailure);
            }
            let id = Id128::parse_base64url(&approved.client_id.ok_or(ViewError::ReadFailure)?)
                .map_err(|_| ViewError::ReadFailure)?;
            clients.push((public, id));
            id
        };
        drop(clients);
        Ok(id)
    }
    /// # Errors
    /// Pairs through actual owner approval, then signs/verifies the actual fresh proof.
    pub async fn authenticate(
        &self,
        client: &FixtureSigningClient,
    ) -> Result<AuthenticatedLocalViewClient, ViewError> {
        let id = self.register_client(client).await?;
        let public = client.public_key();
        let nonce =
            Nonce256::new(crate::random::bytes().map_err(|_| ViewError::RandomUnavailable)?);
        let lease = self
            .policy
            .issue_authentication_challenge(Some(id), public, nonce)
            .map_err(|_| ViewError::ReadFailure)?;
        let c = lease.challenge();
        let connection_id = lease.connection_id();
        let proof = AuthProof {
            v: 1,
            node_id: c.node_id,
            node_public_key: c.node_public_key,
            origin: c.origin.clone(),
            connection_id,
            challenge_id: c.challenge_id,
            nonce: c.nonce,
            iat: c.iat,
            exp: c.exp,
            client_id: Some(id),
            client_public_key: public,
            client_nonce: nonce,
        };
        let compact = jws::sign(
            JwsType::AuthClient,
            &serde_json::to_vec(&proof).map_err(|_| ViewError::ReadFailure)?,
            &client.key,
        )
        .map_err(|_| ViewError::ReadFailure)?;
        let actor = self
            .policy
            .authenticate(connection_id, &compact)
            .await
            .map_err(|_| ViewError::Revoked)?;
        self.authority.accept_actor(actor).await
    }
    /// # Errors
    /// Revokes the actually registered identity through the running Node's owner IPC.
    pub async fn revoke(&self, client: &FixtureSigningClient) -> Result<(), ViewError> {
        let id = self
            .clients
            .lock()
            .await
            .iter()
            .find(|(key, _)| *key == client.public_key())
            .map(|(_, id)| *id)
            .ok_or(ViewError::InvalidClient)?;
        ipc::device_revoke(&self.address, &id.to_base64url())
            .await
            .map_err(|_| ViewError::ReadFailure)?;
        Ok(())
    }
    /// # Errors
    /// Does not remove the area until the original Node task really joined and resources closed.
    pub async fn close(&self) -> Result<(), ViewError> {
        #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
        let mut observation = self.close_observer.begin_first();
        self.authority.close();
        #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
        observation.authority_returned();
        let report = self
            .owner
            .shutdown(TokioInstant::now() + SHUTDOWN_TIMEOUT)
            .await;
        #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
        observation.owner_returned(&report);
        if !report.all_joined() {
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            observation.returning(auth_close_observation::ErasureSite::OwnerNotJoined);
            return Err(ViewError::ReadFailure);
        }
        let area_outcome = self.area.lock().await.cleanup().await;
        #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
        observation.area_returned(&area_outcome);
        let report = area_outcome.map_err(|_| {
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            observation.returning(auth_close_observation::ErasureSite::AreaCleanupError);
            ViewError::ReadFailure
        })?;
        if !report.removed() {
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            observation.returning(auth_close_observation::ErasureSite::AreaNotRemoved);
            return Err(ViewError::ReadFailure);
        }
        #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
        observation.normal_return();
        Ok(())
    }
}
impl Drop for LocalViewAuthFixture {
    fn drop(&mut self) {
        self.authority.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::shared_views::CurrentViewAuthority;
    use std::time::Duration;
    use tokio::time::timeout;
    #[tokio::test]
    async fn shared_view_formal_auth_fixture_real_pair_proof_idle_revoke_and_close() {
        let area = OwnedTestArea::create(&std::env::temp_dir(), "shared-view-formal-auth")
            .expect("self-owned area");
        let tls = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])
            .expect("self-owned cert");
        area.install_tls(
            tls.cert.pem().as_bytes(),
            tls.signing_key.serialize_pem().as_bytes(),
        )
        .expect("create_new TLS");
        let fixture = LocalViewAuthFixture::start(area)
            .await
            .expect("real registered Node");
        let signing = FixtureSigningClient::generate().expect("self-owned signer");
        let client = fixture
            .authenticate(&signing)
            .await
            .expect("real pairing+actual AuthProof");
        assert!(!client.client_id().is_empty());
        assert!(fixture.authority().client_binding(&client).is_ok());
        assert!(matches!(
            fixture
                .policy
                .local_view_authority(Vec::new())
                .client_binding(&client),
            Err(ViewError::InvalidClient)
        ));
        let wait = fixture.authority.wait_client_change(&client);
        let revoke = async {
            fixture.authority.test_wait_until_registered().await;
            fixture
                .revoke(&signing)
                .await
                .expect("owner IPC actual revoke COMMIT");
        };
        let (result, ()) = timeout(Duration::from_secs(5), async { tokio::join!(wait, revoke) })
            .await
            .expect("post-SQL continuously polled wait");
        assert_eq!(result, Err(ViewError::Revoked));
        assert!(matches!(
            fixture.authenticate(&signing).await,
            Err(ViewError::Revoked)
        ));
        fixture
            .close()
            .await
            .expect("actual Node join and one verified area removal");
    }
}

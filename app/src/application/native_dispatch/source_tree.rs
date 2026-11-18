//! Original parent observations retain their required SQL work through source shutdown.
use super::{
    Arc, AtomicBool, ConfigurationPort, DispatchError, NativeControlRepository, NativeCoordinator,
    NativeDispatchRepository, NativeResourceRepository, NativeSessionKey, Ordering,
    OwnedNativeSource, SessionParentProof, SessionTreePath, SourceLaunchPort, runtime::SourceSlot,
};
use sha2::{Digest, Sha256};

pub(super) struct PendingTreeObservation {
    proof: SessionParentProof,
    stored: AtomicBool,
    action: tokio::sync::Mutex<()>,
}

impl PendingTreeObservation {
    pub(super) fn persisted_fingerprint(&self) -> Result<[u8; 32], DispatchError> {
        if !self.stored.load(Ordering::Acquire) {
            return Err(DispatchError::RequiredStateUnavailable);
        }
        Ok(self.proof.fingerprint)
    }
}

impl<P, L, G, N> NativeCoordinator<P, L, G, N>
where
    P: super::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + super::NativeApprovalRepository,
{
    pub(super) fn accept_tree_observation(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        parent: &str,
        child: &str,
        sequence: u64,
        receipt: u64,
    ) -> Result<(), DispatchError> {
        let parent = NativeSessionKey::from_config(Some(source.pre.metadata.source_id), parent)
            .map_err(|_| DispatchError::InvalidTarget)?;
        let child = NativeSessionKey::from_config(Some(source.pre.metadata.source_id), child)
            .map_err(|_| DispatchError::InvalidTarget)?;
        let root = {
            let mut activity = source
                .capacity_activity
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            activity.associate(parent.native_id(), child.native_id())?;
            if !activity.recognizes_member(parent.native_id()) {
                activity.unrecognized(parent.native_id());
                source
                    .pre
                    .authority
                    .quarantined
                    .store(true, Ordering::Release);
            }
            activity.root_for(child.native_id())
        };
        if let Some(root) = root {
            Self::seal_late_child(source, &root, &child)?;
        }
        let pending = {
            let mut inventory = source
                .tree_observations
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            if let Some(original) = inventory.iter().find(|entry| entry.proof.child == child) {
                if original.proof.parent != parent {
                    return Err(DispatchError::SessionTreeUnverified);
                }
                original.clone()
            } else {
                let mut digest = Sha256::new();
                digest.update(b"acp-remote-standard-parent-v1");
                for identity in [
                    source.pre.metadata.source_id.encode(),
                    source.pre.metadata.instance_id.encode(),
                    source.pre.metadata.runtime.0.encode(),
                    source.pre.metadata.connection_epoch.encode(),
                ] {
                    digest.update(identity.as_bytes());
                }
                digest.update(sequence.to_be_bytes());
                digest.update(receipt.to_be_bytes());
                for identity in [parent.native_id(), child.native_id()] {
                    digest.update(
                        u64::try_from(identity.len())
                            .map_err(|_| DispatchError::LengthUnrepresentable)?
                            .to_be_bytes(),
                    );
                    digest.update(identity.as_bytes());
                }
                let pending = Arc::new(PendingTreeObservation {
                    proof: SessionParentProof {
                        source: source.pre.clone(),
                        parent,
                        child,
                        sequence,
                        receipt,
                        fingerprint: digest.finalize().into(),
                        observed_at_ms: self.clock.unix_millis(),
                    },
                    stored: AtomicBool::new(false),
                    action: tokio::sync::Mutex::new(()),
                });
                inventory
                    .try_reserve(1)
                    .map_err(|_| DispatchError::StorageUnavailable)?;
                let _gate = source
                    .pre
                    .authority
                    .gate
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?;
                source.pre.authority.required.fetch_add(1, Ordering::AcqRel);
                inventory.push(pending.clone());
                pending
            }
        };
        if pending.stored.load(Ordering::Acquire) {
            return Ok(());
        }
        let core = self.clone();
        let slot = source.clone();
        let failed = self.clone();
        let failed_slot = source.clone();
        source.tasks.spawn_guarded(
            async move {
                let _ = core.persist_tree_observation(&slot, &pending).await;
            },
            move || failed.fail_source(&failed_slot, DispatchError::RequiredStateUnavailable),
        )?;
        Ok(())
    }

    fn seal_late_child(
        source: &SourceSlot<P, L>,
        root: &str,
        child: &NativeSessionKey,
    ) -> Result<(), DispatchError> {
        let root = NativeSessionKey::from_config(Some(source.pre.metadata.source_id), root)
            .map_err(|_| DispatchError::InvalidTarget)?;
        let resources = source.capacity.lock().map_err(|_| DispatchError::Stopped)?;
        let gate = source
            .pre
            .authority
            .gate
            .lock()
            .map_err(|_| DispatchError::Stopped)?;
        if source.pre.authority.closed.load(Ordering::Acquire)
            || source.pre.authority.closing.load(Ordering::Acquire)
            || resources.get(&root).is_some_and(|resource| {
                resource.pre.authority.releasing.load(Ordering::Acquire)
                    || resource.pre.authority.closed.load(Ordering::Acquire)
            })
        {
            source.view_attachment.seal_target(child);
        }
        drop(gate);
        drop(resources);
        Ok(())
    }

    async fn persist_tree_observation(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        pending: &PendingTreeObservation,
    ) -> Result<(), DispatchError> {
        let _action = pending.action.lock().await;
        if pending.stored.load(Ordering::Acquire) {
            return Ok(());
        }
        let path = self
            .repository
            .record_session_parent(&pending.proof, &self.deadline()?)
            .await?;
        Self::install_tree_path(source, &path)?;
        let _gate = source
            .pre
            .authority
            .gate
            .lock()
            .map_err(|_| DispatchError::Stopped)?;
        pending.stored.store(true, Ordering::Release);
        source.pre.authority.required.fetch_sub(1, Ordering::AcqRel);
        source.changed.notify_waiters();
        Ok(())
    }

    pub(super) async fn reconcile_tree_observations(
        &self,
        source: &Arc<SourceSlot<P, L>>,
    ) -> Result<(), DispatchError> {
        let observations = source
            .tree_observations
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        for observation in observations {
            self.persist_tree_observation(source, &observation).await?;
        }
        Ok(())
    }

    pub(super) fn install_tree_path(
        source: &Arc<SourceSlot<P, L>>,
        path: &SessionTreePath,
    ) -> Result<(), DispatchError> {
        if path.subject.source().0 != source.pre.metadata.source_id
            || path
                .ancestors
                .iter()
                .any(|key| key.source().0 != source.pre.metadata.source_id)
        {
            return Err(DispatchError::InvalidSource);
        }
        let mut activity = source
            .capacity_activity
            .lock()
            .map_err(|_| DispatchError::Stopped)?;
        let mut child = &path.subject;
        for parent in &path.ancestors {
            activity.associate(parent.native_id(), child.native_id())?;
            child = parent;
        }
        if let Some(root) = &path.root {
            if root != child {
                return Err(DispatchError::SessionTreeUnverified);
            }
            activity.verify_root(root.native_id())?;
        }
        let _gate = source
            .pre
            .authority
            .gate
            .lock()
            .map_err(|_| DispatchError::Stopped)?;
        source
            .pre
            .authority
            .quarantined
            .store(activity.has_unowned(), Ordering::Release);
        drop(activity);
        Ok(())
    }

    pub(super) async fn restore_tree_members(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        root: &NativeSessionKey,
    ) -> Result<(), DispatchError> {
        let members = self
            .repository
            .session_tree_members(root, &self.deadline()?)
            .await?;
        for path in members {
            Self::install_tree_path(source, &path)?;
        }
        Ok(())
    }
}

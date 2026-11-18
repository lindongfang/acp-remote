//! Installs a test fault only on the requested original ACP source instance.
use super::*;
use crate::adapters::acp::{
    capacity_close_write_fault::{
        CloseWriteFaultGuard, CloseWriteFaultMode, ResumeEnqueueFaultGuard,
    },
    native_dispatch::AcpSource,
};

fn resolve_source<P, L, G, N>(
    runtime: &NativeRuntimeHandle<P, L, G, N>,
    source_id: EntityId,
    instance_id: EntityId,
    connection_epoch: EntityId,
) -> Result<Arc<runtime::SourceSlot<P, L>>, DispatchError>
where
    P: NativeApprovalPort<Source = AcpSource>,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + NativeApprovalRepository,
{
    let sources = runtime
        .core
        .sources
        .lock()
        .map_err(|_| DispatchError::Stopped)?;
    sources
        .iter()
        .find(|source| {
            let metadata = &source.pre.metadata;
            metadata.source_id == source_id
                && metadata.instance_id == instance_id
                && metadata.connection_epoch == connection_epoch
        })
        .cloned()
        .ok_or(DispatchError::InvalidTarget)
}

#[allow(
    dead_code,
    reason = "Only the opt-in real-Harness Node acceptance entrypoints use this port"
)]
pub fn install<P, L, G, N>(
    runtime: &NativeRuntimeHandle<P, L, G, N>,
    source_id: EntityId,
    instance_id: EntityId,
    connection_epoch: EntityId,
    native: &str,
    mode: CloseWriteFaultMode,
) -> Result<CloseWriteFaultGuard, DispatchError>
where
    P: NativeApprovalPort<Source = AcpSource>,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + NativeApprovalRepository,
{
    let source = resolve_source(runtime, source_id, instance_id, connection_epoch)?;
    source
        .source
        .get()
        .ok_or(DispatchError::ProtocolUnavailable)?
        .capacity_install_close_write_fault(native, mode)
}

/// 为指定原连接实例上的单一目标会话安装一次性 `session/resume` 入队拒绝守卫。
#[allow(
    dead_code,
    reason = "Only the opt-in real-Harness Node acceptance entrypoints use this port"
)]
pub fn install_resume_enqueue_fault<P, L, G, N>(
    runtime: &NativeRuntimeHandle<P, L, G, N>,
    source_id: EntityId,
    instance_id: EntityId,
    connection_epoch: EntityId,
    native: &str,
) -> Result<ResumeEnqueueFaultGuard, DispatchError>
where
    P: NativeApprovalPort<Source = AcpSource>,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + NativeApprovalRepository,
{
    let source = resolve_source(runtime, source_id, instance_id, connection_epoch)?;
    source
        .source
        .get()
        .ok_or(DispatchError::ProtocolUnavailable)?
        .capacity_install_resume_enqueue_fault(native)
}

#[allow(
    dead_code,
    reason = "Only opt-in Node fault recovery wakes this original closer"
)]
pub fn request_metadata_retry<P, L, G, N>(
    runtime: &NativeRuntimeHandle<P, L, G, N>,
    source_id: EntityId,
    instance_id: EntityId,
    connection_epoch: EntityId,
) -> Result<(), DispatchError>
where
    P: NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + NativeApprovalRepository,
{
    let sources = runtime
        .core
        .sources
        .lock()
        .map_err(|_| DispatchError::Stopped)?;
    let source = sources
        .iter()
        .find(|source| {
            let metadata = &source.pre.metadata;
            metadata.source_id == source_id
                && metadata.instance_id == instance_id
                && metadata.connection_epoch == connection_epoch
        })
        .cloned()
        .ok_or(DispatchError::InvalidTarget)?;
    drop(sources);
    NativeCoordinator::<P, L, G, N>::request_source_metadata_retry(&source);
    Ok(())
}

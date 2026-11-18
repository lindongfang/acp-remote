// Test-only module mounted directly under native_dispatch.
// Concrete Node tests can inspect clocks without obtaining source write access.
use super::*;
type L6SourceKnownWork = (
    EntityId,
    Arc<str>,
    crate::domain::session_capacity::Coverage,
    crate::domain::session_capacity::Coverage,
    usize,
    Option<Arc<str>>,
    usize,
);
type L6SourceCandidateClock = (EntityId, Arc<str>, Option<std::time::Instant>);

pub fn rt_source_activity_clocks<P, L, G, N>(
    runtime: &NativeRuntimeHandle<P, L, G, N>,
) -> Result<Vec<(EntityId, std::sync::Arc<str>, std::time::Instant)>, DispatchError>
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
        .map_err(|_| DispatchError::Stopped)?
        .clone();
    let mut clocks = Vec::new();
    for source in sources {
        let source_id = source.pre.metadata.source_id;
        let observed = source
            .capacity_activity
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .rt_activity_clocks_for_test();
        clocks.extend(
            observed
                .into_iter()
                .map(|(native, received)| (source_id, native, received)),
        );
    }
    clocks.sort_by(|left, right| {
        left.0
            .encode()
            .cmp(&right.0.encode())
            .then_with(|| left.1.cmp(&right.1))
    });
    Ok(clocks)
}

pub fn rt_selected_deadline<P, L, G, N>(
    runtime: &NativeRuntimeHandle<P, L, G, N>,
    session: &str,
) -> Result<(crate::domain::approval_owner::ApprovalReplyId, Instant), DispatchError>
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
        .map_err(|_| DispatchError::Stopped)?
        .clone();
    for source in sources {
        let permissions: Vec<_> = source
            .permissions
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .values()
            .filter(|permission| permission.session.as_ref() == session)
            .cloned()
            .collect();
        for permission in permissions {
            let owner = permission
                .response_owner
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            if let Some(super::environment::PermissionResponseOwner::Selected(operation)) =
                owner.as_ref()
            {
                return Ok((
                    operation.id,
                    *operation
                        .write_until
                        .lock()
                        .map_err(|_| DispatchError::Stopped)?,
                ));
            }
        }
    }
    Err(DispatchError::InvalidTarget)
}

pub fn rt_source_candidate_clocks<P, L, G, N>(
    runtime: &NativeRuntimeHandle<P, L, G, N>,
) -> Result<Vec<L6SourceCandidateClock>, DispatchError>
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
        .map_err(|_| DispatchError::Stopped)?
        .clone();
    let mut values = Vec::new();
    for s in sources {
        let clocks = s
            .capacity_activity
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .l6_candidate_clocks_for_test();
        values.extend(
            clocks
                .into_iter()
                .map(|(id, clock)| (s.pre.metadata.source_id, id, clock)),
        );
    }
    Ok(values)
}

pub fn rt_source_known_work<P, L, G, N>(
    runtime: &NativeRuntimeHandle<P, L, G, N>,
) -> Result<Vec<L6SourceKnownWork>, DispatchError>
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
        .map_err(|_| DispatchError::Stopped)?
        .clone();
    let mut all = Vec::new();
    for source in sources {
        let work = source
            .capacity_activity
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .l6_known_work_for_test();
        let permissions = source
            .permissions
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for (session, foreground, background, tools, root) in work {
            let mut pending = 0;
            for permission in &permissions {
                if permission.session.as_ref() == session.as_ref() {
                    let state = permission.snapshot()?;
                    if !state.written && !state.retired {
                        pending += 1;
                    }
                }
            }
            all.push((
                source.pre.metadata.source_id,
                session,
                foreground,
                background,
                tools,
                root,
                pending,
            ));
        }
    }
    Ok(all)
}

pub fn l6_automatic_waits<P, L, G, N>(
    runtime: &NativeRuntimeHandle<P, L, G, N>,
) -> Result<Vec<(EntityId, Arc<str>, EntityId)>, DispatchError>
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
        .map_err(|_| DispatchError::Stopped)?
        .clone();
    let mut values = Vec::new();
    for source in sources {
        let capacities = source
            .capacity
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for capacity in capacities {
            let operation = capacity
                .automatic
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .clone();
            if let Some(operation) = operation {
                values.push((
                    source.pre.metadata.source_id,
                    Arc::from(capacity.pre.native.native_id()),
                    operation.id,
                ));
            }
        }
    }
    Ok(values)
}

pub type L6ActiveTools = (EntityId, EntityId, Arc<str>, Vec<Arc<str>>);

pub fn l6_active_tools<P, L, G, N>(
    runtime: &NativeRuntimeHandle<P, L, G, N>,
) -> Result<Vec<L6ActiveTools>, DispatchError>
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
        .map_err(|_| DispatchError::Stopped)?
        .clone();
    let mut values = Vec::new();
    for source in sources {
        let tools = source
            .capacity_activity
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .l6_active_tool_ids_for_test();
        values.extend(tools.into_iter().map(|(session, tools)| {
            (
                source.pre.metadata.source_id,
                source.pre.metadata.instance_id,
                session,
                tools,
            )
        }));
    }
    Ok(values)
}

pub fn l6_instance_raw_facts<P, L, G, N, T, F>(
    runtime: &NativeRuntimeHandle<P, L, G, N>,
    inspect: F,
) -> Result<Vec<(EntityId, EntityId, usize, T)>, DispatchError>
where
    P: NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + NativeApprovalRepository,
    F: Fn(&P::RawEvent) -> Option<T>,
{
    let sources = runtime
        .core
        .sources
        .lock()
        .map_err(|_| DispatchError::Stopped)?
        .clone();
    let mut values = Vec::new();
    for source in sources {
        let raw = source.raw.lock().map_err(|_| DispatchError::Stopped)?;
        for (index, event) in raw.iter().enumerate() {
            if let Some(fact) = inspect(event) {
                values.push((
                    source.pre.metadata.source_id,
                    source.pre.metadata.instance_id,
                    index,
                    fact,
                ));
            }
        }
    }
    Ok(values)
}

#[derive(Clone, Debug)]
pub struct L6CapacityPostState {
    pub foreground: crate::domain::session_capacity::Coverage,
    pub background: crate::domain::session_capacity::Coverage,
    pub active_tools: Vec<Arc<str>>,
}

#[derive(Clone, Debug)]
pub struct L6CapacityReceipt {
    pub source: EntityId,
    pub instance: EntityId,
    pub session: Arc<str>,
    pub raw_index: usize,
    pub sequence: u64,
    pub receipt: u64,
    pub freshness: &'static str,
    pub post: Option<L6CapacityPostState>,
}

struct L6CapacityReceiptOwner {
    core: usize,
    source: EntityId,
    records: std::sync::Mutex<Vec<L6CapacityReceipt>>,
}

static L6_RECEIPT_WATCHES: std::sync::Mutex<Vec<std::sync::Weak<L6CapacityReceiptOwner>>> =
    std::sync::Mutex::new(Vec::new());

pub struct L6CapacityReceiptWatch {
    owner: Arc<L6CapacityReceiptOwner>,
}

impl L6CapacityReceiptWatch {
    pub fn records(&self) -> Result<Vec<L6CapacityReceipt>, DispatchError> {
        // A poisoned registration lock means observation may have failed.
        // Missing evidence must never become a successful empty snapshot.
        drop(
            L6_RECEIPT_WATCHES
                .lock()
                .map_err(|_| DispatchError::Stopped)?,
        );
        self.owner
            .records
            .lock()
            .map_err(|_| DispatchError::Stopped)
            .map(|items| items.clone())
    }
}

impl Drop for L6CapacityReceiptWatch {
    fn drop(&mut self) {
        if let Ok(mut registrations) = L6_RECEIPT_WATCHES.lock() {
            registrations.retain(|entry| {
                entry
                    .upgrade()
                    .is_some_and(|owner| !Arc::ptr_eq(&owner, &self.owner))
            });
        }
    }
}

pub fn l6_watch_capacity_receipts<P, L, G, N>(
    runtime: &NativeRuntimeHandle<P, L, G, N>,
    source: EntityId,
) -> Result<L6CapacityReceiptWatch, DispatchError>
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
    let owner = Arc::new(L6CapacityReceiptOwner {
        core: Arc::as_ptr(&runtime.core) as usize,
        source,
        records: std::sync::Mutex::new(Vec::new()),
    });
    let mut registrations = L6_RECEIPT_WATCHES
        .lock()
        .map_err(|_| DispatchError::Stopped)?;
    registrations.retain(|entry| entry.strong_count() > 0);
    if registrations
        .iter()
        .filter_map(std::sync::Weak::upgrade)
        .any(|registered| registered.core == owner.core && registered.source == source)
    {
        return Err(DispatchError::InvalidSource);
    }
    registrations.push(Arc::downgrade(&owner));
    drop(registrations);
    Ok(L6CapacityReceiptWatch { owner })
}

pub fn l6_capacity_receipts<P, L, G, N>(
    runtime: &NativeRuntimeHandle<P, L, G, N>,
    source: EntityId,
) -> Result<Vec<L6CapacityReceipt>, DispatchError>
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
    let core = Arc::as_ptr(&runtime.core) as usize;
    let registrations = L6_RECEIPT_WATCHES
        .lock()
        .map_err(|_| DispatchError::Stopped)?;
    let owner = registrations
        .iter()
        .filter_map(std::sync::Weak::upgrade)
        .find(|owner| owner.core == core && owner.source == source)
        .ok_or(DispatchError::InvalidSource)?;
    drop(registrations);
    owner
        .records
        .lock()
        .map_err(|_| DispatchError::Stopped)
        .map(|items| items.clone())
}

// Called only after the original capacity projection has applied successfully.
// No registration means no retained evidence; this cannot change ACK or policy.
pub(super) fn l6_record_capacity_receipt(core: usize, event: &L6CapacityReceipt) {
    let Ok(registrations) = L6_RECEIPT_WATCHES.lock() else {
        return;
    };
    for registration in registrations.iter() {
        if let Some(owner) = registration.upgrade()
            && owner.core == core
            && owner.source == event.source
            && let Ok(mut records) = owner.records.lock()
        {
            records.push(event.clone());
        }
    }
}

#[test]
fn l6_capacity_receipt_collector_preserves_freshness_and_original_identity() {
    let source = EntityId::from_bytes([231; 16]);
    let owner = Arc::new(L6CapacityReceiptOwner {
        core: 17,
        source,
        records: std::sync::Mutex::new(Vec::new()),
    });
    L6_RECEIPT_WATCHES
        .lock()
        .unwrap()
        .push(Arc::downgrade(&owner));
    let watch = L6CapacityReceiptWatch { owner };
    let event = L6CapacityReceipt {
        source,
        instance: EntityId::from_bytes([232; 16]),
        session: Arc::from("owned"),
        raw_index: 7,
        sequence: 18,
        receipt: 29,
        freshness: "history",
        post: Some(L6CapacityPostState {
            foreground: crate::domain::session_capacity::Coverage::Idle,
            background: crate::domain::session_capacity::Coverage::Active,
            active_tools: vec![Arc::from("original-tool")],
        }),
    };
    l6_record_capacity_receipt(18, &event);
    assert!(watch.records().unwrap().is_empty());
    l6_record_capacity_receipt(17, &event);
    let records = watch.records().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].freshness, "history");
    assert_eq!(records[0].session.as_ref(), "owned");
    let post = records[0].post.as_ref().unwrap();
    assert_eq!(
        post.background,
        crate::domain::session_capacity::Coverage::Active
    );
    assert_eq!(post.active_tools, vec![Arc::from("original-tool")]);
    assert_eq!(
        (
            records[0].raw_index,
            records[0].sequence,
            records[0].receipt
        ),
        (7, 18, 29)
    );
    let owner = Arc::downgrade(&watch.owner);
    drop(watch);
    assert!(owner.upgrade().is_none());
    assert!(
        !L6_RECEIPT_WATCHES
            .lock()
            .unwrap()
            .iter()
            .any(|item| item.ptr_eq(&owner))
    );
}

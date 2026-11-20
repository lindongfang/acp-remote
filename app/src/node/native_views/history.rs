use super::{
    Arc, NativeSessionKey, NativeSocketViewRead, NodeNativeViewsService, OriginalReadProgress,
    ReadMode, ReadReason, SocketNativeViewAdmission, SocketReadBacking, SocketReadPurpose,
    SocketRebindTarget, SocketViewAuthority, SocketViewClient, TranscriptCacheCoordinator,
    TranscriptSyncState, ViewError, prepare_cold_status,
};
use crate::{
    application::shared_views::reader::HistoryWindow,
    domain::history_page::{
        Direction, PageError, TurnIndexer, TurnRange,
        cursor::{CursorScope, Generation},
    },
    identity::NodeIdentity,
    management_ws::session_protocol::{HistoryMode, HistoryPageInput},
    transcript_cache::PageReadError,
};
use serde_json::{Value, json};

pub enum HistoryError {
    View(ViewError),
    Page(PageError),
}
impl From<ViewError> for HistoryError {
    fn from(e: ViewError) -> Self {
        Self::View(e)
    }
}
impl From<PageError> for HistoryError {
    fn from(e: PageError) -> Self {
        Self::Page(e)
    }
}

#[derive(Clone)]
pub(super) struct BoundHistory {
    pub request: HistoryRequest,
    pub key: NativeSessionKey,
    pub tc: Arc<TranscriptCacheCoordinator>,
    pub metadata: Value,
}
#[derive(Clone)]
pub struct HistoryRequest {
    pub input: HistoryPageInput,
    pub identity: Arc<NodeIdentity>,
    pub open: Option<ReadReason>,
}
impl HistoryRequest {
    fn check_child_boundaries(
        &self,
        range: Option<TurnRange>,
        has_records: bool,
    ) -> Result<(), HistoryError> {
        if self.input.child.is_some() && range.is_none() && has_records {
            return Err(ViewError::UnsupportedCapability.into());
        }
        Ok(())
    }
    fn scope(&self, direction: Direction) -> CursorScope {
        CursorScope {
            direction,
            target: self.input.target.fingerprint(self.input.child.as_deref()),
            size: self.input.turns.unwrap_or_default(),
        }
    }
    fn select(&self, generation: Generation, total: u64) -> Result<Option<TurnRange>, PageError> {
        let size = self.input.turns.unwrap_or_default();
        Ok(Some(match &self.input.mode {
            HistoryMode::First {} if total == 0 => return Ok(None),
            HistoryMode::First {} => TurnRange::first(total, size, self.input.child.is_some())?,
            HistoryMode::Locate {
                from_turn,
                direction,
            } => TurnRange::locate(total, size, *direction, from_turn.as_u64())?,
            HistoryMode::Continue { cursor, direction } => {
                let anchor = self.scope(*direction).verify(
                    generation,
                    cursor,
                    self.identity.public_key(),
                )?;
                TurnRange::locate(total, size, *direction, anchor)?
            }
        }))
    }
    fn require_replayable_mode(&self) -> Result<(), PageError> {
        if let HistoryMode::Continue { cursor, direction } = &self.input.mode {
            self.scope(*direction)
                .authenticate(cursor, self.identity.public_key())?;
            return Err(PageError::StaleCursor);
        }
        Ok(())
    }
    fn metadata(
        &self,
        generation: Generation,
        total: u64,
        range: Option<TurnRange>,
        window: &HistoryWindow,
        source: &str,
        prelude: bool,
    ) -> Value {
        let issue = |direction, anchor| {
            self.scope(direction)
                .issue(generation, anchor, |bytes| self.identity.sign(bytes))
        };
        let relocated = matches!(self.input.mode, HistoryMode::Locate { .. });
        let turns: Vec<_> = window
            .turns
            .iter()
            .map(|(turn, position)| {
                json!({
                    "turn": turn.to_string(), "firstPosition": position.to_string()
                })
            })
            .collect();
        json!({
            "generationId": generation.encode(),
            "source": source,
            "turnStatus": if range.is_some() {"provable"} else {"unprovable"},
            "order": "ascending",
            "totalTurns": total.to_string(),
            "firstTurn": range.map(|r|r.first.to_string()),
            "lastTurn": range.map(|r|r.last.to_string()),
            "turns": turns,
            "prelude": prelude,
            "olderCursor": range.and_then(TurnRange::older_anchor).map(|anchor|issue(Direction::Older,anchor)),
            "newerCursor": range.and_then(|r|r.newer_anchor(total)).map(|anchor|issue(Direction::Newer,anchor)),
            "relocated": relocated,
            "continuity": if relocated {"unproven"} else {"same_generation"},
            "liveTail": window.live
        })
    }
}
impl NodeNativeViewsService {
    pub(crate) async fn history(
        &self,
        client: &SocketViewClient,
        target: SocketRebindTarget,
        request: HistoryRequest,
    ) -> Result<NativeSocketViewRead, HistoryError> {
        let authority = self
            .socket_authority
            .as_ref()
            .ok_or(ViewError::CaptureUnavailable)?;
        let admission = self
            .socket_admission
            .as_ref()
            .ok_or(ViewError::CaptureUnavailable)?;
        if !Arc::ptr_eq(authority, client.context.authority()) {
            return Err(ViewError::InvalidClient.into());
        }
        let reason = request.open.unwrap_or(ReadReason::FirstOpen);
        let (parent, key) = target
            .authorize(
                admission.as_ref(),
                client,
                reason,
                request.input.child.as_deref(),
                None,
            )
            .await?;
        let tc = self
            .transcript_cache
            .as_ref()
            .ok_or(ViewError::CaptureUnavailable)?;
        let cached = CachedHistory {
            tc,
            admission,
            authority,
            client,
            key: key.clone(),
            target: target.clone(),
        };
        let live = admission.try_open_live_socket(&key).await?;
        if live.is_some() {
            let _ = tc.store().writer().flush().await;
        }
        if (request.open.is_none() || live.is_none())
            && let Some(page) = cached.page(&request).await?
        {
            return cached.open(request, page, "transcript_cache").await;
        }
        let (read, source) = if let Some(live) = live.filter(|candidate| {
            (candidate.is_loaded || candidate.replay_complete)
                && reason != ReadReason::ExplicitRefresh
        }) {
            let read = live.reader.open_current(client, &key).await?;
            (
                NativeSocketViewRead {
                    read,
                    reader: live.reader,
                    history: None,
                    backing: SocketReadBacking::Live(Some(tc.live_status(
                        &key,
                        live.hybrid_preloaded,
                        live.replay_complete,
                    ))),
                },
                "live",
            )
        } else {
            request.require_replayable_mode()?;
            if !admission.history_replay_allowed(&parent)?
                || !admission.history_replay_allowed(&key)?
            {
                return Err(ViewError::InvalidStage.into());
            }
            let admitted = target
                .admit(
                    admission.as_ref(),
                    client,
                    reason,
                    &key,
                    request.input.child.as_deref(),
                )
                .await?;
            let cap = admitted.capability.clone();
            let replay = self
                .finish_history_replay(
                    client,
                    admitted,
                    reason,
                    &key,
                    tc,
                    request.input.child.is_some(),
                )
                .await;
            let read = match replay {
                Err(HistoryError::View(ViewError::UnsupportedCapability))
                    if request.input.child.is_some() =>
                {
                    return cached.query_child(request, parent, cap, reason).await;
                }
                other => other?,
            };
            (read, "replay")
        };
        request.select_live(client, read, &key, tc, source).await
    }
}
impl NodeNativeViewsService {
    async fn finish_history_replay(
        &self,
        client: &SocketViewClient,
        admitted: super::AdmittedSocketNativeView,
        reason: ReadReason,
        key: &NativeSessionKey,
        tc: &TranscriptCacheCoordinator,
        child: bool,
    ) -> Result<NativeSocketViewRead, HistoryError> {
        let mut read = self
            .finish_live_socket_read(client, admitted, reason)
            .await?;
        let mode = read.reader.wait_read_mode(client, &read.read, true).await?;
        if mode == ReadMode::CurrentOnly {
            if !child {
                return Err(ViewError::UnsupportedCapability.into());
            }
            let (version, _) = tc
                .known_source_capabilities(key.source())
                .ok_or(ViewError::UnsupportedCapability)?;
            let journal = read.reader.history_records(client, &read.read)?;
            let mut turns = TurnIndexer::default();
            for position in 0..journal.records.len() {
                let record = journal.records.get(position)?;
                turns.observe(crate::adapters::acp::turn_classify::classify(
                    version,
                    record.raw.message(),
                ));
            }
            if !turns.provable() {
                return Err(ViewError::UnsupportedCapability.into());
            }
        } else {
            if !matches!(read.original_progress(client).await?,OriginalReadProgress::Complete{epoch,..} if epoch==read.descriptor().view_epoch())
            {
                let next = read.rebind_original(client).await?;
                let _ = read.release();
                read = next;
            }
            tc.wait_pending_replay_persist(key).await;
        }
        Ok(read)
    }
}
impl HistoryRequest {
    pub(super) async fn select_live(
        self,
        client: &SocketViewClient,
        mut read: NativeSocketViewRead,
        key: &NativeSessionKey,
        tc: &Arc<TranscriptCacheCoordinator>,
        source: &str,
    ) -> Result<NativeSocketViewRead, HistoryError> {
        let request = self;
        if request.open.is_none() {
            read.read = read
                .reader
                .open_history_snapshot(client, &read.read)
                .await?;
        }
        let mut generation = Generation::Live(
            crate::transcript_cache::entity_id_to_bytes(
                read.descriptor().view_epoch().as_entity_id(),
            )
            .map_err(|_| ViewError::InvalidCursor)?,
        );
        let version = tc.known_source_capabilities(key.source()).map(|v| v.0);
        let journal = read.reader.history_records(client, &read.read)?;
        let mut index = TurnIndexer::default();
        let mut starts = Vec::new();
        let mut digest = version.map(crate::transcript_cache::IncrementalDigest::new);
        if let (Some(version), Some(hash)) = (version, digest.as_mut()) {
            for i in 0..journal.records.len() {
                let raw = journal.records.get(i)?;
                hash.push_frame(raw.raw.message())
                    .map_err(|_| ViewError::ReadFailure)?;
                if let Some(t) = index.observe(crate::adapters::acp::turn_classify::classify(
                    version,
                    raw.raw.message(),
                )) {
                    starts.push((t, raw.revision, i));
                }
            }
        }
        let total = index.turn_count();
        if let Some(digest) = digest
            && total > 0
            && let Ok(Some(token)) = tc
                .store()
                .matching_generation(key, digest.finalize_snapshot())
                .await
        {
            generation = Generation::Cache(token);
        }
        let range = request.select(generation, total)?;
        request.check_child_boundaries(range, journal.records.len() > 0)?;
        let mut window = HistoryWindow {
            records: 0..journal.records.len(),
            turns: Vec::new(),
            live: request.open.is_some(),
            unprovable: range.is_none(),
            invalidated: None,
        };
        let mut prelude = false;
        if let Some(range) = range {
            for (turn, pos, idx) in starts {
                if turn == range.first {
                    if turn > 1 {
                        window.records.start = idx;
                    } else {
                        prelude = idx > 0;
                    }
                }
                if turn > range.last {
                    window.records.end = idx;
                    break;
                }
                if turn >= range.first {
                    window.turns.push((turn, pos));
                }
            }
        }
        let metadata = request.metadata(generation, total, range, &window, source, prelude);
        let tail = version
            .filter(|_| window.live && !window.unprovable)
            .map(
                |version| crate::application::shared_views::reader::HistoryTail {
                    index,
                    next: journal.records.len(),
                    classify: Box::new(
                        move |raw: &crate::adapters::acp::shared_view_capture::RetainedAcpRaw| {
                            crate::adapters::acp::turn_classify::classify(version, raw.message())
                        },
                    ),
                },
            );
        read.reader
            .select_history_window(client, &read.read, window, tail)?;
        read.reader.check_current(client, key).await?;
        read.history = Some(BoundHistory {
            request,
            key: key.clone(),
            tc: tc.clone(),
            metadata,
        });
        Ok(read)
    }
}

struct CachedHistory<'a> {
    tc: &'a Arc<TranscriptCacheCoordinator>,
    admission: &'a Arc<dyn SocketNativeViewAdmission>,
    authority: &'a Arc<SocketViewAuthority>,
    client: &'a SocketViewClient,
    key: NativeSessionKey,
    target: SocketRebindTarget,
}
impl CachedHistory<'_> {
    async fn page(
        &self,
        request: &HistoryRequest,
    ) -> Result<Option<crate::transcript_cache::CachedPage>, HistoryError> {
        match self
            .tc
            .store()
            .read_turn_page(&self.key, |g, t| request.select(Generation::Cache(g), t))
            .await
        {
            Ok(page) => Ok(page),
            Err(PageReadError::Page(e)) => Err(e.into()),
            Err(PageReadError::Cache(_)) => Ok(None),
        }
    }
    async fn query_child(
        self,
        request: HistoryRequest,
        parent: NativeSessionKey,
        cap: Arc<super::SocketReadCapability>,
        reason: ReadReason,
    ) -> Result<NativeSocketViewRead, HistoryError> {
        let views = cap.source().views()?;
        let verify_target = self.target.clone();
        let verify_admission = self.admission.clone();
        let context = self.client.context.clone();
        let expected = (parent, self.key.clone());
        let child = request.input.child.clone();
        let root = cap.root_native().clone();
        let configuration = cap.source_configuration();
        let check: Arc<crate::adapters::acp::native_views::child_history::Check> =
            Arc::new(move || {
                let target = verify_target.clone();
                let admission = verify_admission.clone();
                let client = context.client();
                let expected = expected.clone();
                let child = child.clone();
                let root = root.clone();
                Box::pin(async move {
                    let actual = target
                        .authorize(
                            admission.as_ref(),
                            &client,
                            reason,
                            child.as_deref(),
                            Some((&root, configuration)),
                        )
                        .await?;
                    if actual != expected {
                        return Err(ViewError::InvalidTarget);
                    }
                    if !admission.history_replay_allowed(&actual.0)? {
                        return Err(ViewError::InvalidStage);
                    }
                    client
                        .context
                        .authority()
                        .current_guard(&client, &actual.1)
                        .await
                })
            });
        views.child_history(cap, check, self.tc.clone()).await?;
        let page = self.page(&request).await?.ok_or(ViewError::ReadFailure)?;
        self.open(request, page, "replay").await
    }
    async fn open(
        self,
        request: HistoryRequest,
        mut page: crate::transcript_cache::CachedPage,
        source: &str,
    ) -> Result<NativeSocketViewRead, HistoryError> {
        let Self {
            tc,
            admission,
            authority,
            client,
            key,
            target,
        } = self;
        let generation = Generation::Cache(page.generation);
        request.check_child_boundaries(page.range, !page.snapshot.records.is_empty())?;
        let first = page.snapshot.records.first().map_or(1, |r| r.ordinal);
        let prelude = page
            .boundaries
            .first()
            .is_some_and(|&(t, p)| t == 1 && p > first);
        let prefix = page.prior_tool_anchors.len();
        let window = HistoryWindow {
            records: prefix..prefix + page.snapshot.records.len(),
            turns: page
                .boundaries
                .iter()
                .map(|&(t, p)| (t, p - first + 1 + prefix as u64))
                .collect(),
            live: false,
            unprovable: page.range.is_none(),
            invalidated: None,
        };
        let metadata =
            request.metadata(generation, page.total, page.range, &window, source, prelude);
        page.prior_tool_anchors.append(&mut page.snapshot.records);
        page.snapshot.records = page.prior_tool_anchors;
        page.snapshot.records.append(&mut page.tool_updates);
        let (mut status, needs_probe) = prepare_cold_status(
            tc,
            &page.snapshot.session,
            request.open.unwrap_or(ReadReason::FirstOpen),
        );
        let profile = page.snapshot.session.capability_profile;
        if request.open.is_none() && needs_probe {
            status.sync_state = TranscriptSyncState::Idle;
        }
        let (read, reader, handle) = tc
            .open_cached_socket_view(
                &key,
                client,
                authority,
                page.snapshot,
                status,
                request.open.is_none(),
            )
            .await?;
        reader.select_history_window(client, &read, window, None)?;
        if needs_probe && request.open.is_some() {
            probe_cached_page(admission, client, &key, tc, &target, profile).await;
        }
        reader.check_current(client, &key).await?;
        Ok(NativeSocketViewRead {
            read,
            reader,
            history: Some(BoundHistory {
                request,
                key,
                tc: tc.clone(),
                metadata,
            }),
            backing: SocketReadBacking::Cached {
                handle,
                authority: authority.clone(),
                admission: admission.clone(),
                target,
                coordinator: tc.clone(),
            },
        })
    }
}

async fn probe_cached_page(
    admission: &Arc<dyn SocketNativeViewAdmission>,
    client: &SocketViewClient,
    key: &NativeSessionKey,
    tc: &Arc<TranscriptCacheCoordinator>,
    target: &SocketRebindTarget,
    profile: super::CapabilityProfile,
) {
    let adm = admission.clone();
    let c = client.context.client();
    let k = key.clone();
    let coord = tc.clone();
    let t = target.clone();
    tc.spawn_singleflight_revalidation(key.clone(), async move {
        match t {
            SocketRebindTarget::Target(id) => {
                adm.probe_and_revalidate_socket_target(c, id, k, profile, coord)
                    .await;
            }
            SocketRebindTarget::Source {
                selection,
                native_session_id,
            } => {
                adm.probe_and_revalidate_socket_source(
                    c,
                    selection,
                    native_session_id,
                    k,
                    profile,
                    coord,
                )
                .await;
            }
        }
    })
    .await;
}

impl SocketRebindTarget {
    async fn authorize(
        &self,
        admission: &dyn SocketNativeViewAdmission,
        client: &SocketViewClient,
        reason: ReadReason,
        child: Option<&str>,
        binding: Option<(&NativeSessionKey, (crate::domain::EntityId, u64))>,
    ) -> Result<(NativeSessionKey, NativeSessionKey), ViewError> {
        let configuration = binding.map(|(_, config)| config);
        let parent = match self {
            Self::Target(id) => {
                admission
                    .verify_cold_target(client, *id, reason, configuration)
                    .await?
            }
            Self::Source {
                selection,
                native_session_id,
            } => {
                if configuration.is_some_and(|expected| {
                    expected != (selection.profile.id(), selection.profile.revision())
                }) {
                    return Err(ViewError::InvalidConfiguration);
                }
                admission
                    .verify_cold_source(client, selection.clone(), native_session_id.clone())
                    .await?
            }
        };
        let key = if let Some(child) = child {
            admission
                .history_child(&parent, child, binding.map(|(root, _)| root))
                .await?
        } else {
            parent.clone()
        };
        Ok((parent, key))
    }
    async fn admit(
        self,
        admission: &dyn SocketNativeViewAdmission,
        client: &SocketViewClient,
        reason: ReadReason,
        key: &NativeSessionKey,
        child: Option<&str>,
    ) -> Result<super::AdmittedSocketNativeView, ViewError> {
        Ok(match self {
            Self::Target(id) => {
                admission
                    .admit_socket(
                        client,
                        id,
                        reason,
                        SocketReadPurpose::History(child.map(|_| key.clone())),
                    )
                    .await?
            }
            Self::Source { selection, .. } => {
                admission
                    .admit_socket_source(
                        client,
                        selection,
                        key.native_id().to_owned(),
                        child.is_some(),
                    )
                    .await?
            }
        })
    }
}

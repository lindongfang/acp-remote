use crate::{
    adapters::acp::{
        node_configuration_profile::{self as codec, field, id, object, raw_error, revision, text},
        raw_json::{RawDocument, RawObject},
    },
    domain::{
        EntityId,
        native_catalog::{
            CatalogAction, CatalogDraft, CatalogError, CatalogRequestId, DraftIntent,
            LaunchProfileDraft, ProfileKey, ProfilePublication, SourceSelection,
        },
    },
};
use zeroize::Zeroizing;
pub(super) enum Body {
    Install {
        name: String,
        source_name: Option<String>,
    },
    Publish {
        expected: Option<ProfileKey>,
    },
    Bind {
        target: EntityId,
        workspace: EntityId,
        launch_path: String,
        canonical: String,
    },
}
pub(super) struct Intent {
    pub(super) action: CatalogAction,
    pub(super) harness: EntityId,
    pub(super) source: EntityId,
    pub(super) profile: ProfileKey,
    pub(super) document: Option<Zeroizing<Vec<u8>>>,
    pub(super) body: Body,
    pub(super) directories: Vec<(EntityId, String)>,
}
pub(super) const fn label(a: CatalogAction) -> &'static str {
    match a {
        CatalogAction::InstallHarness => "harness.install",
        CatalogAction::PublishProfile => "profile.publish",
        CatalogAction::BindTargetInitial => "target.bind",
    }
}
pub(super) fn action(s: &str) -> Result<CatalogAction, CatalogError> {
    match s {
        "harness.install" => Ok(CatalogAction::InstallHarness),
        "profile.publish" => Ok(CatalogAction::PublishProfile),
        "target.bind" => Ok(CatalogAction::BindTargetInitial),
        _ => Err(CatalogError::CorruptState),
    }
}
pub(super) fn random_id() -> Result<EntityId, CatalogError> {
    crate::random::bytes()
        .map(EntityId::from_bytes)
        .map_err(|_| CatalogError::RandomUnavailable)
}
fn document(p: &LaunchProfileDraft) -> Result<Option<Zeroizing<Vec<u8>>>, CatalogError> {
    let p = codec::parse_profile(p.document().to_vec())?;
    Ok(Some(Zeroizing::new(p.document().to_vec())))
}
impl Intent {
    pub(super) fn normalize(draft: CatalogDraft) -> Result<Self, CatalogError> {
        let normalized: Result<Self, CatalogError> = match draft.into_intent() {
            DraftIntent::InstallHarness {
                name,
                source,
                profile,
            } => {
                let (source, source_name) = match source {
                    SourceSelection::Existing(id) => (id, None),
                    SourceSelection::New { name } => (random_id()?, Some(name)),
                };
                Ok(Self {
                    action: CatalogAction::InstallHarness,
                    harness: random_id()?,
                    source,
                    profile: ProfileKey::try_new(random_id()?, 1)?,
                    document: document(&profile)?,
                    directories: Vec::new(),
                    body: Body::Install { name, source_name },
                })
            }
            DraftIntent::PublishProfile {
                harness,
                source,
                publication,
                profile,
            } => {
                let (key, expected) = match publication {
                    ProfilePublication::New => (ProfileKey::try_new(random_id()?, 1)?, None),
                    ProfilePublication::Next { expected } => (
                        ProfileKey::try_new(
                            expected.id(),
                            expected
                                .revision()
                                .checked_add(1)
                                .ok_or(CatalogError::Conflict)?,
                        )?,
                        Some(expected),
                    ),
                };
                Ok(Self {
                    action: CatalogAction::PublishProfile,
                    harness,
                    source,
                    profile: key,
                    document: document(&profile)?,
                    directories: Vec::new(),
                    body: Body::Publish { expected },
                })
            }
            DraftIntent::BindTargetInitial {
                target,
                source,
                harness,
                profile,
                workspace,
                launch_path,
                observed_canonical_path,
            } => Ok(Self {
                action: CatalogAction::BindTargetInitial,
                harness,
                source,
                profile,
                document: None,
                directories: Vec::new(),
                body: Body::Bind {
                    target,
                    workspace,
                    launch_path,
                    canonical: observed_canonical_path,
                },
            }),
        };
        normalized?.complete()
    }
    fn complete(mut self) -> Result<Self, CatalogError> {
        if let Some(document) = &self.document {
            let decoded = codec::decode_profile(document.to_vec())?;
            for recipe in decoded.recipes {
                self.directories
                    .try_reserve(recipe.directories.len())
                    .map_err(|_| CatalogError::Allocation)?;
                for d in recipe.directories {
                    self.directories.push((d.workspace, d.path));
                }
            }
        }
        Ok(self)
    }
    pub(super) fn encode(
        &self,
        request: CatalogRequestId,
        actor: EntityId,
        generation: i64,
    ) -> Result<Zeroizing<Vec<u8>>, CatalogError> {
        let mut intent = serde_json::json!({"kind":label(self.action),"harness_id":self.harness.encode(),"source_id":self.source.encode(),"profile_id":self.profile.id().encode(),"profile_revision":self.profile.revision().to_string()});
        let map = intent.as_object_mut().ok_or(CatalogError::CorruptState)?;
        match &self.body {
            Body::Install { name, source_name } => {
                map.insert("harness_name".into(), serde_json::json!(name));
                map.insert(
                    "source".into(),
                    source_name.as_ref().map_or_else(
                        || serde_json::json!({"kind":"existing"}),
                        |name| serde_json::json!({"kind":"new","name":name}),
                    ),
                );
            }
            Body::Publish { expected } => {
                map.insert("publication".into(),expected.as_ref().map_or_else(||serde_json::json!({"kind":"new"}),|k|serde_json::json!({"kind":"next","expected_profile_id":k.id().encode(),"expected_revision":k.revision().to_string()})));
            }
            Body::Bind {
                target,
                workspace,
                launch_path,
                canonical,
            } => {
                map.insert("target_id".into(), serde_json::json!(target.encode()));
                map.insert("workspace_id".into(), serde_json::json!(workspace.encode()));
                map.insert("launch_path".into(), serde_json::json!(launch_path));
                map.insert(
                    "observed_canonical_path".into(),
                    serde_json::json!(canonical),
                );
            }
        }
        let encoded =
            serde_json::to_string(&intent).map_err(|_| CatalogError::InvalidConfiguration)?;
        let intent = if let Some(profile) = &self.document {
            let raw =
                std::str::from_utf8(profile).map_err(|_| CatalogError::InvalidConfiguration)?;
            format!("{},\"profile\":{raw}}}", &encoded[..encoded.len() - 1])
        } else {
            encoded
        };
        let encoded = format!(
            "{{\"format\":1,\"origin_request_id\":{},\"action\":{},\"actor_id\":{},\"register_auth_generation\":{generation},\"intent\":{intent}}}",
            quote(&request.encode())?,
            quote(label(self.action))?,
            quote(&actor.encode())?
        );
        Ok(Zeroizing::new(encoded.into_bytes()))
    }
    pub(super) fn decode(
        bytes: Vec<u8>,
        request: CatalogRequestId,
        actor: EntityId,
        generation: i64,
        expected_action: CatalogAction,
    ) -> Result<Self, CatalogError> {
        let root = RawDocument::parse(bytes).map_err(raw_error)?;
        root.require_unique_fields().map_err(raw_error)?;
        let envelope = object(
            root.root(),
            &[
                "format",
                "origin_request_id",
                "action",
                "actor_id",
                "register_auth_generation",
                "intent",
            ],
        )?;
        if field(envelope, "format")?.unsigned() != Some(1)
            || text(envelope, "origin_request_id")? != request.encode()
            || text(envelope, "action")? != label(expected_action)
            || id(envelope, "actor_id")? != actor
            || field(envelope, "register_auth_generation")?.integer() != Some(generation)
        {
            return Err(CatalogError::CorruptState);
        }
        let fields: &[&str] = match expected_action {
            CatalogAction::InstallHarness => &[
                "kind",
                "harness_id",
                "source_id",
                "profile_id",
                "profile_revision",
                "harness_name",
                "source",
                "profile",
            ],
            CatalogAction::PublishProfile => &[
                "kind",
                "harness_id",
                "source_id",
                "profile_id",
                "profile_revision",
                "publication",
                "profile",
            ],
            CatalogAction::BindTargetInitial => &[
                "kind",
                "harness_id",
                "source_id",
                "profile_id",
                "profile_revision",
                "target_id",
                "workspace_id",
                "launch_path",
                "observed_canonical_path",
            ],
        };
        let v = object(field(envelope, "intent")?, fields)?;
        if text(v, "kind")? != label(expected_action) {
            return Err(CatalogError::CorruptState);
        }
        let profile = ProfileKey::try_new(id(v, "profile_id")?, revision(v, "profile_revision")?)?;
        let body = decode_body(v, profile, expected_action)?;
        let document = if expected_action == CatalogAction::BindTargetInitial {
            None
        } else {
            document(&codec::parse_profile(
                field(v, "profile")?.fragment().as_bytes().to_vec(),
            )?)?
        };
        Ok(Self {
            action: expected_action,
            harness: id(v, "harness_id")?,
            source: id(v, "source_id")?,
            profile,
            document,
            body,
            directories: Vec::new(),
        })
        .and_then(Self::complete)
    }
}
fn decode_body(
    v: RawObject<'_>,
    profile: ProfileKey,
    expected_action: CatalogAction,
) -> Result<Body, CatalogError> {
    let body = match expected_action {
        CatalogAction::InstallHarness => {
            let source = field(v, "source")?;
            let o = source.object().ok_or(CatalogError::InvalidConfiguration)?;
            let source_name = match text(o, "kind")?.as_str() {
                "existing" => {
                    object(source, &["kind"])?;
                    None
                }
                "new" => {
                    let o = object(source, &["kind", "name"])?;
                    Some(text(o, "name")?)
                }
                _ => return Err(CatalogError::InvalidConfiguration),
            };
            let name = text(v, "harness_name")?;
            CatalogDraft::install_harness(
                name.clone(),
                match &source_name {
                    Some(name) => SourceSelection::New { name: name.clone() },
                    None => SourceSelection::Existing(id(v, "source_id")?),
                },
                codec::parse_profile(field(v, "profile")?.fragment().as_bytes().to_vec())?,
            )?;
            if profile.revision() != 1 {
                return Err(CatalogError::CorruptState);
            }
            Body::Install { name, source_name }
        }
        CatalogAction::PublishProfile => {
            let raw = field(v, "publication")?;
            let o = raw.object().ok_or(CatalogError::InvalidConfiguration)?;
            let expected = match text(o, "kind")?.as_str() {
                "new" => {
                    object(raw, &["kind"])?;
                    if profile.revision() != 1 {
                        return Err(CatalogError::CorruptState);
                    }
                    None
                }
                "next" => {
                    let o = object(raw, &["kind", "expected_profile_id", "expected_revision"])?;
                    let k = ProfileKey::try_new(
                        id(o, "expected_profile_id")?,
                        revision(o, "expected_revision")?,
                    )?;
                    if k.id() != profile.id()
                        || k.revision().checked_add(1) != Some(profile.revision())
                    {
                        return Err(CatalogError::CorruptState);
                    }
                    Some(k)
                }
                _ => return Err(CatalogError::InvalidConfiguration),
            };
            Body::Publish { expected }
        }
        CatalogAction::BindTargetInitial => {
            let target = id(v, "target_id")?;
            let workspace = id(v, "workspace_id")?;
            let launch_path = text(v, "launch_path")?;
            let canonical = text(v, "observed_canonical_path")?;
            CatalogDraft::bind_target_initial(
                target,
                id(v, "source_id")?,
                id(v, "harness_id")?,
                profile,
                workspace,
                launch_path.clone(),
                canonical.clone(),
            )?;
            Body::Bind {
                target,
                workspace,
                launch_path,
                canonical,
            }
        }
    };
    Ok(body)
}
fn quote(s: &str) -> Result<String, CatalogError> {
    serde_json::to_string(s).map_err(|_| CatalogError::InvalidConfiguration)
}

#[cfg(test)]
mod tests;

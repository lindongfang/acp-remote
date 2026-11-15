//! Closed upload envelope. Profiles keep their original JSON fragment; this codec grants no workspace authority.
use super::{
    node_configuration_profile::{self as codec, field, id, object, revision, text},
    raw_json::RawDocument,
};
use crate::domain::{
    EntityId,
    native_catalog::{CatalogDraft, CatalogError, ProfileKey, ProfilePublication, SourceSelection},
};
pub enum ParsedCatalogDocument {
    Complete(CatalogDraft),
    Bind {
        target: EntityId,
        source: EntityId,
        harness: EntityId,
        profile: ProfileKey,
        workspace: EntityId,
        launch_path: String,
    },
}
fn profile(
    v: super::raw_json::RawValueRef<'_>,
) -> Result<crate::domain::native_catalog::LaunchProfileDraft, CatalogError> {
    let raw = v.fragment().as_bytes();
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(raw.len())
        .map_err(|_| CatalogError::Allocation)?;
    bytes.extend_from_slice(raw);
    codec::parse_profile(bytes)
}
pub fn parse_catalog_document(bytes: Vec<u8>) -> Result<ParsedCatalogDocument, CatalogError> {
    let document = RawDocument::parse(bytes).map_err(codec::raw_error)?;
    let root = document
        .root()
        .object()
        .ok_or(CatalogError::InvalidConfiguration)?;
    match text(root, "kind")?.as_str() {
        "install_harness" => {
            let r = object(document.root(), &["kind", "name", "source", "profile"])?;
            let source = field(r, "source")?;
            let o = source.object().ok_or(CatalogError::InvalidConfiguration)?;
            let source = match text(o, "kind")?.as_str() {
                "existing" => {
                    let o = object(source, &["kind", "sourceId"])?;
                    SourceSelection::Existing(id(o, "sourceId")?)
                }
                "new" => {
                    let o = object(source, &["kind", "name"])?;
                    SourceSelection::New {
                        name: text(o, "name")?,
                    }
                }
                _ => return Err(CatalogError::InvalidConfiguration),
            };
            Ok(ParsedCatalogDocument::Complete(
                CatalogDraft::install_harness(
                    text(r, "name")?,
                    source,
                    profile(field(r, "profile")?)?,
                )?,
            ))
        }
        "publish_profile" => {
            let r = object(
                document.root(),
                &["kind", "harnessId", "sourceId", "publication", "profile"],
            )?;
            let publication = field(r, "publication")?;
            let o = publication
                .object()
                .ok_or(CatalogError::InvalidConfiguration)?;
            let publication = match text(o, "kind")?.as_str() {
                "new" => {
                    object(publication, &["kind"])?;
                    ProfilePublication::New
                }
                "next" => {
                    let o = object(publication, &["kind", "profileId", "expectedRevision"])?;
                    ProfilePublication::Next {
                        expected: ProfileKey::try_new(
                            id(o, "profileId")?,
                            revision(o, "expectedRevision")?,
                        )?,
                    }
                }
                _ => return Err(CatalogError::InvalidConfiguration),
            };
            Ok(ParsedCatalogDocument::Complete(
                CatalogDraft::publish_profile(
                    id(r, "harnessId")?,
                    id(r, "sourceId")?,
                    publication,
                    profile(field(r, "profile")?)?,
                )?,
            ))
        }
        "bind_target_initial" => {
            let r = object(
                document.root(),
                &[
                    "kind",
                    "targetId",
                    "sourceId",
                    "harnessId",
                    "profileId",
                    "profileRevision",
                    "workspaceId",
                    "launchPath",
                ],
            )?;
            Ok(ParsedCatalogDocument::Bind {
                target: id(r, "targetId")?,
                source: id(r, "sourceId")?,
                harness: id(r, "harnessId")?,
                profile: ProfileKey::try_new(id(r, "profileId")?, revision(r, "profileRevision")?)?,
                workspace: id(r, "workspaceId")?,
                launch_path: text(r, "launchPath")?,
            })
        }
        _ => Err(CatalogError::InvalidConfiguration),
    }
}

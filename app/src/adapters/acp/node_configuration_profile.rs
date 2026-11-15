//! Lossless format-one launch-profile codec. It grants no authentication or filesystem authority.
use super::{
    ConfigSourceStamp, ExtensionFields, Known, McpServer, SessionSetup, SetupError, SetupSnapshot,
    Version,
    raw_json::{RawDocument, RawJsonError, RawObject, RawValueRef},
};
use crate::domain::{
    EntityId,
    native_catalog::{CatalogError, LaunchProfileDraft, ProfileKey},
    native_dispatch::ProtocolVersion,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
#[cfg(windows)]
use std::os::windows::ffi::OsStringExt;
use std::{ffi::OsString, sync::Arc};

/// Parses the complete document without normalizing raw ACP fragments or Windows code units.
/// # Errors
/// Rejects duplicate decoded keys, unknown Node fields, invalid canonical encodings and incomplete recipes.
pub fn parse_profile(bytes: Vec<u8>) -> Result<LaunchProfileDraft, CatalogError> {
    let document = RawDocument::parse(bytes).map_err(raw_error)?;
    document.require_unique_fields().map_err(raw_error)?;
    validate_document(document.root())?;
    let mut stored = Vec::new();
    stored
        .try_reserve_exact(document.bytes().len())
        .map_err(|_| CatalogError::Allocation)?;
    stored.extend_from_slice(document.bytes());
    Ok(LaunchProfileDraft::from_bytes(stored))
}
pub(crate) fn raw_error(error: RawJsonError) -> CatalogError {
    if error == RawJsonError::Allocation {
        CatalogError::Allocation
    } else {
        CatalogError::InvalidConfiguration
    }
}
fn setup_error(error: SetupError) -> CatalogError {
    if error == SetupError::Allocation {
        CatalogError::Allocation
    } else {
        CatalogError::InvalidConfiguration
    }
}
pub(crate) fn object<'a>(
    value: RawValueRef<'a>,
    fields: &[&str],
) -> Result<RawObject<'a>, CatalogError> {
    let object = value.object().ok_or(CatalogError::InvalidConfiguration)?;
    if object.len() != fields.len() || object.iter().any(|(key, _)| !fields.contains(&key)) {
        return Err(CatalogError::InvalidConfiguration);
    }
    Ok(object)
}
pub(crate) fn field<'a>(object: RawObject<'a>, key: &str) -> Result<RawValueRef<'a>, CatalogError> {
    object
        .get(key)
        .map_err(raw_error)?
        .ok_or(CatalogError::InvalidConfiguration)
}
pub(crate) fn text(object: RawObject<'_>, key: &str) -> Result<String, CatalogError> {
    field(object, key)?.string().map_err(raw_error)
}
pub(crate) fn id(object: RawObject<'_>, key: &str) -> Result<EntityId, CatalogError> {
    EntityId::decode(&text(object, key)?).map_err(|_| CatalogError::InvalidConfiguration)
}
pub(crate) fn revision(object: RawObject<'_>, key: &str) -> Result<u64, CatalogError> {
    let raw = text(object, key)?;
    let revision = raw
        .parse::<u64>()
        .map_err(|_| CatalogError::InvalidConfiguration)?;
    if revision == 0 || revision.to_string() != raw {
        return Err(CatalogError::InvalidConfiguration);
    }
    Ok(revision)
}
fn reserve<T>(values: &mut Vec<T>, count: usize) -> Result<(), CatalogError> {
    values
        .try_reserve_exact(count)
        .map_err(|_| CatalogError::Allocation)
}
fn units(value: RawValueRef<'_>) -> Result<Vec<u16>, CatalogError> {
    let encoded = value.string().map_err(raw_error)?;
    let bytes = STANDARD
        .decode(&encoded)
        .map_err(|_| CatalogError::InvalidConfiguration)?;
    if bytes.len() % 2 != 0 || STANDARD.encode(&bytes) != encoded {
        return Err(CatalogError::InvalidConfiguration);
    }
    let mut decoded = Vec::new();
    reserve(&mut decoded, bytes.len() / 2)?;
    decoded.extend(
        bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| u16::from_le_bytes([v[0], v[1]])),
    );
    Ok(decoded)
}
struct DecodedLaunch {
    executable: Vec<u16>,
    arguments: Vec<Vec<u16>>,
    environment: Vec<(Vec<u16>, Vec<u16>)>,
}
fn launch(value: RawValueRef<'_>) -> Result<DecodedLaunch, CatalogError> {
    let v = object(
        value,
        &[
            "executable_utf16le_base64",
            "arguments_utf16le_base64",
            "environment",
        ],
    )?;
    let executable = units(field(v, "executable_utf16le_base64")?)?;
    let args = field(v, "arguments_utf16le_base64")?
        .array()
        .ok_or(CatalogError::InvalidConfiguration)?;
    let mut arguments = Vec::new();
    reserve(&mut arguments, args.len())?;
    for arg in args.iter() {
        arguments.push(units(arg)?);
    }
    let env = field(v, "environment")?
        .array()
        .ok_or(CatalogError::InvalidConfiguration)?;
    let mut environment = Vec::new();
    reserve(&mut environment, env.len())?;
    for e in env.iter() {
        let e = object(e, &["name_utf16le_base64", "value_utf16le_base64"])?;
        environment.push((
            units(field(e, "name_utf16le_base64")?)?,
            units(field(e, "value_utf16le_base64")?)?,
        ));
    }
    Ok(DecodedLaunch {
        executable,
        arguments,
        environment,
    })
}
pub(crate) struct AdditionalDirectory {
    pub(crate) workspace: EntityId,
    pub(crate) path: String,
}
pub(crate) struct ProfileRecipe {
    pub(crate) wire: Version,
    pub(crate) mcp: Vec<McpServer>,
    pub(crate) directories: Vec<AdditionalDirectory>,
    pub(crate) extra: ExtensionFields,
}
fn recipes(value: RawValueRef<'_>) -> Result<Vec<ProfileRecipe>, CatalogError> {
    let values = value.array().ok_or(CatalogError::InvalidConfiguration)?;
    let mut result = Vec::new();
    reserve(&mut result, values.len())?;
    for value in values.iter() {
        let value = object(
            value,
            &["wire", "mcp_servers", "additional_directories", "extra"],
        )?;
        let wire = match field(value, "wire")?.unsigned() {
            Some(1) => Version::V1,
            Some(2) => Version::V2,
            _ => return Err(CatalogError::InvalidConfiguration),
        };
        if result.iter().any(|r: &ProfileRecipe| r.wire == wire) {
            return Err(CatalogError::InvalidConfiguration);
        }
        let raw_mcp = field(value, "mcp_servers")?
            .array()
            .ok_or(CatalogError::InvalidConfiguration)?;
        let mut mcp = Vec::new();
        reserve(&mut mcp, raw_mcp.len())?;
        for raw in raw_mcp.iter() {
            mcp.push(McpServer::from_raw(wire, raw.to_owned()).map_err(setup_error)?);
        }
        let raw_dirs = field(value, "additional_directories")?
            .array()
            .ok_or(CatalogError::InvalidConfiguration)?;
        let mut directories = Vec::new();
        reserve(&mut directories, raw_dirs.len())?;
        for raw in raw_dirs.iter() {
            let v = object(raw, &["workspace_id", "path"])?;
            directories.push(AdditionalDirectory {
                workspace: id(v, "workspace_id")?,
                path: text(v, "path")?,
            });
        }
        let extra =
            ExtensionFields::from_raw(field(value, "extra")?.to_owned()).map_err(setup_error)?;
        // The ACP encoder owns the reserved-field rule. This validation does not alter raw fields.
        SessionSetup::try_from_snapshot(SetupSnapshot {
            wire,
            source: ConfigSourceStamp {
                reference: Arc::from("profile-validation"),
                revision: 1,
            },
            cwd: Known::Known(Arc::from("")),
            mcp_servers: Known::Known(mcp.clone()),
            additional_directories: Known::Known(Vec::new()),
            extra: extra.clone(),
        })
        .map_err(setup_error)?;
        result.push(ProfileRecipe {
            wire,
            mcp,
            directories,
            extra,
        });
    }
    Ok(result)
}
fn validate_document(root: RawValueRef<'_>) -> Result<(), CatalogError> {
    let root = object(root, &["format", "launch", "recipes"])?;
    if field(root, "format")?.unsigned() != Some(1) {
        return Err(CatalogError::InvalidConfiguration);
    }
    launch(field(root, "launch")?)?;
    recipes(field(root, "recipes")?)?;
    Ok(())
}
#[cfg(windows)]
pub(crate) struct DecodedProfile {
    pub(crate) executable: OsString,
    pub(crate) arguments: Vec<OsString>,
    pub(crate) environment: Vec<(OsString, OsString)>,
    pub(crate) recipes: Vec<ProfileRecipe>,
}
#[cfg(windows)]
pub(crate) fn decode_profile(bytes: Vec<u8>) -> Result<DecodedProfile, CatalogError> {
    let document = RawDocument::parse(bytes).map_err(raw_error)?;
    document.require_unique_fields().map_err(raw_error)?;
    let root = object(document.root(), &["format", "launch", "recipes"])?;
    if field(root, "format")?.unsigned() != Some(1) {
        return Err(CatalogError::InvalidConfiguration);
    }
    let decoded = launch(field(root, "launch")?)?;
    let mut arguments = Vec::new();
    reserve(&mut arguments, decoded.arguments.len())?;
    arguments.extend(decoded.arguments.iter().map(|v| OsString::from_wide(v)));
    let mut environment = Vec::new();
    reserve(&mut environment, decoded.environment.len())?;
    environment.extend(
        decoded
            .environment
            .iter()
            .map(|(n, v)| (OsString::from_wide(n), OsString::from_wide(v))),
    );
    Ok(DecodedProfile {
        executable: OsString::from_wide(&decoded.executable),
        arguments,
        environment,
        recipes: recipes(field(root, "recipes")?)?,
    })
}
#[cfg(windows)]
pub(crate) fn setup(profile: ProfileKey, cwd: &str, recipe: ProfileRecipe) -> SetupSnapshot {
    SetupSnapshot {
        wire: recipe.wire,
        source: ConfigSourceStamp {
            reference: Arc::from(profile.id().encode()),
            revision: profile.revision(),
        },
        cwd: Known::Known(Arc::from(cwd)),
        mcp_servers: Known::Known(recipe.mcp),
        additional_directories: Known::Known(
            recipe
                .directories
                .into_iter()
                .map(|d| Arc::from(d.path))
                .collect(),
        ),
        extra: recipe.extra,
    }
}
#[cfg(windows)]
pub(crate) const fn protocol(wire: Version) -> ProtocolVersion {
    match wire {
        Version::V1 => ProtocolVersion::V1,
        Version::V2 => ProtocolVersion::V2,
    }
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod profile_independent;

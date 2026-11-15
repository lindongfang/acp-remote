//! Prepared independent fixtures. Not compiled against an unfrozen implementation.
pub use acp_remote::domain::{
    EntityId,
    native::{HarnessConnectionEpoch, LoadEpoch, NativeSessionKey, ResourceEpoch, RuntimeEpoch},
    views::{
        KeyNamespace, SourceKey, ViewError, ViewFailure, ViewHandle, ViewLimits, ViewState,
        ViewWindows, WindowReadStamp,
    },
};

pub fn target(source: u8, name: &str) -> NativeSessionKey {
    NativeSessionKey::from_config(Some(EntityId::from_bytes([source; 16])), name).unwrap()
}
pub const fn epoch(load: u8) -> ResourceEpoch {
    ResourceEpoch {
        runtime: RuntimeEpoch(EntityId::from_bytes([1; 16])),
        connection: HarnessConnectionEpoch(EntityId::from_bytes([2; 16])),
        load: LoadEpoch(EntityId::from_bytes([load; 16])),
    }
}
pub fn fixture(limits: ViewLimits) -> (ViewWindows, ViewHandle) {
    let mut windows = ViewWindows::new(limits).unwrap();
    let handle = windows.begin(target(9, "native-A"), epoch(3)).unwrap();
    (windows, handle)
}
pub const fn key(id: &str) -> SourceKey<'_> {
    SourceKey {
        namespace: KeyNamespace::Tool,
        id,
    }
}
pub fn stamp<'h>(windows: &ViewWindows, handle: &'h ViewHandle) -> WindowReadStamp<'h> {
    windows.with_window(handle, |guard| guard.stamp()).unwrap()
}
pub fn revision(windows: &ViewWindows, handle: &ViewHandle) -> u64 {
    windows
        .with_window(handle, |guard| guard.stamp().revision())
        .unwrap()
}

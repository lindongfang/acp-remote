use acp_remote::domain::{EntityId, native::*, views::*};
pub fn target(n: u8) -> NativeSessionKey {
    NativeSessionKey::from_config(Some(EntityId::from_bytes([n; 16])), "n").unwrap()
}
pub const fn epoch(n: u8) -> ResourceEpoch {
    ResourceEpoch {
        runtime: RuntimeEpoch(EntityId::from_bytes([n; 16])),
        connection: HarnessConnectionEpoch(EntityId::from_bytes([n; 16])),
        load: LoadEpoch(EntityId::from_bytes([n; 16])),
    }
}
pub const fn key(n: &str) -> SourceKey<'_> {
    SourceKey {
        namespace: KeyNamespace::Tool,
        id: n,
    }
}
pub fn views(bytes: usize, items: usize) -> ViewWindows {
    ViewWindows::new(ViewLimits {
        target_bytes: bytes,
        target_items: items,
        ..ViewLimits::default()
    })
    .unwrap()
}
pub fn write(stage: &mut StagedItem, data: &[u8]) {
    stage.parts().writer.write_all(data).unwrap();
}
pub fn read(views: &mut ViewWindows, h: &ViewHandle, keyname: &str) -> Vec<u8> {
    views
        .item_lease(h, key(keyname))
        .unwrap()
        .item()
        .bytes
        .to_vec()
}
pub fn assert_usage(views: &ViewWindows, t: &NativeSessionKey, bytes: usize, items: usize) {
    assert_eq!(views.target_usage(t), Some(ViewUsage { bytes, items }));
    assert!(views.usage().bytes >= 8192 + bytes);
}

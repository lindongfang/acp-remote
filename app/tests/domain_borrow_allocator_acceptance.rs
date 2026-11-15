//! Separate allocator binary: fixture/callback allocations are not attributed to Domain.
#[path = "acceptance/domain_borrow_support.rs"]
mod support;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    ptr,
};
use support::*;

struct Counter;
thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static CALLS: Cell<usize> = const { Cell::new(0) };
    static FAIL_SIZE: Cell<usize> = const { Cell::new(0) };
    static FAIL_HITS: Cell<usize> = const { Cell::new(0) };
}
fn allocation(size: usize) -> bool {
    if ACTIVE.try_with(Cell::get).unwrap_or(false) {
        let _ = CALLS.try_with(|v| v.set(v.get() + 1));
    }
    FAIL_SIZE
        .try_with(|v| {
            if v.get() != 0 && v.get() == size {
                v.set(0);
                FAIL_HITS.with(|n| n.set(n.get() + 1));
                true
            } else {
                false
            }
        })
        .unwrap_or(false)
}
// SAFETY: successful requests delegate unchanged to System; selected allocation failure is null.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if allocation(layout.size()) {
            ptr::null_mut()
        } else {
            unsafe { System.alloc(layout) }
        }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if allocation(layout.size()) {
            ptr::null_mut()
        } else {
            unsafe { System.alloc_zeroed(layout) }
        }
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, new: usize) -> *mut u8 {
        if allocation(new) {
            ptr::null_mut()
        } else {
            unsafe { System.realloc(p, layout, new) }
        }
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;
struct StopTracking;
impl Drop for StopTracking {
    fn drop(&mut self) {
        ACTIVE.with(|v| v.set(false));
        FAIL_SIZE.with(|v| v.set(0));
    }
}
fn counted<R>(f: impl FnOnce() -> R) -> (R, usize) {
    CALLS.with(|v| v.set(0));
    ACTIVE.with(|v| v.set(true));
    let reset = StopTracking;
    let result = f();
    ACTIVE.with(|v| v.set(false));
    let calls = CALLS.with(Cell::get);
    drop(reset);
    (result, calls)
}

#[test]
fn dbr01_first_and_repeated_borrows_empty_and_full_target_allocate_nothing() {
    for items in [0, 4096] {
        let (mut windows, handle) = fixture(ViewLimits::default());
        for n in 1_u64..=items {
            windows.insert(&handle, None, &n.to_le_bytes()).unwrap();
        }
        let before = windows.usage();
        for _ in 0..2 {
            let (result, allocations) = counted(|| {
                windows.with_window(&handle, |guard| {
                    let mut count = 0;
                    let mut total = 0_u64;
                    for item in guard.items() {
                        count += 1;
                        total += item.position;
                    }
                    (count, total, guard.stamp())
                })
            });
            let (count, total, stamp) = result.unwrap();
            assert_eq!(count, usize::try_from(items).unwrap());
            assert_eq!(total, items * (items + 1) / 2);
            assert_eq!(allocations, 0);
            windows.validate_read_stamp(&handle, &stamp).unwrap();
            assert_eq!(windows.usage(), before);
        }
        if items == 4096 {
            assert!(matches!(
                windows.snapshot(&handle),
                Err(ViewError::Capacity)
            ));
        }
    }
}

#[test]
fn dbr01_node_exactly_32768_objects_can_be_borrowed_without_new_references() {
    let mut windows = ViewWindows::new(ViewLimits::default()).unwrap();
    let mut handles = Vec::new();
    for n in 0..8 {
        let handle = windows
            .begin(target(20 + n, "full-native"), epoch(3))
            .unwrap();
        for marker in 1_u64..=4096 {
            windows
                .insert(&handle, None, &marker.to_le_bytes())
                .unwrap();
        }
        handles.push(handle);
    }
    assert_eq!(windows.usage().items, 32768);
    let before = windows.usage();
    let (total, allocations) = counted(|| {
        let mut count = 0;
        for handle in &handles {
            count += windows
                .with_window(handle, |guard| {
                    let mut seen = 0;
                    for item in guard.items() {
                        assert_eq!(item.bytes.len(), 8);
                        seen += 1;
                    }
                    seen
                })
                .unwrap();
        }
        count
    });
    assert_eq!(total, 32768);
    assert_eq!(allocations, 0);
    assert_eq!(windows.usage(), before);
}

#[test]
fn dbr05_real_payload_null_after_fifo_preserves_watermark_and_exact_revision() {
    let limits = ViewLimits {
        target_bytes: 150_000,
        node_bytes: 300_000,
        target_items: 16,
        node_items: 32,
    };
    let (mut windows, handle) = fixture(limits);
    windows
        .insert(&handle, Some(key("old")), &vec![7; 100_000])
        .unwrap();
    let before = stamp(&windows, &handle);
    let work = windows.stage_workspace(&handle, 0).unwrap();
    FAIL_HITS.with(|v| v.set(0));
    FAIL_SIZE.with(|v| v.set(73_123));
    let reset = StopTracking;
    let result = windows.stage_insert(&handle, work, None, 73_123);
    FAIL_SIZE.with(|v| v.set(0));
    drop(reset);
    assert!(matches!(result, Err(ViewError::Capacity)));
    assert_eq!(FAIL_HITS.with(Cell::get), 1);
    let status = windows.window_status(&handle).unwrap();
    assert_eq!(status.evicted_count, 1);
    assert_eq!(status.evicted_through, Some(1));
    assert_eq!(status.retained_items, 0);
    assert_eq!(status.state, ViewState::Incomplete(ViewFailure::Capacity));
    assert_eq!(revision(&windows, &handle), before.revision() + 2);
    assert_eq!(
        windows.validate_read_stamp(&handle, &before),
        Err(ViewError::ReadChanged)
    );
    windows.insert(&handle, None, b"later").unwrap();
    assert_eq!(revision(&windows, &handle), before.revision() + 3);
}

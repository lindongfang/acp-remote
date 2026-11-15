//! Separate test executable: its scoped allocator never changes product code or other test binaries.
#[path = "acceptance/view_budget_support.rs"]
mod support;
use acp_remote::domain::views::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
use support::*;

#[derive(Clone, Copy)]
struct Event {
    kind: u8,
    size: usize,
    align: usize,
    ptr: usize,
}
const EMPTY: Event = Event {
    kind: 0,
    size: 0,
    align: 0,
    ptr: 0,
};
#[derive(Clone, Copy)]
struct Trace {
    active: bool,
    fail_size: usize,
    hits: usize,
    events: [Event; 128],
    used: usize,
    overflow: bool,
}
impl Trace {
    const fn empty() -> Self {
        Self {
            active: false,
            fail_size: 0,
            hits: 0,
            events: [EMPTY; 128],
            used: 0,
            overflow: false,
        }
    }
    fn allocations(&self) -> impl Iterator<Item = &Event> {
        self.events[..self.used].iter().filter(|e| e.kind == 1)
    }
}
thread_local! {static TRACE:Cell<Trace>=const {Cell::new(Trace::empty())};}
struct ScopedAllocator;
#[global_allocator]
static ALLOCATOR: ScopedAllocator = ScopedAllocator;
fn should_fail(layout: Layout) -> bool {
    TRACE
        .try_with(|cell| {
            let mut t = cell.get();
            if t.active
                && t.fail_size > 0
                && t.hits == 0
                && layout.size() == t.fail_size
                && layout.align() == 1
            {
                t.hits += 1;
                cell.set(t);
                true
            } else {
                false
            }
        })
        .unwrap_or(false)
}
fn event(kind: u8, layout: Layout, pointer: *mut u8) {
    let _ = TRACE.try_with(|cell| {
        let mut t = cell.get();
        if t.active {
            if t.used < t.events.len() {
                t.events[t.used] = Event {
                    kind,
                    size: layout.size(),
                    align: layout.align(),
                    ptr: pointer as usize,
                };
                t.used += 1;
            } else {
                t.overflow = true;
            }
            cell.set(t);
        }
    });
}
unsafe impl GlobalAlloc for ScopedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if should_fail(layout) {
            event(4, layout, std::ptr::null_mut());
            return std::ptr::null_mut();
        }
        let p = unsafe { System.alloc(layout) };
        event(1, layout, p);
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if should_fail(layout) {
            event(4, layout, std::ptr::null_mut());
            return std::ptr::null_mut();
        }
        let p = unsafe { System.alloc_zeroed(layout) };
        event(1, layout, p);
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        event(2, layout, p);
        unsafe { System.dealloc(p, layout) }
    }
    unsafe fn realloc(&self, p: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        let new_layout = Layout::from_size_align(new_size, old.align()).unwrap();
        if should_fail(new_layout) {
            event(4, new_layout, std::ptr::null_mut());
            return std::ptr::null_mut();
        }
        let next = unsafe { System.realloc(p, old, new_size) };
        event(3, new_layout, next);
        next
    }
}
struct Guard(bool);
impl Guard {
    fn start(fail_size: usize) -> Self {
        TRACE.with(|c| {
            assert!(!c.get().active);
            c.set(Trace {
                active: true,
                fail_size,
                ..Trace::empty()
            });
        });
        Self(true)
    }
    fn finish(mut self) -> Trace {
        let result = TRACE.with(|c| {
            let t = c.get();
            c.set(Trace::empty());
            t
        });
        self.0 = false;
        assert!(!result.overflow);
        result
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        if self.0 {
            let _ = TRACE.try_with(|c| c.set(Trace::empty()));
        }
    }
}

#[test]
fn bp_i_actual_public_layout_bulk_capacities_and_pointer_move_publish() {
    use std::mem::size_of;
    assert!(size_of::<StageWorkspace>() <= 512);
    assert!(size_of::<StagedItem>() + size_of::<StageParts<'_>>() <= 512);
    assert!(size_of::<ItemLease>() <= 256);
    println!(
        "independent public layout: workspace={} staged={} lease={} parts={} window_status={}",
        size_of::<StageWorkspace>(),
        size_of::<StagedItem>(),
        size_of::<ItemLease>(),
        size_of::<StageParts<'_>>(),
        size_of::<WindowStatus>()
    );
    let guard = Guard::start(0);
    let mut v = ViewWindows::new(ViewLimits::default()).unwrap();
    let roots = guard.finish();
    assert!(roots.allocations().map(|e| e.size).sum::<usize>() <= 8192);
    let t = target(1);
    let guard = Guard::start(0);
    let h = v.begin(t.clone(), epoch(1)).unwrap();
    let metadata = guard.finish();
    assert!(metadata.allocations().map(|e| e.size).sum::<usize>() <= 2048);
    println!(
        "independent root allocation requests={:?}; identity/window requests={:?}",
        roots.allocations().map(|e| e.size).collect::<Vec<_>>(),
        metadata.allocations().map(|e| e.size).collect::<Vec<_>>()
    );
    let keytext = "k".repeat(2099);
    let payload = vec![0xa5; 12347];
    let guard = Guard::start(0);
    let work = v.stage_workspace(&h, 3079).unwrap();
    let mut stage = v
        .stage_insert(&h, work, Some(key(&keytext)), payload.len())
        .unwrap();
    assert_usage(&v, &t, 2049 + 512 + 3079 + 512 + 2099 + 12347, 1);
    write(&mut stage, &payload);
    v.publish(&h, stage).unwrap();
    let lease = v.item_lease(&h, key(&keytext)).unwrap();
    let bytes_ptr = lease.item().bytes.as_ptr() as usize;
    let key_ptr = lease.item().key.unwrap().id.as_ptr() as usize;
    let trace = guard.finish();
    for n in [3079, 2099, 12347] {
        let events = trace
            .allocations()
            .filter(|e| e.size == n && e.align == 1)
            .collect::<Vec<_>>();
        assert_eq!(
            events.len(),
            1,
            "bulk size {n} should be allocated once at exact capacity"
        );
        if n == 12347 {
            assert_eq!(events[0].ptr, bytes_ptr);
        }
        if n == 2099 {
            assert_eq!(events[0].ptr, key_ptr);
        }
    }
    assert!(!trace.events[..trace.used].iter().any(|e| e.kind == 3));
    assert_eq!(lease.item().bytes, payload);
    assert_eq!(
        v.target_usage(&t).unwrap().bytes,
        2049 + 512 + 2099 + 12347 + 320
    );
    println!(
        "bulk exact Layout and unchanged published pointers: scratch3079/key2099/payload12347; realloc=0"
    );
}

#[test]
fn bp_g_window_status_allocates_nothing_even_when_snapshot_budget_is_full() {
    let mut v = views(2049 + 512 + 1, 100);
    let h = v.begin(target(1), epoch(1)).unwrap();
    let work = v.stage_workspace(&h, 1).unwrap();
    assert!(matches!(v.snapshot(&h), Err(ViewError::Capacity)));
    let before = v.usage();
    let guard = Guard::start(0);
    for _ in 0..10000 {
        assert_eq!(v.window_status(&h).unwrap().retained_items, 0);
    }
    let trace = guard.finish();
    assert_eq!(trace.used, 0);
    assert_eq!(v.usage(), before);
    drop(work);
}

#[test]
fn bp_b_scratch_real_null_returns_capacity_incomplete_and_refunds_every_charge() {
    let mut v = views(100_000, 100);
    let h = v.begin(target(1), epoch(1)).unwrap();
    let baseline = v.usage();
    let guard = Guard::start(10007);
    let result = v.stage_workspace(&h, 10007);
    let trace = guard.finish();
    assert_eq!(trace.hits, 1);
    assert!(matches!(result, Err(ViewError::Capacity)));
    assert_eq!(v.usage(), baseline);
    assert_eq!(
        v.window_status(&h).unwrap().state,
        ViewState::Incomplete(ViewFailure::Capacity)
    );
    println!("actual allocator null hit: scratch Layout(size=10007,align=1)");
}

#[test]
fn bp_c_payload_and_key_real_null_free_partial_allocations_and_scratch() {
    for fail in [12347, 3071] {
        let mut v = views(100_000, 100);
        let h = v.begin(target(1), epoch(1)).unwrap();
        let k = "k".repeat(3071);
        let baseline = v.usage();
        let w = v.stage_workspace(&h, 0).unwrap();
        let guard = Guard::start(fail);
        let result = v.stage_insert(&h, w, Some(key(&k)), 12347);
        let trace = guard.finish();
        assert_eq!(trace.hits, 1);
        assert!(matches!(result, Err(ViewError::Capacity)));
        assert_eq!(v.usage(), baseline);
        assert_eq!(v.window_status(&h).unwrap().retained_items, 0);
        assert_eq!(
            v.window_status(&h).unwrap().state,
            ViewState::Incomplete(ViewFailure::Capacity)
        );
        if fail == 3071 {
            let payload = trace.allocations().find(|e| e.size == 12347).unwrap();
            assert!(
                trace.events[..trace.used]
                    .iter()
                    .any(|e| e.kind == 2 && e.ptr == payload.ptr)
            );
        }
        println!("actual allocator null hit: insert bulk Layout(size={fail},align=1)");
    }
}

#[test]
fn bp_d_replace_payload_and_key_real_null_preserve_old_item_no_eviction() {
    for fail in [12347, 3071] {
        let mut v = views(100_000, 100);
        let h = v.begin(target(1), epoch(1)).unwrap();
        let key_text = "k".repeat(3071);
        v.insert(&h, Some(key(&key_text)), b"old").unwrap();
        let baseline = v.usage();
        let l = v.item_lease(&h, key(&key_text)).unwrap();
        let w = v.stage_workspace(&h, 0).unwrap();
        let guard = Guard::start(fail);
        let result = v.stage_replace(&h, w, l, 12347);
        let trace = guard.finish();
        assert_eq!(trace.hits, 1);
        assert!(matches!(result, Err(ViewError::Capacity)));
        assert_eq!(v.usage(), baseline);
        assert_eq!(read(&mut v, &h, &key_text), b"old");
        assert_eq!(v.window_status(&h).unwrap().evicted_count, 0);
        assert_eq!(
            v.window_status(&h).unwrap().state,
            ViewState::Incomplete(ViewFailure::Capacity)
        );
        println!("actual allocator null hit: replace bulk Layout(size={fail},align=1)");
    }
}

#[test]
fn bp_g_payload_null_after_fifo_keeps_real_watermarks_and_later_insert_continues() {
    let mut v = views(2049 + 512 + 512 + 12347, 100);
    let h = v.begin(target(1), epoch(1)).unwrap();
    let old = [0u8; 4488];
    v.insert(&h, None, &old).unwrap();
    v.insert(&h, None, &old).unwrap();
    let w = v.stage_workspace(&h, 0).unwrap();
    let guard = Guard::start(12347);
    let result = v.stage_insert(&h, w, None, 12347);
    let trace = guard.finish();
    assert_eq!(trace.hits, 1);
    assert!(matches!(result, Err(ViewError::Capacity)));
    let s = v.window_status(&h).unwrap();
    assert_eq!(s.evicted_count, 2);
    assert_eq!(s.evicted_through, Some(2));
    assert_eq!(s.retained_items, 0);
    assert_eq!(s.next_position, Some(3));
    assert_eq!(s.state, ViewState::Incomplete(ViewFailure::Capacity));
    assert_eq!(
        v.target_usage(&target(1)).unwrap(),
        ViewUsage {
            bytes: 2049,
            items: 0
        }
    );
    let w = v.stage_workspace(&h, 0).unwrap();
    let mut stage = v.stage_insert(&h, w, None, 1).unwrap();
    write(&mut stage, b"x");
    assert_eq!(v.publish(&h, stage).unwrap().position, 3);
    assert_eq!(v.window_status(&h).unwrap().evicted_count, 2);
    println!("actual allocator null after FIFO2: status retained eviction2, next insert position3");
}

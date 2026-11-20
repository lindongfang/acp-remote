use super::cursor::{CursorScope, Generation, fingerprint};
use super::*;
use crate::identity::NodeIdentity;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

fn user(id: Option<&str>) -> FrameClass {
    FrameClass::User(id.map(str::to_owned))
}

fn scan(frames: impl IntoIterator<Item = FrameClass>) -> (TurnIndexer, Vec<usize>) {
    let mut index = TurnIndexer::default();
    let starts = frames
        .into_iter()
        .enumerate()
        .filter_map(|(i, f)| index.observe(f).map(|_| i))
        .collect();
    (index, starts)
}

#[test]
fn history_page_turn_evidence_is_generation_wide() {
    use FrameClass::{Assistant, Notification};
    let (index, starts) = scan([
        Notification,
        Notification,
        user(Some("a")),
        Notification,
        user(Some("a")),
        Notification,
        user(Some("a")),
        user(Some("b")),
    ]);
    assert_eq!(starts, [2, 7]);
    assert_eq!(index.turn_count(), 2);
    for (left, right) in [(None, None), (Some("x"), None), (None, Some("x"))] {
        for separator in [vec![], vec![Notification]] {
            let frames = std::iter::once(user(left)).chain(separator).chain([
                user(right),
                Assistant,
                user(Some("z")),
            ]);
            let (index, _) = scan(frames);
            assert!(!index.provable());
            assert_eq!(index.turn_count(), 0);
        }
    }
    assert_eq!(scan([user(None), Assistant, user(None)]).1, [0, 2]);
    assert!(!scan([]).0.provable());
    assert!(!scan([Notification]).0.provable());
    assert!(
        !scan([user(Some("a")), user(Some("b")), Notification, user(None)])
            .0
            .provable()
    );
}

#[test]
fn history_page_incremental_and_batch_keep_the_same_boundaries() {
    let mut seed = 17_u32;
    let mut expected = Vec::new();
    let frames: Vec<_> = (0..10_000)
        .map(|i| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            match seed % 7 {
                0 => {
                    expected.push(i);
                    user(Some(&i.to_string()))
                }
                1 | 2 => FrameClass::Assistant,
                _ => FrameClass::Notification,
            }
        })
        .collect();
    let (full, starts) = scan(frames.clone());
    assert_eq!(starts, expected);
    let mut streaming = TurnIndexer::default();
    let mut streamed = Vec::new();
    for (batch, chunk) in frames.chunks(97).enumerate() {
        for (i, f) in chunk.iter().enumerate() {
            if streaming.observe(f.clone()).is_some() {
                streamed.push(batch * 97 + i);
            }
        }
    }
    assert_eq!(streamed, starts);
    assert_eq!(streaming.turn_count(), full.turn_count());
}

#[test]
fn history_page_sizes_require_canonical_bounded_decimals() {
    assert_eq!(PageSize::parse(None).unwrap().get(), 10);
    for ok in [1, 3, 5, 10, 50] {
        assert_eq!(PageSize::parse(Some(&ok.to_string())).unwrap().get(), ok);
    }
    for bad in [
        "0",
        "-1",
        "1.5",
        "1e1",
        "010",
        "51",
        "18446744073709551616",
        " 5",
        "5 ",
        "５",
        "",
    ] {
        assert_eq!(PageSize::parse(Some(bad)), Err(PageError::InvalidRequest));
    }
}

#[test]
fn history_page_bidirectional_ranges() {
    for (total, n, child) in [
        (25, 10, false),
        (25, 10, true),
        (25, 5, false),
        (25, 3, true),
        (1000, 10, false),
        (1000, 10, true),
    ] {
        let size = PageSize(n);
        let mut range = TurnRange::first(total, size, child).unwrap();
        let expected_first = if child {
            1
        } else {
            total.saturating_sub(u64::from(n) - 1).max(1)
        };
        assert_eq!(range.first, expected_first);
        assert_eq!(
            range.last,
            if child {
                u64::from(n).min(total)
            } else {
                total
            }
        );
        let mut seen = Vec::new();
        let mut pages = Vec::new();
        loop {
            assert_eq!(range.includes_prelude(true), range.first == 1);
            assert!(!range.includes_prelude(false));
            assert_eq!(range.older_anchor().is_none(), range.first == 1);
            assert_eq!(range.newer_anchor(total).is_none(), range.last == total);
            seen.extend(range.first..=range.last);
            pages.push(range);
            let next = if child {
                range.newer_anchor(total)
            } else {
                range.older_anchor()
            };
            let Some(next) = next else {
                break;
            };
            range = TurnRange::locate(
                total,
                size,
                if child {
                    Direction::Newer
                } else {
                    Direction::Older
                },
                next,
            )
            .unwrap();
        }
        seen.sort_unstable();
        assert_eq!(seen, (1..=total).collect::<Vec<_>>());
        for expected in pages.iter().rev().skip(1) {
            let (direction, anchor) = if child {
                (Direction::Older, range.older_anchor())
            } else {
                (Direction::Newer, range.newer_anchor(total))
            };
            range = TurnRange::locate(total, size, direction, anchor.unwrap()).unwrap();
            assert_eq!(&range, expected);
        }
    }
    for (direction, anchor, first, last) in [
        (Direction::Older, 500, 491, 500),
        (Direction::Older, 5, 1, 5),
        (Direction::Older, 1, 1, 1),
        (Direction::Newer, 500, 500, 509),
        (Direction::Newer, 995, 995, 1000),
        (Direction::Newer, 1000, 1000, 1000),
    ] {
        assert_eq!(
            TurnRange::locate(1000, PageSize(10), direction, anchor),
            Ok(TurnRange { first, last })
        );
    }
    assert_eq!(
        TurnRange::locate(1000, PageSize(5), Direction::Newer, 1001),
        Err(PageError::NotFound)
    );
}

#[test]
fn history_page_scope_fingerprint_uses_byte_lengths_and_a_child_presence_bit() {
    for (child, expected) in [
        (None, "3a67d99b7edb20f0f7ede5f16a24d90a"),
        (Some("子"), "5aee3671023bfb7a9a7acf98d10db22a"),
    ] {
        assert_eq!(
            hex::encode(fingerprint(
                "source",
                &[b"source", "会话".as_bytes()],
                child
            )),
            expected
        );
    }
}

#[test]
fn history_page_cursor_authentication_precedes_scope_and_generation() {
    let identity = NodeIdentity::for_test([6; 32]);
    let target = fingerprint("source", &[b"source", b"ab"], Some("c"));
    let scope = CursorScope {
        target,
        direction: Direction::Older,
        size: PageSize(3),
    };
    let generation = Generation::Cache([33; 16]);
    let stale = Generation::Live([33; 16]);
    let wire = scope.issue(generation, 19, |m| identity.sign(m));
    assert_eq!(
        scope.verify(
            generation,
            &wire,
            NodeIdentity::for_test([6; 32]).public_key()
        ),
        Ok(19)
    );
    assert_eq!(
        scope.verify(stale, &wire, identity.public_key()),
        Err(PageError::StaleCursor)
    );
    for other in [
        CursorScope {
            direction: Direction::Newer,
            ..scope
        },
        CursorScope {
            size: PageSize(2),
            ..scope
        },
        CursorScope {
            target: fingerprint("source", &[b"source", b"a"], Some("bc")),
            ..scope
        },
        CursorScope {
            target: fingerprint("source", &[b"source", b"ab"], None),
            ..scope
        },
        CursorScope {
            target: fingerprint("target", &[b"source", b"ab"], Some("c")),
            ..scope
        },
    ] {
        assert_eq!(
            other.verify(stale, &wire, identity.public_key()),
            Err(PageError::InvalidCursor)
        );
    }
    let bytes = URL_SAFE_NO_PAD.decode(&wire).unwrap();
    for pos in [0, 1, 2, 18, 19, 35, 43, 44, 107] {
        let mut bad = bytes.clone();
        bad[pos] ^= 1;
        assert_eq!(
            scope.verify(stale, &URL_SAFE_NO_PAD.encode(bad), identity.public_key()),
            Err(PageError::InvalidCursor)
        );
    }
    for bad in [&wire[..wire.len() - 1], &format!("{wire}="), "*"] {
        assert_eq!(
            scope.verify(generation, bad, identity.public_key()),
            Err(PageError::InvalidCursor)
        );
    }
    for (pos, value) in [(0, 2), (1, 2), (18, 0), (43, 0), (43, 51)] {
        let bad = scope.issue(generation, 19, |m| {
            let mut modified = m.to_vec();
            let start = m.len() - 44;
            modified[start + pos] = value;
            identity.sign(&modified)
        });
        let mut bad = URL_SAFE_NO_PAD.decode(bad).unwrap();
        bad[pos] = value;
        assert_eq!(
            scope.verify(stale, &URL_SAFE_NO_PAD.encode(bad), identity.public_key()),
            Err(PageError::InvalidCursor)
        );
    }
    let wrong_domain = scope.issue(generation, 19, |m| identity.sign(&m[m.len() - 44..]));
    assert_eq!(
        scope.verify(generation, &wrong_domain, identity.public_key()),
        Err(PageError::InvalidCursor)
    );
    assert_eq!(
        scope.verify(
            generation,
            &wire,
            NodeIdentity::for_test([7; 32]).public_key()
        ),
        Err(PageError::InvalidCursor)
    );
}

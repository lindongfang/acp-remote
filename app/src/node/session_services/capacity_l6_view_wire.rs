// L6 observers preserve native record boundaries before parsing bodies.
// This code is only an acceptance client, never a product wire implementation.
struct CapacityL6RecordAssembly {
    stream: Id128,
    last_packet: u64,
    pending: Option<(u64, RecordKind, u64, Vec<u8>)>,
}

impl CapacityL6RecordAssembly {
    fn new(stream: Id128) -> Self {
        Self {
            stream,
            last_packet: 0,
            pending: None,
        }
    }

    fn receive(
        &mut self,
        header: TransferHeader,
        bytes: &[u8],
    ) -> CapacityL6Result<Option<(RecordKind, Vec<u8>)>> {
        if header.stream() != self.stream
            || self.last_packet.checked_add(1) != Some(header.packet())
        {
            return Err("view_wrong_stream_or_repeated_packet");
        }
        let kind = header
            .record_kind()
            .ok_or("view_unexpected_control_frame")?;
        if bytes.len() != header.payload_length() as usize {
            return Err("view_payload_length_mismatch");
        }
        // Original-read completion can supersede an in-flight record between
        // packet credits. Only the original stream's complete rebound may do so.
        if kind == RecordKind::Status && header.offset() == 0 && header.is_last() {
            let status: serde_json::Value =
                serde_json::from_slice(bytes).map_err(|_| "view_status_record_invalid")?;
            if status["type"] == "stream_rebound" {
                if header.record() != 1
                    || header.total() != bytes.len() as u64
                    || status["previousStreamId"] != self.stream.to_base64url()
                {
                    return Err("view_rebound_original_stream_mismatch");
                }
                self.pending = None;
                self.last_packet = header.packet();
                return Ok(Some((kind, bytes.to_vec())));
            }
        }
        if self.pending.is_none() {
            if header.offset() != 0 {
                return Err("view_record_started_with_missing_bytes");
            }
            self.pending = Some((header.record(), kind, header.total(), Vec::new()));
        }
        let (record, current_kind, total, data) =
            self.pending.as_mut().ok_or("view_record_state_missing")?;
        if *record != header.record()
            || *current_kind != kind
            || *total != header.total()
            || header.offset() != data.len() as u64
        {
            return Err("view_record_fragment_identity_or_offset_changed");
        }
        data.try_reserve(bytes.len())
            .map_err(|_| "view_record_allocation_failed")?;
        data.extend_from_slice(bytes);
        self.last_packet = header.packet();
        if header.is_last() {
            if data.len() as u64 != *total {
                return Err("view_record_total_not_received");
            }
            let (_, kind, _, data) = self.pending.take().ok_or("view_record_state_missing")?;
            Ok(Some((kind, data)))
        } else {
            Ok(None)
        }
    }
}

async fn capacity_l6_drain_snapshot(
    socket: &mut ManagementSocket,
    stream: Id128,
) -> CapacityL6Result<(Vec<(RecordKind, Vec<u8>)>, bool)> {
    capacity_l6_drain_view(socket, stream).await
}

async fn capacity_l6_drain_original_complete(
    socket: &mut ManagementSocket,
    stream: Id128,
) -> CapacityL6Result<Vec<(RecordKind, Vec<u8>)>> {
    let (records, rebound) = capacity_l6_drain_view(socket, stream).await?;
    let has_complete = rebound
        || records
            .iter()
            .any(|(kind, _)| *kind == RecordKind::SnapshotEnd);
    if !has_complete {
        return Err("view_original_read_not_complete");
    }
    Ok(records)
}

async fn capacity_l6_drain_view(
    socket: &mut ManagementSocket,
    stream: Id128,
) -> CapacityL6Result<(Vec<(RecordKind, Vec<u8>)>, bool)> {
    let credit = TransferHeader::credit(stream, 0)
        .encode(&[])
        .map_err(|_| "view_initial_credit_invalid")?;
    socket
        .send(Message::Binary(credit.into()))
        .await
        .map_err(|_| "view_initial_credit_failed")?;
    let packets = std::sync::atomic::AtomicU64::new(0);
    let last_raw_kind = std::sync::atomic::AtomicU8::new(0);
    let records_count = std::sync::atomic::AtomicUsize::new(0);
    let timed = timeout(Duration::from_secs(90), async {
        let mut assembly = CapacityL6RecordAssembly::new(stream);
        let mut records = Vec::new();
        loop {
            match socket.next().await {
                Some(Ok(Message::Binary(bytes))) => {
                    let (header, body) = TransferHeader::decode(&bytes)
                        .map_err(|_| "view_received_invalid_header")?;
                    packets.store(header.packet(), std::sync::atomic::Ordering::Relaxed);
                    if let Some(k) = header.record_kind() {
                        last_raw_kind.store(match k {
                            RecordKind::OriginalRaw => 1,
                            RecordKind::Status => 2,
                            RecordKind::ItemProjection => 3,
                            RecordKind::SnapshotEnd => 4,
                        }, std::sync::atomic::Ordering::Relaxed);
                    }
                    let record = assembly.receive(header, body)?;
                    let credit = TransferHeader::credit(stream, header.packet())
                        .encode(&[])
                        .map_err(|_| "view_chunk_credit_invalid")?;
                    socket
                        .send(Message::Binary(credit.into()))
                        .await
                        .map_err(|_| "view_chunk_credit_failed")?;
                    if let Some((kind, body)) = record {
                        let rebound = if kind == RecordKind::Status {
                            let status: serde_json::Value = serde_json::from_slice(&body)
                                .map_err(|_| "view_status_record_invalid")?;
                            let rebound = status["type"] == "stream_rebound";
                            if rebound && status["previousStreamId"] != stream.to_base64url() {
                                return Err("view_rebound_original_stream_mismatch");
                            }
                            rebound
                        } else {
                            false
                        };
                        records.push((kind, body));
                        records_count.store(records.len(), std::sync::atomic::Ordering::Relaxed);
                        if rebound || kind == RecordKind::SnapshotEnd {
                            return Ok((records, rebound));
                        }
                    }
                }
                Some(Ok(Message::Text(text))) => {
                    let event: serde_json::Value = serde_json::from_str(text.as_str())
                        .map_err(|_| "view_invalid_management_event")?;
                    if event.get("requestId").is_some() {
                        return Err("view_unexpected_management_response");
                    }
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                _ => return Err("view_socket_closed_before_snapshot"),
            }
        }
    })
    .await;
    match timed {
        Ok(inner) => inner,
        Err(_) => {
            let p = packets.load(std::sync::atomic::Ordering::Relaxed);
            let k = last_raw_kind.load(std::sync::atomic::Ordering::Relaxed);
            let r = records_count.load(std::sync::atomic::Ordering::Relaxed);
            let msg = format!("view_snapshot_deadline_elapsed_p{p}_k{k}_r{r}");
            Err(Box::leak(msg.into_boxed_str()))
        }
    }
}

#[test]
fn capacity_l6_view_wire_reassembles_large_native_text_without_cross_stream_bytes() {
    let stream = Id128::new([1; 16]);
    let other = Id128::new([2; 16]);
    let body = serde_json::to_vec(&serde_json::json!({
        "text": format!("{}needle-at-boundary{}", "a".repeat(16376), "b".repeat(128))
    }))
    .expect("test JSON");
    let mut assembly = CapacityL6RecordAssembly::new(stream);
    let split = 16384;
    let first = TransferHeader::data(
        stream,
        1,
        7,
        0,
        body.len() as u64,
        RecordKind::OriginalRaw,
        split,
    )
    .expect("first frame");
    assert!(
        assembly
            .receive(first, &body[..split])
            .expect("first fragment")
            .is_none()
    );
    let foreign = TransferHeader::data(
        other,
        2,
        7,
        split as u64,
        body.len() as u64,
        RecordKind::OriginalRaw,
        body.len() - split,
    )
    .expect("foreign frame");
    assert_eq!(
        assembly.receive(foreign, &body[split..]).err(),
        Some("view_wrong_stream_or_repeated_packet")
    );
    let last = TransferHeader::data(
        stream,
        2,
        7,
        split as u64,
        body.len() as u64,
        RecordKind::OriginalRaw,
        body.len() - split,
    )
    .expect("last frame");
    let (kind, actual) = assembly
        .receive(last, &body[split..])
        .expect("last fragment")
        .expect("whole record");
    assert_eq!(kind, RecordKind::OriginalRaw);
    assert_eq!(actual, body);
    let value: serde_json::Value = serde_json::from_slice(&actual).expect("complete JSON");
    assert!(
        value["text"]
            .as_str()
            .expect("text")
            .contains("needle-at-boundary")
    );
}

#[test]
fn capacity_l6_view_wire_rejects_missing_duplicate_and_interleaved_fragments() {
    let stream = Id128::new([3; 16]);
    let mut assembly = CapacityL6RecordAssembly::new(stream);
    let first =
        TransferHeader::data(stream, 1, 9, 0, 6, RecordKind::OriginalRaw, 2).expect("first frame");
    assert!(
        assembly
            .receive(first, b"ab")
            .expect("first fragment")
            .is_none()
    );
    assert_eq!(
        assembly.receive(first, b"ab").err(),
        Some("view_wrong_stream_or_repeated_packet")
    );
    let skipped = TransferHeader::data(stream, 3, 9, 2, 6, RecordKind::OriginalRaw, 4)
        .expect("skipped packet");
    assert_eq!(
        assembly.receive(skipped, b"cdef").err(),
        Some("view_wrong_stream_or_repeated_packet")
    );
    let gap =
        TransferHeader::data(stream, 2, 9, 3, 6, RecordKind::OriginalRaw, 3).expect("gapped frame");
    assert_eq!(
        assembly.receive(gap, b"def").err(),
        Some("view_record_fragment_identity_or_offset_changed")
    );
    let interleaved = TransferHeader::data(stream, 2, 10, 2, 6, RecordKind::OriginalRaw, 4)
        .expect("interleaved record");
    assert_eq!(
        assembly.receive(interleaved, b"cdef").err(),
        Some("view_record_fragment_identity_or_offset_changed")
    );
}

#[test]
fn capacity_l6_view_wire_rebound_discards_only_original_partial_record() {
    let stream = Id128::new([4; 16]);
    let mut assembly = CapacityL6RecordAssembly::new(stream);
    let first =
        TransferHeader::data(stream, 1, 9, 0, 6, RecordKind::OriginalRaw, 2).expect("first");
    assert!(assembly.receive(first, b"ab").expect("prefix").is_none());
    let body = serde_json::to_vec(&serde_json::json!({
        "type":"stream_rebound","previousStreamId":stream.to_base64url()
    }))
    .expect("status JSON");
    let rebound = TransferHeader::data(
        stream,
        2,
        1,
        0,
        body.len() as u64,
        RecordKind::Status,
        body.len(),
    )
    .expect("original complete status");
    let (kind, actual) = assembly
        .receive(rebound, &body)
        .expect("legitimate generation promotion")
        .expect("complete rebound");
    assert_eq!(kind, RecordKind::Status);
    assert_eq!(actual, body);
    assert!(assembly.pending.is_none());
    assert_eq!(assembly.last_packet, 2);
    assert_eq!(
        assembly.receive(rebound, &body).err(),
        Some("view_wrong_stream_or_repeated_packet")
    );
}

#[test]
fn capacity_l6_view_wire_rebound_cannot_hide_invalid_or_unrelated_status() {
    let stream = Id128::new([5; 16]);
    let other = Id128::new([6; 16]);
    for (frame_stream, packet, original, record, status_type, fragmented) in [
        (other, 2, stream, 1, "stream_rebound", false),
        (stream, 3, stream, 1, "stream_rebound", false),
        (stream, 2, other, 1, "stream_rebound", false),
        (stream, 2, stream, 2, "stream_rebound", false),
        (stream, 2, stream, 1, "stream_rebound", true),
        (stream, 2, stream, 1, "other_status", false),
    ] {
        let mut assembly = CapacityL6RecordAssembly::new(stream);
        let first =
            TransferHeader::data(stream, 1, 9, 0, 6, RecordKind::OriginalRaw, 2).expect("first");
        assert!(assembly.receive(first, b"ab").expect("prefix").is_none());
        let body = serde_json::to_vec(&serde_json::json!({
            "type":status_type,"previousStreamId":original.to_base64url()
        }))
        .expect("status JSON");
        let total = body.len() as u64 + u64::from(fragmented);
        let header = TransferHeader::data(
            frame_stream,
            packet,
            record,
            0,
            total,
            RecordKind::Status,
            body.len(),
        )
        .expect("negative header");
        assert!(assembly.receive(header, &body).is_err());
        assert_eq!(assembly.last_packet, 1);
        assert_eq!(assembly.pending.as_ref().expect("original prefix").3, b"ab");
    }
}

#[test]
fn capacity_l6_view_wire_snapshot_end_completes_assembly_without_rebound() {
    let stream = Id128::new([7; 16]);
    let mut assembly = CapacityL6RecordAssembly::new(stream);
    let end_header =
        TransferHeader::data(stream, 1, 1, 0, 0, RecordKind::SnapshotEnd, 0).expect("snapshot end");
    let (kind, actual) = assembly
        .receive(end_header, &[])
        .expect("receive end")
        .expect("record complete");
    assert_eq!(kind, RecordKind::SnapshotEnd);
    assert!(actual.is_empty());
    assert_eq!(assembly.last_packet, 1);
}

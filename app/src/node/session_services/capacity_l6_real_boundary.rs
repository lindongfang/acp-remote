async fn capacity_l6_only_candidate(f: &R3WssCase) -> CapacityL6Result<CapacityL6ClockRoot> {
    let native = &f
        .node
        .as_ref()
        .and_then(|node| node.session_services.as_ref())
        .ok_or("boundary_runtime_missing")?
        .native;
    let source = f
        .selection
        .as_ref()
        .ok_or("boundary_source_missing")?
        .source;
    let candidates=crate::application::native_dispatch::runtime_child_source_clock_probe::rt_source_candidate_clocks(native)
        .map_err(|_|"boundary_candidate_read_failed")?.into_iter()
        .filter(|(s,_,since)|*s==source&&since.is_some()).map(|(_,id,_)|id.to_string()).collect::<Vec<_>>();
    if candidates.len() != 1 {
        return Err("boundary_requires_one_actual_current_candidate");
    }
    capacity_l6_clock_root(f, candidates[0].clone()).await
}

async fn capacity_l6_pre_send_recheck(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    scenario: &CapacityL6Scenario,
    case: &str,
    steps: &mut Vec<serde_json::Value>,
) -> CapacityL6Result<()> {
    use crate::application::native_dispatch::runtime_child_source_clock_probe::l6_automatic_waits;
    let root = capacity_l6_only_candidate(f).await?;
    // The scenario asks the actual Harness to schedule later work. Acceptance of
    // that request does not establish it: the live work below is mandatory.
    capacity_l6_phase_commands(f, c, scenario, "received_before_send_arm", &root.native).await?;
    capacity_l6_clock_eligible(f, &root).await?;
    let baseline = capacity_l6_clock_sample(f, &root)?.0;
    let registered = capacity_l6_call(
        f,
        "source.session.register",
        serde_json::json!({
        "selection":f.selection_json(),"operation":{"kind":"create"}}),
    )
    .await?;
    let operation = registered["result"]["sourceOperationId"]
        .as_str()
        .ok_or("boundary_new_not_registered")?
        .to_owned();
    let pool = capacity_l6_clock_pool(f)?;
    let native = f
        .node
        .as_ref()
        .and_then(|node| node.session_services.as_ref())
        .ok_or("boundary_runtime_missing")?
        .native
        .clone();
    let source = f
        .selection
        .as_ref()
        .ok_or("boundary_source_missing")?
        .source;
    // Pause only the selected original automatic operation, before its SQL
    // transaction. Earlier Create registration and reservation checks run normally.
    let barrier = f.node.as_ref().ok_or("boundary_node_missing")?
        .storage.as_ref().ok_or("boundary_storage_missing")?
        .request_registry().native_metadata_hooks_for_test()
        .pause_next(crate::session_storage::native_metadata_test_hooks::NativeMetadataPoint::AutomaticReleaseBeforeBegin)
        .map_err(|_| "boundary_original_prebegin_cut_not_armed")?;
    let gate = CapacityL6ReserveGate(barrier.clone());
    let request = Id128::new(crate::random::bytes().map_err(|_| "system_random_unavailable")?);
    capacity_l6_pace_command(f).await?;
    send_json(
        &mut f.peers[0].socket,
        &serde_json::json!({"v":1,"type":"command","requestId":request,
        "action":"source.session.execute","payload":{"sourceOperationId":operation}}),
    )
    .await;
    timeout(Duration::from_secs(5), barrier.wait_until_entered())
        .await
        .map_err(|_| "boundary_selected_original_prebegin_cut_not_entered")?;
    let selected = timeout(Duration::from_secs(5), async {
        loop {
            if let Some((_, _, id)) = l6_automatic_waits(&native)
                .map_err(|_| "boundary_selected_owner_unavailable")?
                .into_iter()
                .find(|(s, n, _)| *s == source && n.as_ref() == root.native)
            {
                break Ok::<_, &'static str>(id);
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "boundary_original_selection_not_observed")??;
    let persisted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM node_session_automatic_releases WHERE operation_id=?",
    )
    .bind(selected.encode())
    .fetch_one(&pool)
    .await
    .map_err(|_| "boundary_original_precommit_query_failed")?;
    if persisted != 0 {
        return Err("boundary_did_not_hold_original_before_sql_commit");
    }
    // The hook supplies no work facts and owns no SQLite connection. Only the
    // actual peer's received Current activity satisfies this ordering.
    let has_selected_phase = scenario
        .phases
        .get("received_before_send_selected")
        .is_some_and(|commands| !commands.is_empty());
    if has_selected_phase {
        // peer[0] 正等待已挂起的 source.session.execute 响应，必须通过已认证的 peer[1] 提交真实输入。
        capacity_l6_phase_commands_on(
            f,
            c,
            scenario,
            "received_before_send_selected",
            &root.native,
            1,
        )
        .await?;
    }
    let wait_secs = if has_selected_phase { 60 } else { 5 };
    let received = timeout(Duration::from_secs(wait_secs), async {
        loop {
            let facts = capacity_l6_known_work(f, &root.native)?;
            if capacity_l6_known_matches(&facts, &root.native, "any_known")
                && capacity_l6_clock_sample(f, &root)?.0 > baseline
            {
                break Ok::<_, &'static str>(capacity_l6_clock_sample(f, &root)?.0);
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "real_deferred_current_work_not_received_before_send")??;
    drop(gate);
    let response = timeout(Duration::from_secs(90), async {
        loop {
            match f.peers[0].socket.next().await {
                Some(Ok(Message::Text(text))) => {
                    let response: serde_json::Value = serde_json::from_str(text.as_str())
                        .map_err(|_| "boundary_invalid_management_response")?;
                    if response["requestId"] == request.to_base64url() {
                        break Ok(response);
                    }
                    if response.get("requestId").is_some() {
                        break Err("boundary_wrong_management_response");
                    }
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                _ => break Err("boundary_management_socket_ended"),
            }
        }
    })
    .await
    .map_err(|_| "boundary_original_attempt_timeout")??;
    let outcome: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT phase,outcome FROM node_session_automatic_releases WHERE operation_id=?",
    )
    .bind(selected.encode())
    .fetch_optional(&pool)
    .await
    .map_err(|_| "boundary_original_outcome_query_failed")?;
    let state: Option<(i64, i64)> = sqlx::query_as(
        "SELECT physically_closed,released FROM node_session_capacity WHERE reservation_id=?",
    )
    .bind(&root.reservation)
    .fetch_optional(&pool)
    .await
    .map_err(|_| "boundary_original_resource_query_failed")?;
    if outcome != Some(("withdrawn".to_owned(), Some("withdrawn".to_owned())))
        || state != Some((0, 0))
        || response["result"]["outcome"] == "succeeded"
        || capacity_l6_inspect(f).await?["occupied"] != 50
    {
        return Err("received_real_work_did_not_withdraw_original_without_refund");
    }
    steps.push(serde_json::json!({"step":"actual_received_current_work_before_close_first_byte",
        "reservation":root.reservation,"operationId":selected.encode(),
        "actualReceiptAdvanced":received>baseline,"outcome":"withdrawn","occupied":50,
        "boundary":"owned Node selected-before-SQL pause only; actual peer activity, no synthetic observation"}));
    capacity_l6_mark(
        steps,
        case,
        if case == "cp011" {
            "pressure_before_send_received_work_recheck"
        } else {
            "received_before_send_recheck"
        },
    )?;
    let finish_started = std::time::Instant::now();
    capacity_l6_phase_commands(f, c, scenario, "received_before_send_finish", &root.native).await?;
    capacity_l6_boundary_requalify(f, &root, &selected.encode(), finish_started).await?;
    Ok(())
}

fn capacity_l6_boundary_requalification_valid(
    row: (i64, i64, i64, i64),
    received: std::time::Instant,
    candidate: Option<std::time::Instant>,
    finish_started: std::time::Instant,
) -> bool {
    row == (0, 0, 1, 0)
        && received >= finish_started
        && candidate.is_some_and(|since| since >= finish_started && since >= received)
}

async fn capacity_l6_boundary_original_state(
    pool: &sqlx::SqlitePool,
    root: &CapacityL6ClockRoot,
    selected: &str,
    source: &str,
) -> CapacityL6Result<(i64, i64, i64, i64)> {
    sqlx::query_as(
        "SELECT c.physically_closed,c.released,\
        (SELECT count(*) FROM node_session_automatic_releases o \
         JOIN node_session_automatic_release_members selected_member ON selected_member.operation_id=o.operation_id \
         WHERE o.operation_id=? AND selected_member.reservation_id=c.reservation_id \
         AND o.anchor_reservation=c.reservation_id \
         AND o.phase='withdrawn' AND o.outcome='withdrawn'),\
        (SELECT count(*) FROM node_session_automatic_releases o \
         JOIN node_session_automatic_release_members m ON m.operation_id=o.operation_id \
         WHERE m.reservation_id=c.reservation_id AND o.operation_id!=? \
         AND o.phase NOT IN ('withdrawn','finished')) \
        FROM node_session_capacity c WHERE c.reservation_id=? AND c.native_id=? \
        AND c.instance_id=? AND c.runtime_epoch=? AND c.source_id=?",
    )
    .bind(selected)
    .bind(selected)
    .bind(&root.reservation)
    .bind(&root.native)
    .bind(&root.instance)
    .bind(&root.epoch)
    .bind(source)
    .fetch_optional(pool)
    .await
    .map_err(|_| "boundary_requalification_original_query_failed")?
    .ok_or("boundary_requalification_original_owner_changed")
}

async fn capacity_l6_boundary_requalify(
    f: &R3WssCase,
    root: &CapacityL6ClockRoot,
    selected: &str,
    finish_started: std::time::Instant,
) -> CapacityL6Result<()> {
    let pool = capacity_l6_clock_pool(f)?;
    let source = f
        .selection
        .as_ref()
        .ok_or("boundary_requalification_source_missing")?
        .source
        .encode();
    let until = tokio::time::Instant::now() + Duration::from_secs(45);
    loop {
        let row = capacity_l6_boundary_original_state(&pool, root, selected, &source).await?;
        if row != (0, 0, 1, 0) {
            return Err("boundary_requalification_owner_or_original_operation_changed");
        }
        let (received, candidate) = capacity_l6_clock_sample(f, root)?;
        if capacity_l6_boundary_requalification_valid(row, received, candidate, finish_started) {
            return Ok(());
        }
        if tokio::time::Instant::now() >= until {
            let facts = capacity_l6_known_work(f, &root.native)?;
            for fact in facts {
                eprintln!(
                    "L6_BOUNDARY_REQUALIFICATION foreground={:?} background={:?} tools={} approvals={} receive_after_finish={} candidate_present={} candidate_after_receipt={}",
                    fact.foreground,
                    fact.background,
                    fact.tools,
                    fact.pending_approvals,
                    received >= finish_started,
                    candidate.is_some(),
                    candidate.is_some_and(|since| since >= received)
                );
            }
            let target: Option<String> = sqlx::query_scalar(
                "SELECT target_id FROM product_targets WHERE source_id=? AND native_id=?",
            )
            .bind(&source)
            .bind(&root.native)
            .fetch_optional(&pool)
            .await
            .map_err(|_| "boundary_diagnostic_target_query_failed")?;
            if let Some(target) = target.and_then(|id| crate::domain::EntityId::decode(&id).ok()) {
                let native = &f
                    .node
                    .as_ref()
                    .ok_or("boundary_node_missing")?
                    .session_services
                    .as_ref()
                    .ok_or("boundary_runtime_missing")?
                    .native;
                if let Some(snapshot) = native
                    .park_snapshot_for_test(target)
                    .map_err(|_| "boundary_diagnostic_park_query_failed")?
                {
                    eprintln!(
                        "L6_BOUNDARY_OWNER required={} target_requirements={} failed_requirements={} control_holds={} pending_input={} ticket_present={}",
                        snapshot.source_required,
                        snapshot.target_requirements,
                        snapshot.failed_requirements,
                        snapshot.control_holds,
                        snapshot.attempt.is_some(),
                        snapshot.ticket.is_some()
                    );
                }
            }
            return Err("boundary_real_current_work_did_not_end_in_fresh_eligibility");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[test]
fn capacity_l6_boundary_withdrawn_history_accepts_only_fresh_original_eligibility() {
    let started = std::time::Instant::now();
    let received = started + Duration::from_millis(1);
    let candidate = received + Duration::from_millis(1);
    assert!(capacity_l6_boundary_requalification_valid(
        (0, 0, 1, 0),
        received,
        Some(candidate),
        started,
    ));
    for row in [(1, 0, 1, 0), (0, 1, 1, 0), (0, 0, 0, 0), (0, 0, 1, 1)] {
        assert!(!capacity_l6_boundary_requalification_valid(
            row,
            received,
            Some(candidate),
            started,
        ));
    }
}

#[test]
fn capacity_l6_boundary_stale_or_still_protected_clocks_cannot_requalify() {
    let previous = std::time::Instant::now();
    let started = previous + Duration::from_secs(1);
    let received = started + Duration::from_millis(1);
    for (received, candidate) in [
        (previous, Some(started)),
        (received, Some(previous)),
        (received, None),
        (received, Some(started)),
    ] {
        assert!(!capacity_l6_boundary_requalification_valid(
            (0, 0, 1, 0),
            received,
            candidate,
            started,
        ));
    }
}

#[tokio::test]
async fn capacity_l6_boundary_sql_keeps_withdrawn_original_and_rejects_pending_member_owner() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    // This is an observer-query contract, not a production-schema migration or
    // a synthetic real-Harness owner. The original query runs unchanged.
    for statement in [
        "CREATE TABLE node_session_capacity(reservation_id TEXT,native_id TEXT,instance_id TEXT,runtime_epoch TEXT,physically_closed INTEGER,released INTEGER,source_id TEXT)",
        "CREATE TABLE node_session_automatic_releases(operation_id TEXT,anchor_reservation TEXT,phase TEXT,outcome TEXT)",
        "CREATE TABLE node_session_automatic_release_members(operation_id TEXT,reservation_id TEXT)",
        "INSERT INTO node_session_capacity VALUES('r','n','i','e',0,0,'s')",
        "INSERT INTO node_session_automatic_releases VALUES('original','r','withdrawn','withdrawn')",
        "INSERT INTO node_session_automatic_release_members VALUES('original','r')",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    let root = CapacityL6ClockRoot {
        native: "n".into(),
        reservation: "r".into(),
        instance: "i".into(),
        epoch: "e".into(),
    };
    let original = capacity_l6_boundary_original_state(&pool, &root, "original", "s")
        .await
        .unwrap();
    assert_eq!(original, (0, 0, 1, 0));
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM node_session_automatic_releases WHERE anchor_reservation='r'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        count, 1,
        "withdrawn evidence is retained; generic clock_eligible's zero-history prerequisite cannot be used here"
    );
    let started = std::time::Instant::now();
    assert!(capacity_l6_boundary_requalification_valid(
        original,
        started,
        Some(started),
        started
    ));
    sqlx::query("INSERT INTO node_session_automatic_releases VALUES('other','another-anchor','pending',NULL)")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO node_session_automatic_release_members VALUES('other','r')")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        capacity_l6_boundary_original_state(&pool, &root, "original", "s")
            .await
            .unwrap(),
        (0, 0, 1, 1)
    );
    assert_eq!(
        capacity_l6_boundary_original_state(&pool, &root, "wrong-operation", "s")
            .await
            .unwrap()
            .2,
        0
    );
    assert!(
        capacity_l6_boundary_original_state(&pool, &root, "original", "wrong-source")
            .await
            .is_err()
    );
    let wrong = CapacityL6ClockRoot {
        epoch: "new-generation".into(),
        ..root
    };
    assert!(
        capacity_l6_boundary_original_state(&pool, &wrong, "original", "s")
            .await
            .is_err()
    );
    pool.close().await;
}

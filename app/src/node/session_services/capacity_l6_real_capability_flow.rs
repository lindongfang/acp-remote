async fn capacity_l6_cp032_body(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    scenario: &CapacityL6Scenario,
    steps: &mut Vec<serde_json::Value>,
) -> CapacityL6Result<()> {
    capacity_l6_cp032_capabilities(f, c, scenario, steps).await?;
    capacity_l6_pre_send_recheck(f, c, scenario, "cp032", steps).await?;
    let original_operation = capacity_l6_cp032_finish(f, c, steps).await?;
    capacity_l6_cp032_restart(f, c, &original_operation, steps).await?;
    capacity_l6_original_ticket_faults(f, c, scenario, "cp032", steps).await
}

//! Development checks for fixed hook reachability; historical acceptance is maintained separately.
use super::*;
use crate::session_storage::schema;
use sqlx::sqlite::SqlitePoolOptions;
use std::error::Error;

struct FileRoots {
    path: std::path::PathBuf,
}
impl FileRoots {
    fn remove_owned(&self) -> std::io::Result<()> {
        std::fs::remove_file(&self.path)
    }
}
type TestResult = Result<(), Box<dyn Error>>;
fn scenarios() -> Vec<NamespaceScenario> {
    let mut scenarios = Vec::new();
    for table in [
        NecessaryTable::Commands,
        NecessaryTable::Profiles,
        NecessaryTable::Selection,
        NecessaryTable::Meta,
        NecessaryTable::Clients,
        NecessaryTable::Sources,
        NecessaryTable::HarnessSources,
        NecessaryTable::Targets,
        NecessaryTable::Harnesses,
        NecessaryTable::Workspaces,
    ] {
        for kind in [ShadowKind::Table, ShadowKind::View] {
            scenarios.push(NamespaceScenario::Shadow(table, kind));
        }
        for schema in [TriggerSchema::Main, TriggerSchema::Temp] {
            scenarios.push(NamespaceScenario::Trigger(table, schema));
        }
    }
    for pragma in [
        PragmaObject::ForeignKeyList,
        PragmaObject::IndexList,
        PragmaObject::IndexXinfo,
        PragmaObject::TableInfo,
        PragmaObject::ForeignKeyCheck,
        PragmaObject::TableList,
        PragmaObject::UserVersion,
    ] {
        for schema in [ObjectSchema::Main, ObjectSchema::Temp] {
            for kind in [ShadowKind::Table, ShadowKind::View] {
                scenarios.push(NamespaceScenario::Pragma(pragma, schema, kind));
            }
        }
    }
    scenarios
}

#[tokio::test]
async fn all_fixed_namespace_scripts_are_rejected_on_original_transaction() -> TestResult {
    let scenarios = scenarios();
    assert_eq!(scenarios.len(), 68);
    for scenario in scenarios {
        struct MemoryRoots {
            scenario: NamespaceScenario,
        }
        let roots = std::sync::Arc::new(MemoryRoots { scenario });
        let raw_scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
            std::path::Path::new(":memory:"),
            roots.clone(),
        );
        raw_scope.start_raw().await;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(raw_scope.options().in_memory(true).foreign_keys(true))
            .await?;
        schema::initialize(&pool).await?;
        let scenario = roots.scenario;
        let result = schema::initialize_catalog_for_test(&pool, None, Some(scenario)).await;
        assert!(
            matches!(result, Err(StorageError::UnrecognizedSchema)),
            "{scenario:?}: {result:?}"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA main.user_version")
                .fetch_one(&pool)
                .await?,
            crate::storage::SCHEMA_VERSION
        );
        let canaries: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM main.sqlite_schema WHERE name='schema9_namespace_canary' OR substr(lower(name),1,7)='pragma_') + (SELECT COUNT(*) FROM temp.sqlite_schema WHERE name='schema9_namespace_canary' OR substr(lower(name),1,7)='pragma_')").fetch_one(&pool).await?;
        assert_eq!(canaries, 0, "{scenario:?}");
        schema::initialize(&pool).await?;
        pool.close().await;
        raw_scope.stop_and_observe().await;
    }
    Ok(())
}

#[tokio::test]
async fn storage_hook_failure_uses_original_open_and_close_chain() -> TestResult {
    let serial = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "acp-schema9-hook-{}-{serial}.sqlite3",
        std::process::id()
    ));
    let roots = std::sync::Arc::new(FileRoots { path });
    let path = &roots.path;
    let scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(path, roots.clone());
    let storage = scope.open().await?;
    storage.close().await;
    scope.stop_and_observe().await;
    drop(storage);
    let failed_scope =
        crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(path, roots.clone());
    let result = failed_scope
        .open_catalog(
            None,
            Some(NamespaceScenario::Pragma(
                PragmaObject::ForeignKeyList,
                ObjectSchema::Temp,
                ShadowKind::View,
            )),
        )
        .await;
    assert!(
        matches!(result, Err(ref error) if matches!(error.as_ref(), StorageError::UnrecognizedSchema))
    );
    failed_scope.stop_and_observe().await;
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let exclusive = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(path)?;
        drop(exclusive);
    }
    for extension in ["-wal", "-shm"] {
        assert!(!std::path::PathBuf::from(format!("{}{extension}", path.display())).exists());
    }
    let scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(path, roots.clone());
    let storage = scope.open().await?;
    storage.close().await;
    scope.stop_and_observe().await;
    drop(storage);
    roots.remove_owned()?;
    Ok(())
}

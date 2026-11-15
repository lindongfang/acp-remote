//! Test-only fixed namespace scenarios on the original migration transaction.
use crate::error::StorageError;
use sqlx::SqliteConnection;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NecessaryTable {
    Commands,
    Profiles,
    Selection,
    Meta,
    Clients,
    Sources,
    HarnessSources,
    Targets,
    Harnesses,
    Workspaces,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShadowKind {
    Table,
    View,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TriggerSchema {
    Main,
    Temp,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PragmaObject {
    ForeignKeyList,
    IndexList,
    IndexXinfo,
    TableInfo,
    ForeignKeyCheck,
    TableList,
    UserVersion,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectSchema {
    Main,
    Temp,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NamespaceScenario {
    Shadow(NecessaryTable, ShadowKind),
    Trigger(NecessaryTable, TriggerSchema),
    Pragma(PragmaObject, ObjectSchema, ShadowKind),
}
impl NamespaceScenario {
    pub(crate) async fn install(
        self,
        connection: &mut SqliteConnection,
    ) -> Result<(), StorageError> {
        sqlx::query(self.sql()).execute(connection).await?;
        Ok(())
    }
    const fn sql(self) -> &'static str {
        match self {
            Self::Shadow(table, kind) => shadow_sql(table, kind),
            Self::Trigger(table, schema) => trigger_sql(table, schema),
            Self::Pragma(pragma, schema, kind) => pragma_sql(pragma, schema, kind),
        }
    }
}

const fn shadow_sql(table: NecessaryTable, kind: ShadowKind) -> &'static str {
    use NecessaryTable::*;
    match (table, kind) {
        (Commands, ShadowKind::Table) => {
            "CREATE TEMP TABLE node_catalog_commands(schema9_canary INTEGER)"
        }
        (Commands, ShadowKind::View) => {
            "CREATE TEMP VIEW node_catalog_commands AS SELECT 1 AS schema9_canary"
        }
        (Profiles, ShadowKind::Table) => {
            "CREATE TEMP TABLE node_launch_profiles(schema9_canary INTEGER)"
        }
        (Profiles, ShadowKind::View) => {
            "CREATE TEMP VIEW node_launch_profiles AS SELECT 1 AS schema9_canary"
        }
        (Selection, ShadowKind::Table) => {
            "CREATE TEMP TABLE node_target_launch_selection(schema9_canary INTEGER)"
        }
        (Selection, ShadowKind::View) => {
            "CREATE TEMP VIEW node_target_launch_selection AS SELECT 1 AS schema9_canary"
        }
        (Meta, ShadowKind::Table) => "CREATE TEMP TABLE node_meta(schema9_canary INTEGER)",
        (Meta, ShadowKind::View) => "CREATE TEMP VIEW node_meta AS SELECT 1 AS schema9_canary",
        (Clients, ShadowKind::Table) => "CREATE TEMP TABLE clients(schema9_canary INTEGER)",
        (Clients, ShadowKind::View) => "CREATE TEMP VIEW clients AS SELECT 1 AS schema9_canary",
        (Sources, ShadowKind::Table) => "CREATE TEMP TABLE product_sources(schema9_canary INTEGER)",
        (Sources, ShadowKind::View) => {
            "CREATE TEMP VIEW product_sources AS SELECT 1 AS schema9_canary"
        }
        (HarnessSources, ShadowKind::Table) => {
            "CREATE TEMP TABLE product_harness_sources(schema9_canary INTEGER)"
        }
        (HarnessSources, ShadowKind::View) => {
            "CREATE TEMP VIEW product_harness_sources AS SELECT 1 AS schema9_canary"
        }
        (Targets, ShadowKind::Table) => "CREATE TEMP TABLE product_targets(schema9_canary INTEGER)",
        (Targets, ShadowKind::View) => {
            "CREATE TEMP VIEW product_targets AS SELECT 1 AS schema9_canary"
        }
        (Harnesses, ShadowKind::Table) => "CREATE TEMP TABLE harnesses(schema9_canary INTEGER)",
        (Harnesses, ShadowKind::View) => "CREATE TEMP VIEW harnesses AS SELECT 1 AS schema9_canary",
        (Workspaces, ShadowKind::Table) => "CREATE TEMP TABLE workspaces(schema9_canary INTEGER)",
        (Workspaces, ShadowKind::View) => {
            "CREATE TEMP VIEW workspaces AS SELECT 1 AS schema9_canary"
        }
    }
}

const fn trigger_sql(table: NecessaryTable, schema: TriggerSchema) -> &'static str {
    use NecessaryTable::*;
    match (table, schema) {
        (Commands, TriggerSchema::Main) => {
            "CREATE TRIGGER main.schema9_namespace_canary AFTER INSERT ON node_catalog_commands BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
        (Commands, TriggerSchema::Temp) => {
            "CREATE TEMP TRIGGER schema9_namespace_canary AFTER INSERT ON main.node_catalog_commands BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
        (Profiles, TriggerSchema::Main) => {
            "CREATE TRIGGER main.schema9_namespace_canary AFTER INSERT ON node_launch_profiles BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
        (Profiles, TriggerSchema::Temp) => {
            "CREATE TEMP TRIGGER schema9_namespace_canary AFTER INSERT ON main.node_launch_profiles BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
        (Selection, TriggerSchema::Main) => {
            "CREATE TRIGGER main.schema9_namespace_canary AFTER INSERT ON node_target_launch_selection BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
        (Selection, TriggerSchema::Temp) => {
            "CREATE TEMP TRIGGER schema9_namespace_canary AFTER INSERT ON main.node_target_launch_selection BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
        (Meta, TriggerSchema::Main) => {
            "CREATE TRIGGER main.schema9_namespace_canary AFTER INSERT ON node_meta BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
        (Meta, TriggerSchema::Temp) => {
            "CREATE TEMP TRIGGER schema9_namespace_canary AFTER INSERT ON main.node_meta BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
        (Clients, TriggerSchema::Main) => {
            "CREATE TRIGGER main.schema9_namespace_canary AFTER INSERT ON clients BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
        (Clients, TriggerSchema::Temp) => {
            "CREATE TEMP TRIGGER schema9_namespace_canary AFTER INSERT ON main.clients BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
        (Sources, TriggerSchema::Main) => {
            "CREATE TRIGGER main.schema9_namespace_canary AFTER INSERT ON product_sources BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
        (Sources, TriggerSchema::Temp) => {
            "CREATE TEMP TRIGGER schema9_namespace_canary AFTER INSERT ON main.product_sources BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
        (HarnessSources, TriggerSchema::Main) => {
            "CREATE TRIGGER main.schema9_namespace_canary AFTER INSERT ON product_harness_sources BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
        (HarnessSources, TriggerSchema::Temp) => {
            "CREATE TEMP TRIGGER schema9_namespace_canary AFTER INSERT ON main.product_harness_sources BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
        (Targets, TriggerSchema::Main) => {
            "CREATE TRIGGER main.schema9_namespace_canary AFTER INSERT ON product_targets BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
        (Targets, TriggerSchema::Temp) => {
            "CREATE TEMP TRIGGER schema9_namespace_canary AFTER INSERT ON main.product_targets BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
        (Harnesses, TriggerSchema::Main) => {
            "CREATE TRIGGER main.schema9_namespace_canary AFTER INSERT ON harnesses BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
        (Harnesses, TriggerSchema::Temp) => {
            "CREATE TEMP TRIGGER schema9_namespace_canary AFTER INSERT ON main.harnesses BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
        (Workspaces, TriggerSchema::Main) => {
            "CREATE TRIGGER main.schema9_namespace_canary AFTER INSERT ON workspaces BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
        (Workspaces, TriggerSchema::Temp) => {
            "CREATE TEMP TRIGGER schema9_namespace_canary AFTER INSERT ON main.workspaces BEGIN SELECT RAISE(ABORT,'schema9-canary'); END"
        }
    }
}

const fn pragma_sql(pragma: PragmaObject, schema: ObjectSchema, kind: ShadowKind) -> &'static str {
    match (pragma, schema, kind) {
        (PragmaObject::ForeignKeyList, ObjectSchema::Main, ShadowKind::Table) => {
            "CREATE TABLE main.pragma_foreign_key_list(schema9_canary INTEGER)"
        }
        (PragmaObject::ForeignKeyList, ObjectSchema::Main, ShadowKind::View) => {
            "CREATE VIEW main.pragma_foreign_key_list AS SELECT 1 AS schema9_canary"
        }
        (PragmaObject::ForeignKeyList, ObjectSchema::Temp, ShadowKind::Table) => {
            "CREATE TEMP TABLE pragma_foreign_key_list(schema9_canary INTEGER)"
        }
        (PragmaObject::ForeignKeyList, ObjectSchema::Temp, ShadowKind::View) => {
            "CREATE TEMP VIEW pragma_foreign_key_list AS SELECT 1 AS schema9_canary"
        }
        (PragmaObject::IndexList, ObjectSchema::Main, ShadowKind::Table) => {
            "CREATE TABLE main.pragma_index_list(schema9_canary INTEGER)"
        }
        (PragmaObject::IndexList, ObjectSchema::Main, ShadowKind::View) => {
            "CREATE VIEW main.pragma_index_list AS SELECT 1 AS schema9_canary"
        }
        (PragmaObject::IndexList, ObjectSchema::Temp, ShadowKind::Table) => {
            "CREATE TEMP TABLE pragma_index_list(schema9_canary INTEGER)"
        }
        (PragmaObject::IndexList, ObjectSchema::Temp, ShadowKind::View) => {
            "CREATE TEMP VIEW pragma_index_list AS SELECT 1 AS schema9_canary"
        }
        (PragmaObject::IndexXinfo, ObjectSchema::Main, ShadowKind::Table) => {
            "CREATE TABLE main.pragma_index_xinfo(schema9_canary INTEGER)"
        }
        (PragmaObject::IndexXinfo, ObjectSchema::Main, ShadowKind::View) => {
            "CREATE VIEW main.pragma_index_xinfo AS SELECT 1 AS schema9_canary"
        }
        (PragmaObject::IndexXinfo, ObjectSchema::Temp, ShadowKind::Table) => {
            "CREATE TEMP TABLE pragma_index_xinfo(schema9_canary INTEGER)"
        }
        (PragmaObject::IndexXinfo, ObjectSchema::Temp, ShadowKind::View) => {
            "CREATE TEMP VIEW pragma_index_xinfo AS SELECT 1 AS schema9_canary"
        }
        (PragmaObject::TableInfo, ObjectSchema::Main, ShadowKind::Table) => {
            "CREATE TABLE main.pragma_table_info(schema9_canary INTEGER)"
        }
        (PragmaObject::TableInfo, ObjectSchema::Main, ShadowKind::View) => {
            "CREATE VIEW main.pragma_table_info AS SELECT 1 AS schema9_canary"
        }
        (PragmaObject::TableInfo, ObjectSchema::Temp, ShadowKind::Table) => {
            "CREATE TEMP TABLE pragma_table_info(schema9_canary INTEGER)"
        }
        (PragmaObject::TableInfo, ObjectSchema::Temp, ShadowKind::View) => {
            "CREATE TEMP VIEW pragma_table_info AS SELECT 1 AS schema9_canary"
        }
        (PragmaObject::ForeignKeyCheck, ObjectSchema::Main, ShadowKind::Table) => {
            "CREATE TABLE main.pragma_foreign_key_check(schema9_canary INTEGER)"
        }
        (PragmaObject::ForeignKeyCheck, ObjectSchema::Main, ShadowKind::View) => {
            "CREATE VIEW main.pragma_foreign_key_check AS SELECT 1 AS schema9_canary"
        }
        (PragmaObject::ForeignKeyCheck, ObjectSchema::Temp, ShadowKind::Table) => {
            "CREATE TEMP TABLE pragma_foreign_key_check(schema9_canary INTEGER)"
        }
        (PragmaObject::ForeignKeyCheck, ObjectSchema::Temp, ShadowKind::View) => {
            "CREATE TEMP VIEW pragma_foreign_key_check AS SELECT 1 AS schema9_canary"
        }
        (PragmaObject::TableList, ObjectSchema::Main, ShadowKind::Table) => {
            "CREATE TABLE main.pragma_table_list(schema9_canary INTEGER)"
        }
        (PragmaObject::TableList, ObjectSchema::Main, ShadowKind::View) => {
            "CREATE VIEW main.pragma_table_list AS SELECT 1 AS schema9_canary"
        }
        (PragmaObject::TableList, ObjectSchema::Temp, ShadowKind::Table) => {
            "CREATE TEMP TABLE pragma_table_list(schema9_canary INTEGER)"
        }
        (PragmaObject::TableList, ObjectSchema::Temp, ShadowKind::View) => {
            "CREATE TEMP VIEW pragma_table_list AS SELECT 1 AS schema9_canary"
        }
        (PragmaObject::UserVersion, ObjectSchema::Main, ShadowKind::Table) => {
            "CREATE TABLE main.pragma_user_version(schema9_canary INTEGER)"
        }
        (PragmaObject::UserVersion, ObjectSchema::Main, ShadowKind::View) => {
            "CREATE VIEW main.pragma_user_version AS SELECT 1 AS schema9_canary"
        }
        (PragmaObject::UserVersion, ObjectSchema::Temp, ShadowKind::Table) => {
            "CREATE TEMP TABLE pragma_user_version(schema9_canary INTEGER)"
        }
        (PragmaObject::UserVersion, ObjectSchema::Temp, ShadowKind::View) => {
            "CREATE TEMP VIEW pragma_user_version AS SELECT 1 AS schema9_canary"
        }
    }
}

#[cfg(test)]
mod tests;

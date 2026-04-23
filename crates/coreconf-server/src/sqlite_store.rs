use crate::audit::AuditEvent;
use crate::store::Store;
use coreconf_schema::CompiledSchemaBundle;
use rusqlite::{params, Connection};
use serde_json::Value;
use std::path::Path;

pub struct SqliteStore {
    conn: Connection,
}

impl SqliteStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|err| err.to_string())?;
        conn.execute_batch(
            "
            create table if not exists bundles (
                schema_version text primary key,
                bundle_json text not null
            );
            create table if not exists snapshots (
                schema_version text primary key,
                snapshot_json text not null
            );
            create table if not exists server_state (
                id integer primary key check (id = 1),
                active_schema_version text
            );
            create table if not exists audit_events (
                id integer primary key autoincrement,
                actor text not null,
                action text not null,
                resource text not null
            );
            insert or ignore into server_state (id, active_schema_version) values (1, null);
            ",
        )
        .map_err(|err| err.to_string())?;
        Ok(Self { conn })
    }
}

impl Store for SqliteStore {
    fn write_bundle(
        &mut self,
        schema_version: &str,
        bundle: &CompiledSchemaBundle,
    ) -> Result<(), String> {
        let bundle_json = serde_json::to_string(bundle).map_err(|err| err.to_string())?;
        self.conn
            .execute(
                "insert or replace into bundles (schema_version, bundle_json) values (?1, ?2)",
                params![schema_version, bundle_json],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    fn read_bundle(&self, schema_version: &str) -> Result<Option<CompiledSchemaBundle>, String> {
        let mut stmt = self
            .conn
            .prepare("select bundle_json from bundles where schema_version = ?1")
            .map_err(|err| err.to_string())?;
        let mut rows = stmt.query(params![schema_version]).map_err(|err| err.to_string())?;
        match rows.next().map_err(|err| err.to_string())? {
            Some(row) => {
                let json: String = row.get(0).map_err(|err| err.to_string())?;
                serde_json::from_str(&json)
                    .map(Some)
                    .map_err(|err| err.to_string())
            }
            None => Ok(None),
        }
    }

    fn set_active_schema_version(&mut self, schema_version: &str) -> Result<(), String> {
        self.conn
            .execute(
                "update server_state set active_schema_version = ?1 where id = 1",
                params![schema_version],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    fn active_schema_version(&self) -> Result<Option<String>, String> {
        let mut stmt = self
            .conn
            .prepare("select active_schema_version from server_state where id = 1")
            .map_err(|err| err.to_string())?;
        let mut rows = stmt.query([]).map_err(|err| err.to_string())?;
        match rows.next().map_err(|err| err.to_string())? {
            Some(row) => row.get(0).map_err(|err| err.to_string()),
            None => Ok(None),
        }
    }

    fn write_snapshot(&mut self, schema_version: &str, snapshot: &Value) -> Result<(), String> {
        self.conn
            .execute(
                "insert or replace into snapshots (schema_version, snapshot_json) values (?1, ?2)",
                params![schema_version, snapshot.to_string()],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    fn read_snapshot(&self, schema_version: &str) -> Result<Option<Value>, String> {
        let mut stmt = self
            .conn
            .prepare("select snapshot_json from snapshots where schema_version = ?1")
            .map_err(|err| err.to_string())?;
        let mut rows = stmt.query(params![schema_version]).map_err(|err| err.to_string())?;
        match rows.next().map_err(|err| err.to_string())? {
            Some(row) => {
                let json: String = row.get(0).map_err(|err| err.to_string())?;
                serde_json::from_str(&json)
                    .map(Some)
                    .map_err(|err| err.to_string())
            }
            None => Ok(None),
        }
    }

    fn append_audit(&mut self, event: AuditEvent) -> Result<(), String> {
        self.conn
            .execute(
                "insert into audit_events (actor, action, resource) values (?1, ?2, ?3)",
                params![event.actor, event.action, event.resource],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    fn read_audit(&self) -> Result<Vec<AuditEvent>, String> {
        let mut stmt = self
            .conn
            .prepare("select actor, action, resource from audit_events order by id")
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(AuditEvent {
                    actor: row.get(0)?,
                    action: row.get(1)?,
                    resource: row.get(2)?,
                })
            })
            .map_err(|err| err.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| err.to_string())
    }
}

//! Reconcile complete source snapshots. Manual entries never become source-owned.
use super::{Item, SyncResult};
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};

pub fn init_schema(conn: &Connection) -> Result<(), String> {
    let columns = conn
        .prepare("PRAGMA table_info(launch_items)")
        .map_err(|e| e.to_string())?
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if !columns.iter().any(|name| name == "sync_source") {
        // Migration is atomic, including provenance recovery for old imported rows.
        let tx =
            rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
        tx.execute_batch(
            "ALTER TABLE launch_items ADD COLUMN sync_source TEXT NOT NULL DEFAULT '';
             UPDATE launch_items SET sync_source='apps'
               WHERE type='application' AND updated_at='';
             UPDATE launch_items SET sync_source='bookmarks'
               WHERE type='website' AND (description LIKE '来自Chrome%收藏夹%'
                 OR description LIKE '来自Edge%收藏夹%' OR description LIKE '来自Brave%收藏夹%');
             UPDATE launch_items SET sync_source='folders'
               WHERE type IN ('file','folder') AND (description='同步目录' OR description LIKE '来自%');",
        ).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn key(item: &Item) -> String {
    // Overlapping roots may give the same folder different display names.
    if matches!(item.kind.as_str(), "folder" | "file") {
        return format!(
            "{}|{}",
            item.kind,
            item.url
                .replace('/', "\\")
                .trim_end_matches('\\')
                .to_lowercase()
        );
    }
    super::item_dedupe_key(&item.kind, &item.name, &item.url, &item.args)
        .unwrap_or_else(|| format!("{}|{}", item.kind, item.url))
}

pub fn reconcile(
    conn: &Connection,
    source: &str,
    items: &[Item],
    retain_unseen: impl Fn(&Item) -> bool,
) -> Result<SyncResult, String> {
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let existing = {
        let query = match source {
            "apps" => "SELECT * FROM launch_items WHERE type='application' ORDER BY id",
            "bookmarks" => "SELECT * FROM launch_items WHERE type='website' ORDER BY id",
            "folders" => "SELECT * FROM launch_items WHERE type IN ('file','folder') ORDER BY id",
            _ => return Err(format!("未知同步来源：{source}")),
        };
        let mut stmt = tx.prepare(query).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    super::row_to_item(row)?,
                    row.get::<_, String>("sync_source")?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    let mut known = HashMap::new();
    for (item, owner) in &existing {
        if owner.is_empty() {
            known.insert(key(item), item);
        } else {
            known.entry(key(item)).or_insert(item);
        }
    }
    let mut seen = HashSet::new();
    let mut result = SyncResult::default();
    let timestamp = super::now_millis().to_string();
    for item in items {
        let identity = key(item);
        if !seen.insert(identity.clone()) {
            continue;
        }
        result.total += 1;
        if let Some(old) = known.get(&identity) {
            let description_is_generated = old.description.is_empty()
                || old.description == "同步目录"
                || old.description.starts_with("来自");
            if old.name == item.name
                && old.url == item.url
                && old.args == item.args
                && old.workdir == item.workdir
                && (!old.alias.is_empty() || item.alias.is_empty())
                && (!old.icon.is_empty() || item.icon.is_empty())
                && (!description_is_generated || old.description == item.description)
            {
                continue;
            }
            // Alias, enabled state and custom descriptions/icons remain user preferences.
            // Generated descriptions follow bookmark moves and directory/source changes.
            result.updated += tx
                .execute(
                    "UPDATE launch_items SET name=?1, url=?2, args=?3, workdir=?4,
                   alias=CASE WHEN alias='' THEN ?5 ELSE alias END,
                   description=CASE WHEN description='' OR description='同步目录'
                     OR description LIKE '来自%' THEN ?6 ELSE description END,
                   icon=CASE WHEN icon='' THEN ?7 ELSE icon END, updated_at=?8
                 WHERE id=?9 AND (name<>?1 OR url<>?2 OR args<>?3 OR workdir<>?4
                   OR (alias='' AND ?5<>'') OR (icon='' AND ?7<>'')
                   OR ((description='' OR description='同步目录' OR description LIKE '来自%')
                     AND description<>?6))",
                    params![
                        item.name,
                        item.url,
                        item.args,
                        item.workdir,
                        item.alias,
                        item.description,
                        item.icon,
                        timestamp,
                        old.id
                    ],
                )
                .map_err(|e| e.to_string())? as i32;
        } else {
            tx.execute(
                "INSERT INTO launch_items
                 (name,alias,type,url,args,workdir,description,icon,enabled,updated_at,sync_source)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,1,?9,?10)",
                params![
                    item.name,
                    item.alias,
                    item.kind,
                    item.url,
                    item.args,
                    item.workdir,
                    item.description,
                    item.icon,
                    timestamp,
                    source
                ],
            )
            .map_err(|e| e.to_string())?;
            result.added += 1;
        }
    }
    for (old, owner) in &existing {
        let identity = key(old);
        let stale = !seen.contains(&identity) && !retain_unseen(old);
        let duplicate = seen.contains(&identity) && known[&identity].id != old.id;
        if owner == source && (stale || duplicate) {
            result.removed += tx
                .execute("DELETE FROM launch_items WHERE id=?1", [old.id])
                .map_err(|e| e.to_string())? as i32;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        super::super::init_schema(&conn).unwrap();
        conn
    }

    fn item(kind: &str, url: &str) -> Item {
        Item {
            id: 0,
            name: "Example".into(),
            alias: String::new(),
            kind: kind.into(),
            url: url.into(),
            args: String::new(),
            workdir: String::new(),
            description: String::new(),
            icon: String::new(),
            enabled: true,
            updated_at: String::new(),
        }
    }

    #[test]
    fn all_sources_add_update_delete_and_empty_snapshots() {
        for (source, kind, url) in [
            ("apps", "application", "C:\\Example.exe"),
            ("bookmarks", "website", "https://example.com"),
            ("folders", "file", "C:\\example.txt"),
        ] {
            let conn = database();
            let mut current = item(kind, url);
            assert_eq!(
                reconcile(&conn, source, &[current.clone()], |_| false)
                    .unwrap()
                    .added,
                1
            );
            assert_eq!(
                reconcile(&conn, source, &[current.clone()], |_| false)
                    .unwrap()
                    .updated,
                0
            );
            current.description = "New description".into();
            assert_eq!(
                reconcile(&conn, source, &[current], |_| false)
                    .unwrap()
                    .updated,
                1
            );
            assert_eq!(reconcile(&conn, source, &[], |_| false).unwrap().removed, 1);
        }
    }

    #[test]
    fn manual_items_and_other_sources_survive() {
        let conn = database();
        conn.execute("INSERT INTO launch_items(name,type,url,updated_at) VALUES ('Manual','website','https://manual.test','now')", []).unwrap();
        reconcile(&conn, "apps", &[item("application", "C:\\app.exe")], |_| {
            false
        })
        .unwrap();
        reconcile(
            &conn,
            "bookmarks",
            &[item("website", "https://manual.test")],
            |_| false,
        )
        .unwrap();
        assert_eq!(
            reconcile(&conn, "bookmarks", &[], |_| false)
                .unwrap()
                .removed,
            0
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM launch_items", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
    }

    #[test]
    fn preferences_survive_and_duplicate_snapshots_are_idempotent() {
        let conn = database();
        let current = item("website", "https://example.test");
        assert_eq!(
            reconcile(
                &conn,
                "bookmarks",
                &[current.clone(), current.clone()],
                |_| false
            )
            .unwrap()
            .added,
            1
        );
        conn.execute(
            "UPDATE launch_items SET alias='custom', description='my note', enabled=0",
            [],
        )
        .unwrap();
        assert_eq!(
            reconcile(&conn, "bookmarks", &[current], |_| false)
                .unwrap()
                .updated,
            0
        );
        let saved = conn
            .query_row("SELECT * FROM launch_items", [], super::super::row_to_item)
            .unwrap();
        assert_eq!(saved.alias, "custom");
        assert_eq!(saved.description, "my note");
        assert!(!saved.enabled);
    }

    #[test]
    fn partial_scan_protects_only_unreadable_paths() {
        let conn = database();
        reconcile(
            &conn,
            "folders",
            &[
                item("file", "C:\\locked\\a.txt"),
                item("file", "C:\\deleted.txt"),
            ],
            |_| false,
        )
        .unwrap();
        let result = reconcile(&conn, "folders", &[], |old| old.url.contains("locked")).unwrap();
        assert_eq!(result.removed, 1);
        assert_eq!(
            conn.query_row("SELECT count(*) FROM launch_items", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn application_arguments_are_distinct() {
        let conn = database();
        let a = item("application", "C:\\app.exe");
        let mut b = a.clone();
        b.args = "--profile work".into();
        assert_eq!(
            reconcile(&conn, "apps", &[a.clone(), b], |_| false)
                .unwrap()
                .added,
            2
        );
        assert_eq!(
            reconcile(&conn, "apps", &[a], |_| false).unwrap().removed,
            1
        );
    }

    #[test]
    fn legacy_imports_are_migrated_once_and_can_be_removed() {
        let conn = database();
        conn.execute("ALTER TABLE launch_items DROP COLUMN sync_source", [])
            .unwrap();
        for (kind, url, description, updated) in [
            ("application", "C:\\removed.exe", "", ""),
            (
                "website",
                "https://removed.test",
                "来自Chrome Default收藏夹 · 书签栏",
                "123",
            ),
            ("file", "C:\\removed.txt", "来自下载", "123"),
            ("application", "C:\\manual.exe", "", "刚刚"),
        ] {
            conn.execute(
                "INSERT INTO launch_items(type,url,description,updated_at) VALUES (?1,?2,?3,?4)",
                params![kind, url, description, updated],
            )
            .unwrap();
        }
        init_schema(&conn).unwrap();
        for source in ["apps", "bookmarks", "folders"] {
            assert_eq!(reconcile(&conn, source, &[], |_| false).unwrap().removed, 1);
        }
        init_schema(&conn).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM launch_items WHERE sync_source=''",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }

    #[test]
    fn failed_commit_rolls_back_additions_and_deletions_together() {
        let conn = database();
        reconcile(&conn, "apps", &[item("application", "C:\\old.exe")], |_| {
            false
        })
        .unwrap();
        conn.execute_batch("CREATE TRIGGER refuse_delete BEFORE DELETE ON launch_items BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
        assert!(
            reconcile(&conn, "apps", &[item("application", "C:\\new.exe")], |_| {
                false
            })
            .is_err()
        );
        assert_eq!(
            conn.query_row("SELECT url FROM launch_items", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "C:\\old.exe"
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM launch_items", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn bookmark_moves_and_shared_urls_follow_the_remaining_profile() {
        use super::super::{collect_bookmarks, parse_bookmarks, synced_item};
        let conn = database();
        let scan = |sources: &[(&str, &str)]| {
            let mut items = Vec::new();
            for (source, name) in sources {
                let json = serde_json::json!({"roots": {"bookmark_bar": {"name": "Moved folder", "children": [
                    {"type": "url", "name": name, "url": "https://shared.test"}
                ]}}});
                let parsed = parse_bookmarks(&json.to_string()).unwrap();
                collect_bookmarks(&parsed, source, &HashMap::new(), &mut items);
            }
            reconcile(
                &conn,
                "bookmarks",
                &items.into_iter().map(synced_item).collect::<Vec<_>>(),
                |_| false,
            )
            .unwrap()
        };
        assert_eq!(
            scan(&[("Chrome Default", "First"), ("Edge Default", "Second")]).added,
            1
        );
        let moved = scan(&[("Edge Default", "Second")]);
        assert_eq!((moved.updated, moved.removed), (1, 0));
        let saved = conn
            .query_row("SELECT * FROM launch_items", [], super::super::row_to_item)
            .unwrap();
        assert_eq!(saved.name, "Second");
        assert!(saved.description.contains("Edge Default"));
        assert!(parse_bookmarks("{").is_err());
        assert!(parse_bookmarks("{}").is_err());
        assert!(parse_bookmarks(r#"{"roots":{"bookmark_bar":null}}"#).is_err());
        assert!(parse_bookmarks(r#"{"roots":{"bookmark_bar":{"children":[]}}}"#).is_ok());
        assert_eq!(scan(&[]).removed, 1);
    }
}

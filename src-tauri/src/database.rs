//! Keep user data in the stable app-data directory across installer upgrades.
use rusqlite::Connection;
use std::path::Path;

pub fn initialize(path: &Path) -> Result<(), String> {
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_secs(3))
        .map_err(|e| e.to_string())?;
    let columns = conn
        .prepare("PRAGMA table_info(launch_items)")
        .map_err(|e| e.to_string())?
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if !columns.is_empty() && !columns.iter().any(|name| name == "sync_source") {
        let directory = path.parent().ok_or("数据库目录无效")?.join("backups");
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let backup = directory.join("nexious-before-0.1.9.db");
        if !backup.try_exists().map_err(|e| e.to_string())? {
            // SQLite takes a consistent snapshot including committed WAL contents;
            // copying only nexious.db would lose recent changes still in the WAL.
            let pending = directory.join(format!(
                "nexious-before-0.1.9-{}-{}.pending",
                std::process::id(),
                super::now_millis()
            ));
            conn.execute("VACUUM INTO ?1", [pending.to_string_lossy().as_ref()])
                .map_err(|e| format!("升级前备份失败，原数据未迁移：{e}"))?;
            std::fs::rename(&pending, &backup)
                .map_err(|e| format!("保存升级备份失败，原数据未迁移：{e}"))?;
        }
    }
    super::init_schema(&conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::types::Value;

    fn snapshot(conn: &Connection) -> Vec<Vec<Vec<Value>>> {
        [
            "SELECT id,name,alias,type,url,description,icon,enabled,updated_at,args,workdir FROM launch_items ORDER BY id",
            "SELECT * FROM settings ORDER BY key",
            "SELECT * FROM search_history ORDER BY id",
            "SELECT * FROM sync_records ORDER BY id",
        ].into_iter().map(|sql| {
            let mut stmt = conn.prepare(sql).unwrap();
            let columns = stmt.column_count();
            stmt.query_map([], |row| (0..columns).map(|i| row.get(i)).collect())
                .unwrap().collect::<Result<Vec<_>, _>>().unwrap()
        }).collect()
    }

    #[test]
    fn upgrade_preserves_all_user_data_and_backs_up_wal() {
        let directory = std::env::temp_dir().join(format!(
            "nexious-upgrade-test-{}-{}",
            std::process::id(),
            super::super::now_millis()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("nexious.db");
        let old = Connection::open(&path).unwrap();
        super::super::init_schema(&old).unwrap();
        old.execute_batch("ALTER TABLE launch_items DROP COLUMN sync_source;
            PRAGMA journal_mode=WAL;
            PRAGMA wal_autocheckpoint=0;
            INSERT INTO launch_items (name,alias,type,url,description,icon,enabled,updated_at,args,workdir) VALUES
              ('Editor','code,编辑器','application','C:\\Editor.exe','custom note','custom icon',0,'','--profile work','C:\\Work'),
              ('Bookmark','my alias','website','https://example.test','来自Edge Default收藏夹 · 工作','website icon',1,'123','',''),
              ('Document','doc','file','C:\\Work\\doc.txt','来自Work','file icon',1,'456','',''),
              ('Manual folder','notes','folder','C:\\Notes','personal notes','folder icon',1,'刚刚','','');
            INSERT INTO settings VALUES ('shortcut','Ctrl+Alt+Space'),('theme','dark'),('autoSyncApps','false'),
              ('folderRoots','[]'),('opacity','77'),('lastSync','12345');
            INSERT INTO search_history(keyword,item_id,created_at) VALUES ('Editor',1,'789');
            INSERT INTO sync_records(added,updated,created_at) VALUES (4,2,'12345');").unwrap();
        let before = snapshot(&old);
        assert!(directory.join("nexious.db-wal").metadata().unwrap().len() > 0);
        initialize(&path).unwrap();
        assert_eq!(snapshot(&old), before);
        let backup_path = directory.join("backups/nexious-before-0.1.9.db");
        let backup = Connection::open(&backup_path).unwrap();
        assert_eq!(snapshot(&backup), before);
        assert_eq!(
            backup
                .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        drop(backup);
        let backup_bytes = std::fs::read(&backup_path).unwrap();
        initialize(&path).unwrap();
        assert_eq!(snapshot(&old), before);
        assert_eq!(std::fs::read(&backup_path).unwrap(), backup_bytes);
        drop(old);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn release_versions_and_data_identity_stay_consistent() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let package: serde_json::Value =
            serde_json::from_str(include_str!("../../package.json")).unwrap();
        assert_eq!(config["identifier"], "com.nexious.quick");
        assert_eq!(config["productName"], "Nexious Quick");
        assert_eq!(config["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(package["version"], env!("CARGO_PKG_VERSION"));
    }
}

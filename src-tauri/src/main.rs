#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tauri::menu::{MenuBuilder, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, LogicalSize, Manager};
use tauri_plugin_opener::OpenerExt;

use base64::Engine as _;

#[cfg(windows)]
#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(
        hwnd: *mut core::ffi::c_void,
        dw_attribute: u32,
        pv_attribute: *const u32,
        cb_attribute: u32,
    ) -> i32;
}

pub const MAIN_WINDOW: &str = "main";
pub const SETTINGS_WINDOW: &str = "settings";
pub const ITEMS_CHANGED_EVENT: &str = "nexious:items-changed";
pub const SETTINGS_CHANGED_EVENT: &str = "nexious:settings-changed";

#[derive(Serialize, Deserialize, Clone)]
pub struct Item {
    pub id: i64,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub alias: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub args: String,
    #[serde(default)]
    pub workdir: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: String,
}

fn default_true() -> bool {
    true
}

fn db_file(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("nexious.db"))
}

fn with_db<T>(
    app: &AppHandle,
    f: impl FnOnce(&Connection) -> Result<T, String>,
) -> Result<T, String> {
    let path = db_file(app)?;
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_millis(3000))
        .map_err(|e| e.to_string())?;
    // 大批量同步（如整盘目录）写入较多，WAL 模式可避免读写互相阻塞
    let _ = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;");
    f(&conn)
}

fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS launch_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL DEFAULT '',
            alias TEXT NOT NULL DEFAULT '',
            type TEXT NOT NULL DEFAULT 'application',
            url TEXT NOT NULL DEFAULT '',
            description TEXT NOT NULL DEFAULT '',
            icon TEXT NOT NULL DEFAULT '',
            enabled INTEGER NOT NULL DEFAULT 1,
            updated_at TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS search_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            keyword TEXT NOT NULL DEFAULT '',
            item_id INTEGER,
            created_at TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS sync_records (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            added INTEGER NOT NULL DEFAULT 0,
            updated INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT ''
        );",
    )
    .map_err(|e| e.to_string())?;
    // 旧库升级：补充新列（已存在时 SQLite 会报 duplicate column，直接忽略）
    for ddl in [
        "ALTER TABLE launch_items ADD COLUMN args TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE launch_items ADD COLUMN workdir TEXT NOT NULL DEFAULT ''",
    ] {
        let _ = conn.execute(ddl, []);
    }
    Ok(())
}

fn row_to_item(row: &rusqlite::Row) -> rusqlite::Result<Item> {
    Ok(Item {
        id: row.get("id")?,
        name: row.get("name")?,
        alias: row.get("alias")?,
        kind: row.get("type")?,
        url: row.get("url")?,
        args: row.get("args")?,
        workdir: row.get("workdir")?,
        description: row.get("description")?,
        icon: row.get("icon")?,
        enabled: row.get::<_, i64>("enabled")? != 0,
        updated_at: row.get("updated_at")?,
    })
}

fn items_where_clause(kind: &str) -> &'static str {
    match kind {
        "application" => "type='application'",
        "website" => "type='website'",
        "file" => "type='file'",
        "folder" => "type IN ('folder','file')",
        _ => "1=1",
    }
}

fn load_items_sync(app: &AppHandle) -> Result<Vec<Item>, String> {
    with_db(app, |conn| {
        init_schema(conn)?;
        // 启动器内存只保留少量应用/网站，海量文件与文件夹改为后端检索，
        // 避免整表（尤其整盘同步后可达数万条）载入 WebView 造成卡顿。
        let mut stmt = conn
            .prepare("SELECT * FROM launch_items WHERE type IN ('application','website') ORDER BY id DESC")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], row_to_item)
            .map_err(|e| e.to_string())?;
        let mut items = Vec::new();
        for r in rows {
            items.push(r.map_err(|e| e.to_string())?);
        }
        Ok(items)
    })
}

fn load_items_by_kind_sync(app: &AppHandle, kind: &str) -> Result<Vec<Item>, String> {
    with_db(app, |conn| {
        init_schema(conn)?;
        let where_sql = items_where_clause(kind);
        let sql = format!("SELECT * FROM launch_items WHERE {where_sql} ORDER BY id DESC");
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], row_to_item)
            .map_err(|e| e.to_string())?;
        let mut items = Vec::new();
        for r in rows {
            items.push(r.map_err(|e| e.to_string())?);
        }
        Ok(items)
    })
}

#[tauri::command]
async fn db_load_items(app: AppHandle) -> Result<Vec<Item>, String> {
    // 同步后整表刷新可能包含大量项（尤其整盘目录），放到后台线程执行避免阻塞 UI 主线程
    run_in_background(move || load_items_sync(&app)).await
}

/// 按类型加载启动项（application / website / folder / file），供设置页按需取数。
#[tauri::command]
async fn db_load_items_by_kind(app: AppHandle, kind: String) -> Result<Vec<Item>, String> {
    let k = if kind.trim().is_empty() {
        "all".to_string()
    } else {
        kind.trim().to_string()
    };
    run_in_background(move || load_items_by_kind_sync(&app, &k)).await
}

#[derive(Serialize)]
struct PageResult {
    items: Vec<Item>,
    total: i64,
}

/// 转义 LIKE 通配符，避免用户输入中的 % _ \ 影响检索。
fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn alias_list(item: &Item) -> Vec<String> {
    item.alias
        .split([',', '，'])
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

/// 与前端 scoreItem 保持一致的排序分，用于后端检索排序。
fn score_search_item(item: &Item, query: &str) -> i32 {
    let name = item.name.to_lowercase();
    if name == query {
        return 100;
    }
    let aliases = alias_list(item);
    if aliases.iter().any(|a| a == query) {
        return 90;
    }
    if name.starts_with(query) {
        return 80;
    }
    if aliases.iter().any(|a| a.starts_with(query)) {
        return 75;
    }
    if name.contains(query) {
        return 70;
    }
    if aliases.iter().any(|a| a.contains(query)) {
        return 60;
    }
    if item.url.to_lowercase().contains(query) {
        return 50;
    }
    if item.description.to_lowercase().contains(query) {
        return 40;
    }
    0
}

/// 搜索匹配优先级：应用 > 网站链接 > 文件/文件夹（同类型内再按相关度排序）
fn type_priority(kind: &str) -> i32 {
    match kind {
        "application" => 0,
        "website" => 1,
        _ => 2,
    }
}

fn allowed_kinds_clause(website_on: bool, folder_on: bool) -> String {
    let mut clause = String::from("type='application'");
    if website_on {
        clause.push_str(" OR type='website'");
    }
    if folder_on {
        clause.push_str(" OR type IN ('folder','file')");
    }
    clause
}

fn search_items_sync(
    app: &AppHandle,
    q: &str,
    website_on: bool,
    folder_on: bool,
    limit: usize,
) -> Result<Vec<Item>, String> {
    let query = q.trim().to_lowercase();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let pattern = format!("%{}%", escape_like(&query));
    with_db(app, |conn| {
        init_schema(conn)?;
        let allowed = allowed_kinds_clause(website_on, folder_on);
        let sql = format!(
            "SELECT * FROM launch_items WHERE enabled=1 AND ({allowed}) AND \
             (name LIKE ?1 ESCAPE '\\' OR alias LIKE ?1 ESCAPE '\\' \
              OR url LIKE ?1 ESCAPE '\\' OR description LIKE ?1 ESCAPE '\\')"
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![pattern], row_to_item)
            .map_err(|e| e.to_string())?;
        let mut scored: Vec<(i32, i32, usize, Item)> = Vec::new();
        for row in rows {
            let item = row.map_err(|e| e.to_string())?;
            let priority = type_priority(&item.kind);
            let score = score_search_item(&item, &query);
            scored.push((priority, score, item.name.len(), item));
        }
        scored.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        scored.truncate(limit.max(1));
        Ok(scored.into_iter().map(|(_, _, _, item)| item).collect())
    })
}

fn page_items_sync(
    app: &AppHandle,
    kind: &str,
    keyword: &str,
    offset: i64,
    limit: i64,
) -> Result<PageResult, String> {
    let where_type = items_where_clause(kind);
    let kw = keyword.trim();
    let (filter_sql, like) = if kw.is_empty() {
        (String::new(), String::new())
    } else {
        (
            " AND (name LIKE ?1 ESCAPE '\\' OR alias LIKE ?1 ESCAPE '\\' \
              OR url LIKE ?1 ESCAPE '\\' OR description LIKE ?1 ESCAPE '\\')"
                .to_string(),
            format!("%{}%", escape_like(&kw.to_lowercase())),
        )
    };
    let count_sql = format!("SELECT COUNT(*) FROM launch_items WHERE {where_type}{filter_sql}");
    let page_sql = format!(
        "SELECT * FROM launch_items WHERE {where_type}{filter_sql} \
         ORDER BY id DESC LIMIT {limit} OFFSET {offset}"
    );
    with_db(app, |conn| {
        init_schema(conn)?;
        let total = if like.is_empty() {
            conn.query_row(&count_sql, [], |row| row.get::<_, i64>(0))
                .map_err(|e| e.to_string())?
        } else {
            conn.query_row(&count_sql, rusqlite::params![like], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|e| e.to_string())?
        };
        let items = if like.is_empty() {
            let mut stmt = conn.prepare(&page_sql).map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], row_to_item)
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
        } else {
            let mut stmt = conn.prepare(&page_sql).map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(rusqlite::params![like], row_to_item)
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
        };
        Ok(PageResult { items, total })
    })
}

/// 启动器搜索：只在数据库里完成过滤与排序，返回少量命中项，避免把整表载入内存。
#[tauri::command]
async fn search_items(
    app: AppHandle,
    q: String,
    website_on: bool,
    folder_on: bool,
    limit: Option<usize>,
) -> Result<Vec<Item>, String> {
    let limit = limit.unwrap_or(6).clamp(1, 20);
    run_in_background(move || search_items_sync(&app, &q, website_on, folder_on, limit)).await
}

/// 设置页分页取数：每页只回传少量条目，避免把数万条文件记录一次性发给前端。
#[tauri::command]
async fn db_page_items(
    app: AppHandle,
    kind: String,
    keyword: String,
    offset: i64,
    limit: i64,
) -> Result<PageResult, String> {
    let kind = if kind.trim().is_empty() {
        "all".to_string()
    } else {
        kind.trim().to_string()
    };
    let offset = offset.max(0);
    let limit = limit.clamp(1, 5000);
    run_in_background(move || page_items_sync(&app, &kind, &keyword, offset, limit)).await
}

fn existing_duplicate_id(conn: &Connection, item: &Item) -> Result<Option<i64>, String> {
    let Some(key) = item_dedupe_key(&item.kind, &item.name, &item.url, &item.args) else {
        return Ok(None);
    };
    let mut stmt = conn
        .prepare("SELECT id, name, type, url, args FROM launch_items")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (id, name, kind, url, args) = row.map_err(|e| e.to_string())?;
        if id == item.id {
            continue;
        }
        if let Some(other) = item_dedupe_key(&kind, &name, &url, &args) {
            if other == key {
                return Ok(Some(id));
            }
        }
    }
    Ok(None)
}

#[tauri::command]
fn db_save_item(app: AppHandle, item: Item) -> Result<Item, String> {
    with_db(&app, |conn| {
        init_schema(conn)?;
        let enabled = if item.enabled { 1 } else { 0 };
        if item.id > 0 {
            conn.execute(
                "UPDATE launch_items SET name=?1, alias=?2, type=?3, url=?4, args=?5, workdir=?6,
                 description=?7, icon=?8, enabled=?9, updated_at=?10 WHERE id=?11",
                rusqlite::params![
                    item.name,
                    item.alias,
                    item.kind,
                    item.url,
                    item.args,
                    item.workdir,
                    item.description,
                    item.icon,
                    enabled,
                    item.updated_at,
                    item.id
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(item)
        } else {
            // 与启动数据去重规则一致：不允许新增重复的数据项
            if existing_duplicate_id(conn, &item)?.is_some() {
                return Err("该数据项已存在（相同路径/网址/名称），不能重复添加".into());
            }
            conn.execute(
                "INSERT INTO launch_items (name, alias, type, url, args, workdir, description, icon, enabled, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                rusqlite::params![
                    item.name,
                    item.alias,
                    item.kind,
                    item.url,
                    item.args,
                    item.workdir,
                    item.description,
                    item.icon,
                    enabled,
                    item.updated_at
                ],
            )
            .map_err(|e| e.to_string())?;
            let id = conn.last_insert_rowid();
            let mut saved = item;
            saved.id = id;
            Ok(saved)
        }
    })
}

#[tauri::command]
fn db_delete_item(app: AppHandle, id: i64) -> Result<(), String> {
    with_db(&app, |conn| {
        init_schema(conn)?;
        conn.execute("DELETE FROM launch_items WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        Ok(())
    })
}

/// 生成条目的去重键：
/// - 文件/文件夹：按「类型 + 路径 + 名称」判断重复；
/// - 网站链接：按规范化后的 URL（小写、去尾部斜杠）判断；
/// - 应用：按「路径 + 启动参数」判断。
fn item_dedupe_key(kind: &str, name: &str, url: &str, args: &str) -> Option<String> {
    let lower = |s: &str| s.to_lowercase();
    match kind {
        "folder" | "file" => {
            let name = name.trim();
            let url = url.trim();
            if name.is_empty() || url.is_empty() {
                return None;
            }
            Some(format!("{kind}|{}|{}", lower(name), lower(url)))
        }
        "website" => {
            let url = lower(url.trim_end_matches('/'));
            if url.is_empty() {
                return None;
            }
            Some(format!("website|{url}"))
        }
        "application" => {
            let url = url.trim();
            if url.is_empty() {
                return None;
            }
            Some(format!("application|{}|{}", lower(url), lower(args.trim())))
        }
        _ => None,
    }
}

/// 持久化去重：同一条目只保留信息最完整的一条（别名/描述/图标更全者优先）。
/// 返回删除的行数。
fn dedupe_items(app: &AppHandle) -> Result<usize, String> {
    with_db(app, |conn| {
        init_schema(conn)?;
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let mut stmt = tx
            .prepare("SELECT id, name, type, url, args, alias, description, icon FROM launch_items ORDER BY id")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        // key -> (信息完整度, 保留的 id)
        let mut best: HashMap<String, (usize, i64)> = HashMap::new();
        let mut to_delete: Vec<i64> = Vec::new();
        for row in rows {
            let (id, name, kind, url, args, alias, description, icon) = row.map_err(|e| e.to_string())?;
            let Some(key) = item_dedupe_key(&kind, &name, &url, &args) else {
                continue;
            };
            let richness = [alias.trim(), description.trim(), icon.trim()]
                .iter()
                .filter(|value| !value.is_empty())
                .count();
            match best.get(&key) {
                Some((best_rich, _)) if richness <= *best_rich => {
                    to_delete.push(id);
                }
                Some((_, best_id)) => {
                    to_delete.push(*best_id);
                    best.insert(key, (richness, id));
                }
                None => {
                    best.insert(key, (richness, id));
                }
            }
        }
        drop(stmt);
        let deleted = to_delete.len();
        if !to_delete.is_empty() {
            let mut del = tx
                .prepare("DELETE FROM launch_items WHERE id=?1")
                .map_err(|e| e.to_string())?;
            for id in to_delete {
                del.execute([id]).map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(deleted)
    })
}

#[tauri::command]
fn db_load_settings(app: AppHandle) -> Result<HashMap<String, String>, String> {
    with_db(&app, |conn| {
        init_schema(conn)?;
        let mut stmt = conn
            .prepare("SELECT key, value FROM settings")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        let mut map = HashMap::new();
        for r in rows {
            let (k, v) = r.map_err(|e| e.to_string())?;
            map.insert(k, v);
        }
        Ok(map)
    })
}

#[tauri::command]
fn db_save_settings(app: AppHandle, settings: HashMap<String, String>) -> Result<(), String> {
    with_db(&app, |conn| {
        init_schema(conn)?;
        for (k, v) in &settings {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value=?2",
                [k, v],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    })?;
    let _ = app.emit(SETTINGS_CHANGED_EVENT, settings);
    Ok(())
}

#[tauri::command]
fn get_setting(app: AppHandle, key: String) -> Result<Option<String>, String> {
    with_db(&app, |conn| {
        init_schema(conn)?;
        let mut stmt = conn
            .prepare("SELECT value FROM settings WHERE key=?1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([key]).map_err(|e| e.to_string())?;
        if let Ok(Some(row)) = rows.next() {
            return Ok(Some(row.get::<_, String>(0).map_err(|e| e.to_string())?));
        }
        Ok(None)
    })
}

fn set_setting_sync(app: &AppHandle, key: &str, value: &str) -> Result<(), String> {
    with_db(app, |conn| {
        init_schema(conn)?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=?2",
            [key, value],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

/// 读取布尔设置项；数据库中不存在时返回默认值。
fn read_bool_setting(app: &AppHandle, key: &str, default: bool) -> bool {
    with_db(app, |conn| {
        init_schema(conn)?;
        let mut stmt = conn
            .prepare("SELECT value FROM settings WHERE key=?1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([key]).map_err(|e| e.to_string())?;
        if let Ok(Some(row)) = rows.next() {
            let value: String = row.get(0).map_err(|e| e.to_string())?;
            return Ok(value == "true");
        }
        Ok(default)
    })
    .unwrap_or(default)
}

const AUTOSTART_VALUE: &str = "NexiousQuick";
const AUTOSTART_FLAG: &str = "--autostart";

#[cfg(windows)]
fn autostart_command(enabled: bool) -> Result<bool, String> {
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    if enabled {
        let exe = std::env::current_exe().map_err(|e| err_msg("获取程序路径失败", e))?;
        let value = format!("\"{}\" {}", exe.display(), AUTOSTART_FLAG);
        let output = reg_output(&[
            "add",
            key,
            "/v",
            AUTOSTART_VALUE,
            "/t",
            "REG_SZ",
            "/d",
            value.as_str(),
            "/f",
        ])
        .map_err(|e| err_msg("开启开机自启动失败", e))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Ok(true)
    } else {
        let output = reg_output(&["delete", key, "/v", AUTOSTART_VALUE, "/f"])
            .map_err(|e| err_msg("关闭开机自启动失败", e))?;
        Ok(output.status.success() || output.status.code() == Some(1))
    }
}

#[cfg(not(windows))]
fn autostart_command(enabled: bool) -> Result<bool, String> {
    if enabled { Err("当前系统暂不支持开机自启动".into()) } else { Ok(false) }
}

#[cfg(windows)]
fn autostart_state() -> Result<bool, String> {
    let output = reg_output(&[
        "query",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
        "/v",
        AUTOSTART_VALUE,
    ])
    .map_err(|e| err_msg("读取开机自启动状态失败", e))?;
    Ok(output.status.success())
}

#[cfg(not(windows))]
fn autostart_state() -> Result<bool, String> { Ok(false) }

/// 旧版本注册的开机自启命令行没有 --autostart 标记，检测到后自动补全，
/// 保证升级后的老用户也能以“后台静默启动”方式自启。
#[cfg(windows)]
fn ensure_autostart_flag() {
    let Ok(exe) = std::env::current_exe() else { return };
    let exe_lower = exe.display().to_string().to_lowercase();
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let Ok(output) = reg_output(&["query", key, "/v", AUTOSTART_VALUE]) else {
        return;
    };
    if !output.status.success() {
        return;
    }
    let text = String::from_utf8_lossy(&output.stdout).to_lowercase();
    if text.contains(exe_lower.as_str()) && !text.contains(AUTOSTART_FLAG) {
        let _ = autostart_command(true);
    }
}

#[cfg(not(windows))]
fn ensure_autostart_flag() {}

/// 是否由开机自启动（--autostart）拉起，用于决定是否后台静默启动。
fn is_autostart_launch() -> bool {
    std::env::args().any(|arg| arg == AUTOSTART_FLAG)
}

fn apply_autostart_state(app: &AppHandle, enabled: Option<bool>) -> Result<bool, String> {
    let actual = match enabled {
        Some(v) => autostart_command(v)?,
        None => autostart_state()?,
    };
    set_setting_sync(app, "autoStart", &actual.to_string())?;
    let mut patch = HashMap::new();
    patch.insert("autoStart".to_string(), actual.to_string());
    let _ = app.emit(SETTINGS_CHANGED_EVENT, patch);
    Ok(actual)
}

#[tauri::command]
async fn get_autostart(app: AppHandle) -> Result<bool, String> {
    // 读注册表会启动外部 reg.exe，放后台线程避免阻塞 IPC
    run_in_background(move || apply_autostart_state(&app, None)).await
}

#[tauri::command]
async fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    // 写注册表会启动外部 reg.exe，放后台线程避免阻塞 IPC
    run_in_background(move || apply_autostart_state(&app, Some(enabled))).await
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[tauri::command]
fn get_user_dirs(app: AppHandle) -> Result<HashMap<String, String>, String> {
    let mut map = HashMap::new();
    if let Some(home) = dirs_home(&app) {
        for key in ["Desktop", "Documents", "Downloads", "Pictures"] {
            map.insert(key.to_lowercase(), home.join(key).to_string_lossy().to_string());
        }
    }
    Ok(map)
}

fn dirs_home(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(home) = std::env::var("USERPROFILE") {
        return Some(PathBuf::from(home));
    }
    app.path().home_dir().ok()
}

#[tauri::command]
fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    let target = if url.contains("://") || url.starts_with("shell:") {
        url
    } else {
        format!("https://{}", url.trim_start_matches('/'))
    };
    app.opener()
        .open_url(target, None::<&str>)
        .map_err(|e| err_msg("打开网站失败", e))
}

#[derive(Serialize)]
struct ReleaseCheck {
    found: bool,
    url: String,
}

/// 检查 GitHub 最新 Release：请求网页版 releases/latest 跟随跳转拿到最终标签页地址。
/// 走网页端点而非 api.github.com，可避免匿名请求触发 API 限流（HTTP 403）。
#[tauri::command]
async fn check_latest_release() -> Result<ReleaseCheck, String> {
    // 网络请求放到后台线程执行，避免阻塞 IPC 导致界面卡顿
    run_in_background(check_latest_release_inner).await
}

fn check_latest_release_inner() -> Result<ReleaseCheck, String> {
    const RELEASES_URL: &str = "https://github.com/Nexius-Nova/nexious-quick/releases/latest";
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$url = '{url}'
$out = ''
try {
    $resp = Invoke-WebRequest -UseBasicParsing -Uri $url -TimeoutSec 12 -MaximumRedirection 5
    if ($resp.BaseResponse.ResponseUri) {
        $out = $resp.BaseResponse.ResponseUri.AbsoluteUri
    }
} catch {
    if ($_.Exception.Response) {
        $status = [int]$_.Exception.Response.StatusCode
        if ($status -eq 404) { $out = '__NOT_FOUND__' }
    }
}
[Console]::OutputEncoding = [System.Text.Encoding]::ASCII
[Console]::Write($out)
"#;
    let script = script.replace("{url}", RELEASES_URL);
    let mut cmd = std::process::Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &script,
    ]);
    hide_console(&mut cmd);
    let output = cmd
        .output()
        .map_err(|e| err_msg("检查更新失败：无法启动 PowerShell", e))?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text == "__NOT_FOUND__" {
        return Ok(ReleaseCheck { found: false, url: String::new() });
    }
    if text.starts_with("https://") {
        return Ok(ReleaseCheck { found: true, url: text });
    }
    Err("检查更新失败：无法连接 GitHub，请检查网络后重试".into())
}

fn err_msg(cn: &str, e: impl std::fmt::Display) -> String {
    format!("{cn}：{e}")
}

fn record_history(app: &AppHandle, keyword: &str, item_id: i64) {
    let _ = with_db(app, |conn| {
        init_schema(conn)?;
        conn.execute(
            "INSERT INTO search_history (keyword, item_id, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![keyword, item_id, now_millis().to_string()],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    });
}

#[tauri::command]
fn open_item(app: AppHandle, item: Item, keyword: String) -> Result<(), String> {
    let res = launch(&app, &item);
    if res.is_ok() {
        record_history(&app, &keyword, item.id);
    }
    res
}

// ---------- 已运行应用的窗口激活（Windows） ----------

/// 启动应用前先检查是否已在运行：已运行则激活其已有窗口，避免重复打开。
#[cfg(windows)]
mod running_app {
    use std::path::Path;

    type Handle = *mut core::ffi::c_void;

    const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const MAX_PATH: usize = 260;
    const GW_OWNER: u32 = 4;
    const SW_RESTORE: i32 = 9;

    #[repr(C)]
    struct ProcessEntry32W {
        dw_size: u32,
        cnt_usage: u32,
        th32_process_id: u32,
        th32_default_heap_id: usize,
        th32_module_id: u32,
        cnt_threads: u32,
        th32_parent_process_id: u32,
        pc_pri_class_base: i32,
        dw_flags: u32,
        sz_exe_file: [u16; MAX_PATH],
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> Handle;
        fn Process32FirstW(snapshot: Handle, entry: *mut ProcessEntry32W) -> i32;
        fn Process32NextW(snapshot: Handle, entry: *mut ProcessEntry32W) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
        fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> Handle;
        fn QueryFullProcessImageNameW(
            process: Handle,
            flags: u32,
            exe_name: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn GetCurrentThreadId() -> u32;
    }

    #[link(name = "user32")]
    extern "system" {
        fn EnumWindows(callback: extern "system" fn(Handle, isize) -> i32, param: isize) -> i32;
        fn IsWindowVisible(hwnd: Handle) -> i32;
        fn GetWindow(hwnd: Handle, cmd: u32) -> Handle;
        fn GetWindowThreadProcessId(hwnd: Handle, process_id: *mut u32) -> u32;
        fn ShowWindow(hwnd: Handle, cmd: i32) -> i32;
        fn SetForegroundWindow(hwnd: Handle) -> i32;
        fn BringWindowToTop(hwnd: Handle) -> i32;
        fn AttachThreadInput(attach: u32, attach_to: u32, enable: i32) -> i32;
        fn GetForegroundWindow() -> Handle;
    }

    /// 读取进程对应的可执行文件完整路径（无权限时返回 None）
    fn process_image_path(process_id: u32) -> Option<String> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
        if handle.is_null() {
            return None;
        }
        let mut buffer = vec![0u16; 1024];
        let mut size = buffer.len() as u32;
        let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size) };
        unsafe { CloseHandle(handle) };
        if ok == 0 || size == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buffer[..size as usize]))
    }

    /// Windows 的 canonicalize 会带 `\\?\` 前缀，需要还原成普通路径再比较
    fn normalize_path(path: &Path) -> String {
        let text = path.to_string_lossy().replace(r"\\?\UNC\", r"\\");
        let trimmed: Option<String> = text.strip_prefix(r"\\?\").map(|rest| rest.to_string());
        trimmed.unwrap_or(text).to_lowercase()
    }

    /// 按可执行文件完整路径查找正在运行的进程
    fn find_process(target: &Path) -> Option<u32> {
        let canonical = std::fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
        let expected = normalize_path(&canonical);
        let target_name = canonical
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if target_name.is_empty() {
            return None;
        }
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot as isize == -1 || snapshot.is_null() {
            return None;
        }
        let mut entry: ProcessEntry32W = unsafe { std::mem::zeroed() };
        entry.dw_size = std::mem::size_of::<ProcessEntry32W>() as u32;
        let mut found = None;
        let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) };
        while ok != 0 {
            let process_id = entry.th32_process_id;
            let name_end = entry
                .sz_exe_file
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(MAX_PATH);
            let name = String::from_utf16_lossy(&entry.sz_exe_file[..name_end]).to_lowercase();
            // 先按进程名粗筛，命中后再核对完整路径，避免同名的其它目录程序误判
            if name == target_name {
                if let Some(image) = process_image_path(process_id) {
                    if image.to_lowercase() == expected {
                        found = Some(process_id);
                        break;
                    }
                }
            }
            ok = unsafe { Process32NextW(snapshot, &mut entry) };
        }
        unsafe { CloseHandle(snapshot) };
        found
    }

    struct WindowSearch {
        process_id: u32,
        hwnd: Handle,
        owner_free_only: bool,
    }

    extern "system" fn pick_window(hwnd: Handle, param: isize) -> i32 {
        let out = unsafe { &mut *(param as *mut WindowSearch) };
        if unsafe { IsWindowVisible(hwnd) } == 0 {
            return 1;
        }
        // 优先选择无宿主的应用主窗口，跳过托盘/工具窗口
        if out.owner_free_only && (unsafe { GetWindow(hwnd, GW_OWNER) }) as isize != 0 {
            return 1;
        }
        let mut process_id = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, &mut process_id) };
        if process_id != out.process_id {
            return 1;
        }
        out.hwnd = hwnd;
        0
    }

    /// 把已运行进程的主窗口还原并置于前台
    fn activate(process_id: u32) -> bool {
        let mut search = WindowSearch {
            process_id,
            hwnd: std::ptr::null_mut(),
            owner_free_only: true,
        };
        unsafe { EnumWindows(pick_window, &mut search as *mut WindowSearch as isize) };
        // 个别程序主窗口带宿主，找不到无宿主窗口时放宽条件再找一次
        if search.hwnd.is_null() {
            search.owner_free_only = false;
            unsafe { EnumWindows(pick_window, &mut search as *mut WindowSearch as isize) };
        }
        if search.hwnd.is_null() {
            return false;
        }
        unsafe {
            ShowWindow(search.hwnd, SW_RESTORE);
            let current = GetCurrentThreadId();
            let foreground = GetForegroundWindow();
            let foreground_thread = if foreground.is_null() {
                0
            } else {
                GetWindowThreadProcessId(foreground, std::ptr::null_mut())
            };
            // SetForegroundWindow 受前台锁定限制，附加到前台线程后再调用可稳定生效
            if foreground_thread != 0 && foreground_thread != current {
                AttachThreadInput(current, foreground_thread, 1);
            }
            BringWindowToTop(search.hwnd);
            // 前台锁定等原因可能让 SetForegroundWindow 返回 0，此时窗口也已还原到桌面，
            // 只要找到主窗口就视为已激活，避免再启动一个重复实例。
            SetForegroundWindow(search.hwnd);
            if foreground_thread != 0 && foreground_thread != current {
                AttachThreadInput(current, foreground_thread, 0);
            }
            true
        }
    }

    /// 目标程序已在运行时激活其窗口并返回 true（调用方据此跳过重复启动）
    pub fn activate_if_running(path: &Path) -> bool {
        match find_process(path) {
            // 找到进程但定位不到窗口（如纯托盘程序）时返回 false，交由调用方正常启动
            Some(process_id) => activate(process_id),
            None => false,
        }
    }
}

#[cfg(not(windows))]
mod running_app {
    use std::path::Path;

    pub fn activate_if_running(_path: &Path) -> bool {
        false
    }
}

fn launch(app: &AppHandle, item: &Item) -> Result<(), String> {
    let url = item.url.trim().to_string();
    if url.is_empty() {
        return Err("启动项路径为空".into());
    }
    // 网站：交给系统默认浏览器
    if item.kind == "website" || url.starts_with("http://") || url.starts_with("https://") {
        return app
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|e| err_msg("打开网站失败", e));
    }
    // UWP 应用：shell:AppsFolder\<AUMID>
    if let Some(aumid) = url.strip_prefix("shell:AppsFolder\\") {
        if !aumid.is_empty() {
            return spawn_command("explorer.exe", &[&format!("shell:AppsFolder\\{aumid}")], None);
        }
        return Err("无效的应用标识".into());
    }
    // 特殊启动方式：shell 命名空间（::{CLSID}）与自定义协议（如 steam://），交给系统默认方式打开
    if url.starts_with("::{")
        || (url.contains("://") && !url.starts_with("http://") && !url.starts_with("https://"))
    {
        return shell_open::open(&url).map_err(|e| err_msg("启动失败", e));
    }
    let path = PathBuf::from(&url);
    if path.is_dir() {
        return app
            .opener()
            .open_path(url, None::<&str>)
            .map_err(|e| err_msg("打开文件夹失败", e));
    }
    if !path.exists() {
        return Err(format!("路径不存在：{url}"));
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "lnk" => spawn_command("explorer.exe", &[url.as_str()], None),
        "exe" => {
            let args = split_args(&item.args);
            // 未附加启动参数时：目标程序已在运行则直接激活其窗口，不再启动第二个实例
            if args.is_empty() && running_app::activate_if_running(&path) {
                return Ok(());
            }
            let workdir = if item.workdir.trim().is_empty() {
                path.parent().map(|p| p.to_path_buf())
            } else {
                Some(PathBuf::from(item.workdir.trim()))
            };
            let mut cmd = std::process::Command::new(&path);
            cmd.args(&args);
            if let Some(dir) = workdir.as_deref() {
                cmd.current_dir(dir);
            }
            // 控制台程序（cmd.exe / powershell.exe 等）必须保留控制台窗口，否则用户点了看不到任何界面
            if !is_console_app(&path) {
                hide_console(&mut cmd);
            }
            match cmd.spawn() {
                Ok(_) => Ok(()),
                // 程序清单要求管理员权限（ERROR_ELEVATION_REQUIRED=740）：自动弹 UAC 授权并以管理员身份启动
                Err(e) if elevated_launch::is_elevation_required(&e) => {
                    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                    elevated_launch::spawn_elevated(&path, &arg_refs, workdir.as_deref())
                        .map_err(|m| err_msg("启动应用失败", m))
                }
                Err(e) => Err(err_msg("启动应用失败", e)),
            }
        }
        "bat" | "cmd" => {
            let mut cmd = std::process::Command::new("cmd");
            cmd.args(["/C", "start", "", &url]);
            hide_console(&mut cmd);
            cmd.spawn().map(|_| ()).map_err(|e| err_msg("启动失败", e))
        }
        _ => app
            .opener()
            .open_path(url, None::<&str>)
            .map_err(|e| err_msg("打开文件失败", e)),
    }
}

fn spawn_command(program: &str, args: &[&str], dir: Option<PathBuf>) -> Result<(), String> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    hide_console(&mut cmd);
    cmd.spawn().map(|_| ()).map_err(|e| err_msg("启动失败", e))
}

#[cfg(windows)]
fn hide_console(cmd: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_cmd: &mut std::process::Command) {}

/// 判断可执行文件是否为控制台程序（PE 可选头 Subsystem = IMAGE_SUBSYSTEM_WINDOWS_CUI）。
/// 控制台程序不能带 CREATE_NO_WINDOW 启动，否则窗口不可见、看起来像“点了没反应”。
#[cfg(windows)]
fn is_console_app(path: &std::path::Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};

    const IMAGE_SUBSYSTEM_WINDOWS_CUI: u16 = 3;

    fn read_at(file: &mut std::fs::File, offset: u64, len: usize) -> Option<Vec<u8>> {
        file.seek(SeekFrom::Start(offset)).ok()?;
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf).ok()?;
        Some(buf)
    }

    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let Some(head) = read_at(&mut file, 0, 0x40) else {
        return false;
    };
    if head[0..2] != *b"MZ" {
        return false;
    }
    let pe_offset = u32::from_le_bytes([head[0x3c], head[0x3d], head[0x3e], head[0x3f]]) as u64;
    let Some(signature) = read_at(&mut file, pe_offset, 4) else {
        return false;
    };
    if signature != *b"PE\0\0" {
        return false;
    }
    // PE 签名(4) + COFF 文件头(20) 之后是可选头，Subsystem 位于可选头偏移 68（PE32 与 PE32+ 一致）
    let Some(subsystem) = read_at(&mut file, pe_offset + 24 + 68, 2) else {
        return false;
    };
    u16::from_le_bytes([subsystem[0], subsystem[1]]) == IMAGE_SUBSYSTEM_WINDOWS_CUI
}

#[cfg(not(windows))]
fn is_console_app(_path: &std::path::Path) -> bool {
    false
}

/// 解析启动项参数：Windows 下按系统规则切分，保留引号内的空格，
/// 避免 `cmd /k "C:\xx\xx.bat"`、PowerShell `-c "..."` 这类参数被按空格拆坏。
#[cfg(windows)]
fn split_args(raw: &str) -> Vec<String> {
    use std::ffi::{OsStr, OsString};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    type Handle = *mut core::ffi::c_void;

    #[link(name = "shell32")]
    extern "system" {
        fn CommandLineToArgvW(cmd_line: *const u16, num_args: *mut i32) -> *mut *mut u16;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(ptr: Handle) -> Handle;
    }

    if raw.trim().is_empty() {
        return Vec::new();
    }
    // CommandLineToArgvW 会把第一个 token 当作程序名，这里补一个占位名再丢弃
    let line: Vec<u16> = OsStr::new(&format!("x {raw}"))
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut count = 0i32;
    let argv = unsafe { CommandLineToArgvW(line.as_ptr(), &mut count) };
    if argv.is_null() || count <= 1 {
        return raw.split_whitespace().map(|s| s.to_string()).collect();
    }
    let mut out = Vec::new();
    unsafe {
        for i in 1..count as isize {
            let ptr = *argv.offset(i);
            if ptr.is_null() {
                continue;
            }
            let mut len = 0usize;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            let arg = OsString::from_wide(std::slice::from_raw_parts(ptr, len));
            out.push(arg.to_string_lossy().to_string());
        }
        LocalFree(argv as Handle);
    }
    out
}

#[cfg(not(windows))]
fn split_args(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(|s| s.to_string()).collect()
}

// ---------- 管理员权限（UAC）启动 ----------

/// 启动带有“需要管理员权限”清单的程序：普通 CreateProcess 会返回
/// ERROR_ELEVATION_REQUIRED(740)。此时改用 ShellExecuteW 的 runas 动词，
/// 让系统弹出 UAC 授权框后以管理员身份启动该程序。
/// 用系统默认方式打开非文件目标：shell 命名空间（`::{CLSID}`）与自定义协议（如 `steam://`）。
#[cfg(windows)]
mod shell_open {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            hwnd: *mut std::ffi::c_void,
            lp_operation: *const u16,
            lp_file: *const u16,
            lp_parameters: *const u16,
            lp_directory: *const u16,
            n_show_cmd: i32,
        ) -> isize;
    }

    pub fn open(target: &str) -> Result<(), String> {
        let file: Vec<u16> = OsStr::new(target)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let operation: Vec<u16> = OsStr::new("open")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1, // SW_SHOWNORMAL
            )
        };
        if result > 32 {
            return Ok(());
        }
        Err(format!("系统无法打开该目标（错误码 {}）", result as u32))
    }
}

#[cfg(not(windows))]
mod shell_open {
    pub fn open(_target: &str) -> Result<(), String> {
        Err("当前平台不支持该启动方式".into())
    }
}

#[cfg(windows)]
mod elevated_launch {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    const ERROR_ELEVATION_REQUIRED: i32 = 740;

    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            hwnd: *mut std::ffi::c_void,
            lp_operation: *const u16,
            lp_file: *const u16,
            lp_parameters: *const u16,
            lp_directory: *const u16,
            n_show_cmd: i32,
        ) -> isize;
    }

    pub fn is_elevation_required(e: &std::io::Error) -> bool {
        e.raw_os_error() == Some(ERROR_ELEVATION_REQUIRED)
    }

    fn wide(s: &OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    /// 按 Windows 命令行规则编码单个参数（与 std::process::Command 的编码方式保持一致）。
    fn append_arg(out: &mut String, arg: &str) {
        let needs_quotes =
            arg.is_empty() || arg.chars().any(|c| c == ' ' || c == '\t' || c == '"');
        if !needs_quotes {
            out.push_str(arg);
            return;
        }
        out.push('"');
        let mut backslashes = 0usize;
        for c in arg.chars() {
            match c {
                '\\' => backslashes += 1,
                '"' => {
                    for _ in 0..backslashes * 2 {
                        out.push('\\');
                    }
                    backslashes = 0;
                    out.push('\\');
                    out.push('"');
                }
                _ => {
                    for _ in 0..backslashes {
                        out.push('\\');
                    }
                    backslashes = 0;
                    out.push(c);
                }
            }
        }
        for _ in 0..backslashes * 2 {
            out.push('\\');
        }
        out.push('"');
    }

    pub fn spawn_elevated(
        path: &Path,
        args: &[&str],
        workdir: Option<&Path>,
    ) -> Result<(), String> {
        let file = wide(path.as_os_str());
        let mut params = String::new();
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                params.push(' ');
            }
            append_arg(&mut params, arg);
        }
        let params = wide(OsStr::new(&params));
        let operation = wide(OsStr::new("runas"));
        let dir_w = workdir.map(|d| wide(d.as_os_str()));
        let dir_ptr = dir_w.as_ref().map_or(std::ptr::null(), |w| w.as_ptr());
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                file.as_ptr(),
                params.as_ptr(),
                dir_ptr,
                1, // SW_SHOWNORMAL
            )
        };
        if result > 32 {
            return Ok(());
        }
        let code = result as u32;
        if code == 1223 {
            // ERROR_CANCELLED：用户在 UAC 授权框点了“否”
            return Err("需要管理员权限，但授权被取消，无法启动".into());
        }
        Err(format!("需要管理员权限，提权启动未成功（错误码 {code}）"))
    }
}

#[cfg(not(windows))]
mod elevated_launch {
    use std::path::Path;

    pub fn is_elevation_required(_e: &std::io::Error) -> bool {
        false
    }

    pub fn spawn_elevated(
        _path: &Path,
        _args: &[&str],
        _workdir: Option<&Path>,
    ) -> Result<(), String> {
        Err("当前平台不支持自动提权启动".into())
    }
}

/// 运行 reg.exe 并隐藏控制台窗口（打包版无控制台时若不隐藏，每次调用都会闪现命令行窗口）。
fn reg_output(args: &[&str]) -> std::io::Result<std::process::Output> {
    let mut cmd = std::process::Command::new("reg.exe");
    cmd.args(args);
    hide_console(&mut cmd);
    cmd.output()
}

// ---------- 应用扫描（开始菜单 + UWP + 图标提取） ----------

#[derive(Deserialize)]
struct ScannedApp {
    name: String,
    target: String,
    #[serde(default)]
    args: String,
    #[serde(default)]
    dir: String,
    #[serde(default)]
    icon: String,
}

#[derive(Serialize)]
struct SyncResult {
    added: i32,
    updated: i32,
    total: i32,
}

const SCAN_SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName System.Drawing
$shell = New-Object -ComObject WScript.Shell
$list = @{}
$byTarget = @{}
$byName = @{}
$byLeaf = @{}
$pfMap = @{}
Get-AppxPackage | ForEach-Object { $pfMap[$_.PackageFamilyName.ToLower()] = $_.InstallLocation }

function Get-UwpIcon([string]$loc) {
    $icon = ''
    try {
        $manifest = Join-Path $loc 'AppxManifest.xml'
        if (-not (Test-Path $manifest)) { return '' }
        $xml = [xml](Get-Content $manifest -Raw)
        $rel = ''
        foreach ($app in @($xml.Package.Applications.Application)) {
            $ve = $app.VisualElements
            if (-not $ve) { continue }
            if (([string]$ve.AppListEntry).Trim() -eq 'none') { continue }
            $rel = ([string]$ve.Square44x44Logo).Split("`n")[0].Trim()
            if (-not $rel) { $rel = ([string]$ve.Square150x150Logo).Split("`n")[0].Trim() }
            if ($rel) { break }
        }
        if (-not $rel) { return '' }
        $full = Join-Path $loc $rel
        $dir = [IO.Path]::GetDirectoryName($full)
        $stem = [IO.Path]::GetFileNameWithoutExtension($full)
        $ext = [IO.Path]::GetExtension($full)
        $picked = ''
        foreach ($scale in @('scale-200', 'scale-400')) {
            $try = Join-Path $dir ($stem + '.' + $scale + $ext)
            if (Test-Path $try) { $picked = $try; break }
        }
        if (-not $picked -and (Test-Path $full)) { $picked = $full }
        if (-not $picked -and (Test-Path $dir)) {
            $cand = Get-ChildItem $dir -File -Filter ($stem + '*.png') | Where-Object { $_.Name -match 'scale-200|targetsize-48|applist|storelogo' } | Select-Object -First 1
            if ($cand) { $picked = $cand.FullName }
        }
        if ($picked) {
            $icon = [Convert]::ToBase64String([IO.File]::ReadAllBytes($picked))
        }
    } catch { $icon = '' }
    return $icon
}

function Get-FileIcon([string]$file) {
    $icon = ''
    try {
        $i = [System.Drawing.Icon]::ExtractAssociatedIcon($file)
        if ($i) {
            $b = $i.ToBitmap()
            $ms = New-Object System.IO.MemoryStream
            $b.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
            $icon = [Convert]::ToBase64String($ms.ToArray())
            $ms.Dispose(); $b.Dispose(); $i.Dispose()
        }
    } catch { $icon = '' }
    return $icon
}

function Add-App([string]$name, [string]$target, [string]$argList, [string]$workDir, [string]$pkg) {
    $name = ([string]$name).Trim()
    $target = ([string]$target).Trim()
    $argList = ([string]$argList).Trim()
    if (-not $name -or -not $target) { return }
    $key = ($target + '|' + $argList).ToLower()
    if ($list.ContainsKey($key)) {
        if ($pkg -and -not $list[$key]['pkg']) { $list[$key]['pkg'] = $pkg }
        return
    }
    $list[$key] = @{ name = $name; target = $target; args = $argList; dir = ([string]$workDir).Trim(); pkg = [string]$pkg }
    $byTarget[$target.ToLower()] = $true
    $byName[$name.ToLower()] = $true
    if ($target -match '^[A-Za-z]:\\') { $byLeaf[([IO.Path]::GetFileName($target) + '|' + $argList).ToLower()] = $true }
}

function Add-AppOnce([string]$name, [string]$target, [string]$argList, [string]$workDir, [string]$pkg) {
    $name = ([string]$name).Trim()
    $target = ([string]$target).Trim()
    $argList = ([string]$argList).Trim()
    if (-not $name -or -not $target) { return }
    if ($byTarget.ContainsKey($target.ToLower()) -or $byName.ContainsKey($name.ToLower())) { return }
    if ($target -match '^[A-Za-z]:\\' -and $byLeaf.ContainsKey(([IO.Path]::GetFileName($target) + '|' + $argList).ToLower())) { return }
    Add-App $name $target $argList $workDir $pkg
}

function Read-Lnk([string]$file) {
    $lnk = $shell.CreateShortcut($file)
    $t = [string]$lnk.TargetPath
    $a = ([string]$lnk.Arguments).Trim()
    if (-not $t) { return $null }
    if ($a -match '^shell:AppsFolder\\' -and [IO.Path]::GetFileName($t).ToLower() -eq 'explorer.exe') {
        $t = $a
        $a = ''
    } elseif ($t -notmatch '^shell:AppsFolder\\' -and -not (Test-Path $t)) {
        return $null
    }
    return @{ name = [IO.Path]::GetFileNameWithoutExtension($file); target = $t; args = $a; dir = [string]$lnk.WorkingDirectory }
}

foreach ($dir in @("$env:ProgramData\Microsoft\Windows\Start Menu\Programs", "$env:APPDATA\Microsoft\Windows\Start Menu\Programs")) {
    Get-ChildItem -Path $dir -Filter *.lnk -Recurse | ForEach-Object {
        $i = Read-Lnk $_.FullName
        if ($i) { Add-App $i['name'] $i['target'] $i['args'] $i['dir'] '' }
    }
}

foreach ($dir in @("$env:USERPROFILE\Desktop", "$env:PUBLIC\Desktop")) {
    Get-ChildItem -Path $dir -Filter *.lnk -Recurse | ForEach-Object {
        $i = Read-Lnk $_.FullName
        if ($i) { Add-AppOnce $i['name'] $i['target'] $i['args'] $i['dir'] '' }
    }
}

$shellTargets = @{
    '::{52205FD8-5DFB-447D-801A-D0B52F2E83E1}' = "$env:windir\explorer.exe"
    '::{5399E694-6CE5-4D6C-8FCE-1D8870FDCBA0}' = "$env:windir\System32\control.exe"
    '::{2559A1F3-21D7-11D4-BDAF-00C04F60B9F0}' = "$env:windir\System32\shell32.dll"
}
$appsFolder = (New-Object -ComObject Shell.Application).NameSpace('shell:AppsFolder')
foreach ($it in $appsFolder.Items()) {
    $name = ([string]$it.Name).Trim()
    if (-not $name) { continue }
    $tp = ([string]$it.ExtendedProperty('System.Link.TargetParsingPath')).Trim()
    $pa = ([string]$it.ExtendedProperty('System.Link.Arguments')).Trim()
    $path = ([string]$it.Path).Trim()
    $target = ''
    if ($tp -match '^[A-Za-z]:\\' -or $tp -match '^::' -or $tp -match '^[a-z][a-z0-9+.-]*://') {
        $target = $tp
    } elseif ($path -match '^[A-Za-z]:\\' -or $path -match '^::' -or $path -match '^[a-z][a-z0-9+.-]*://') {
        $target = $path
        $pa = ''
    } elseif ($path) {
        $target = 'shell:AppsFolder\' + $path
        $pa = ''
    }
    if (-not $target) { continue }
    if ($target -match '^https?://') { continue }
    $key = ($target + '|' + $pa).ToLower()
    if ($list.ContainsKey($key)) { $list[$key]['name'] = $name; continue }
    Add-AppOnce $name $target $pa '' ''
}

$skipPath = '\\Windows\\|\\WindowsApps\\|\\SystemApps\\|\\Common Files\\|\\Microsoft Office\\|\\Microsoft Shared\\|\\Windows Mail\\|\\Internet Explorer\\IEDIAG'
foreach ($root in @('HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths', 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths', 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths')) {
    if (-not (Test-Path $root)) { continue }
    Get-ChildItem $root | ForEach-Object {
        $exe = ([string](Get-ItemProperty $_.PSPath).'(default)').Trim().Trim('"')
        if (-not $exe -or $exe -notmatch '^[A-Za-z]:\\' -or -not (Test-Path $exe)) { return }
        if ($exe -match $skipPath) { return }
        $desc = ''
        try { $desc = ([string](Get-Item $exe).VersionInfo.FileDescription).Trim() } catch { $desc = '' }
        if (-not $desc) { $desc = [IO.Path]::GetFileNameWithoutExtension($exe) }
        Add-AppOnce $desc $exe '' ([IO.Path]::GetDirectoryName($exe)) ''
    }
}

Get-StartApps | ForEach-Object {
    $id = $_.AppID
    if ($id -and $id.Contains('!') -and $_.Name) {
        $fam = ($id -split '!')[0].ToLower()
        $pkgLoc = ''
        if ($pfMap.ContainsKey($fam)) { $pkgLoc = $pfMap[$fam] }
        Add-App $_.Name ('shell:AppsFolder\' + $id) '' '' $pkgLoc
    }
}

$result = @()
foreach ($k in $list.Keys) {
    $v = $list[$k]
    $icon = ''
    if ($v['target'] -match '^[A-Za-z]:\\') {
        $icon = Get-FileIcon $v['target']
    } elseif ($shellTargets.ContainsKey($v['target'])) {
        $icon = Get-FileIcon $shellTargets[$v['target']]
    } elseif ($v['pkg']) {
        $icon = Get-UwpIcon $v['pkg']
    }
    $result += @{ name = [string]$v['name']; target = [string]$v['target']; args = [string]$v['args']; dir = [string]$v['dir']; icon = $icon }
}
@{ apps = $result } | ConvertTo-Json -Compress -Depth 4"#;

fn scan_apps() -> Result<Vec<ScannedApp>, String> {
    let mut cmd = std::process::Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        SCAN_SCRIPT,
    ]);
    hide_console(&mut cmd);
    let output = cmd.output().map_err(|e| err_msg("无法启动系统扫描", e))?;
    if !output.status.success() {
        return Err("系统应用扫描失败".into());
    }
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|e| err_msg("解析扫描结果失败", e))?;
    let mut apps = Vec::new();
    if let Some(list) = parsed.get("apps").and_then(|v| v.as_array()) {
        for a in list {
            let name = a.get("name").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            let target = a.get("target").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            if name.is_empty() || target.is_empty() {
                continue;
            }
            let lower = name.to_lowercase();
            const SKIP: [&str; 6] = ["uninstall", "卸载", "帮助", "readme", "troubleshoot", "许可"];
            if SKIP.iter().any(|s| lower.contains(s)) {
                continue;
            }
            apps.push(ScannedApp {
                icon: a.get("icon").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                args: a.get("args").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                dir: a.get("dir").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                name,
                target,
            });
        }
    }
    Ok(apps)
}

fn merge_scanned(app: &AppHandle) -> Result<SyncResult, String> {
    let apps = scan_apps()?;
    let mut result = SyncResult { added: 0, updated: 0, total: apps.len() as i32 };
    with_db(app, |conn| {
        init_schema(conn)?;
        // 单事务批量写入，避免逐条提交造成大量磁盘 IO
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let mut known = HashMap::<String, i64>::new();
        {
            let mut stmt = tx
                .prepare("SELECT id, url, args FROM launch_items WHERE type='application'")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|e| e.to_string())?;
            for row in rows.flatten() {
                // 与扫描端一致：同一个 exe 的不同启动参数是不同应用，需分别保留
                known
                    .entry(format!("{}|{}", row.1.to_lowercase(), row.2.trim().to_lowercase()))
                    .or_insert(row.0);
            }
        }
        for a in &apps {
            let icon_data_url = if a.icon.is_empty() {
                String::new()
            } else {
                format!("data:image/png;base64,{}", a.icon)
            };
            let key = format!("{}|{}", a.target.to_lowercase(), a.args.trim().to_lowercase());
            if let Some(&id) = known.get(&key) {
                // 字段没有变化时跳过写入，减少无谓的 DB 写入
                tx.execute(
                    "UPDATE launch_items SET name=?1, args=?2, workdir=?3,
                     icon=CASE WHEN ?4='' THEN icon ELSE ?4 END
                     WHERE id=?5 AND (name<>?1 OR args<>?2 OR workdir<>?3 OR (?4<>'' AND icon<>?4))",
                    rusqlite::params![a.name, a.args, a.dir, icon_data_url, id],
                )
                .map_err(|e| e.to_string())?;
                result.updated += 1;
            } else {
                tx.execute(
                    "INSERT INTO launch_items (name, alias, type, url, args, workdir, description, icon, enabled, updated_at)
                     VALUES (?1,'','application',?2,?3,?4,'',?5,1,'')",
                    rusqlite::params![a.name, a.target, a.args, a.dir, icon_data_url],
                )
                .map_err(|e| e.to_string())?;
                result.added += 1;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    })?;
    // 同步后清理历史重复项（文件/文件夹按 路径+名称 判定）
    dedupe_items(app)?;
    let millis = now_millis().to_string();
    set_setting_sync(app, "lastSync", &millis)?;
    set_setting_sync(app, "lastSyncSummary", &format!("added:{},updated:{}", result.added, result.updated))?;
    let mut patch = HashMap::new();
    patch.insert("lastSync".to_string(), millis.clone());
    let _ = app.emit(SETTINGS_CHANGED_EVENT, patch);
    with_db(app, |conn| {
        conn.execute(
            "INSERT INTO sync_records (added, updated, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![result.added, result.updated, millis],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })?;
    let _ = app.emit(ITEMS_CHANGED_EVENT, ());
    Ok(result)
}

/// 把耗时的同步任务放到后台线程执行，避免占用主线程导致窗口卡顿。
async fn run_in_background<T>(
    task: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String>
where
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|e| format!("后台任务执行失败：{e}"))?
}

#[tauri::command]
async fn sync_apps(app: AppHandle) -> Result<SyncResult, String> {
    run_in_background(move || merge_scanned(&app)).await
}

#[derive(Clone)]
struct SyncedItem {
    name: String,
    alias: String,
    kind: String,
    url: String,
    description: String,
    icon: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct FolderRoot {
    path: String,
    name: String,
    #[serde(default = "default_true")]
    recursive: bool,
}

#[derive(Clone, Serialize)]
struct DirEntry {
    name: String,
    path: String,
    is_dir: bool,
}

fn merge_synced_items(app: &AppHandle, items: &[SyncedItem]) -> Result<SyncResult, String> {
    let mut result = SyncResult { added: 0, updated: 0, total: items.len() as i32 };
    with_db(app, |conn| {
        init_schema(conn)?;
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let mut known = HashMap::<String, i64>::new();
        {
            let mut stmt = tx
                .prepare("SELECT id, url FROM launch_items")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))
                .map_err(|e| e.to_string())?;
            for row in rows.flatten() {
                known.entry(row.1.to_lowercase()).or_insert(row.0);
            }
        }
        for item in items {
            let key = item.url.to_lowercase();
            if let Some(&id) = known.get(&key) {
                // 保留用户已填写的别名/描述/图标；仅在实际发生变化时写入
                tx.execute(
                    "UPDATE launch_items SET name=?1, alias=CASE WHEN alias='' THEN ?2 ELSE alias END,
                     type=?3, description=CASE WHEN description='' THEN ?4 ELSE description END,
                     icon=CASE WHEN icon='' THEN ?5 ELSE icon END, enabled=1
                     WHERE id=?6 AND (name<>?1 OR type<>?3 OR enabled<>1
                       OR (alias='' AND ?2<>'')
                       OR (description='' AND ?4<>'')
                       OR (icon='' AND ?5<>''))",
                    rusqlite::params![item.name, item.alias, item.kind, item.description, item.icon, id],
                )
                .map_err(|e| e.to_string())?;
                result.updated += 1;
            } else {
                tx.execute(
                    "INSERT INTO launch_items (name, alias, type, url, args, workdir, description, icon, enabled, updated_at)
                     VALUES (?1,?2,?3,?4,'','',?5,?6,1,?7)",
                    rusqlite::params![item.name, item.alias, item.kind, item.url, item.description, item.icon, now_millis().to_string()],
                )
                .map_err(|e| e.to_string())?;
                result.added += 1;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    })?;
    // 同步后清理历史重复项（文件/文件夹按 路径+名称 判定）
    dedupe_items(app)?;
    Ok(result)
}

const FOLDER_ROOTS_KEY: &str = "folderRoots";
const MAX_SYNC_DEPTH: usize = 24;
const MAX_SYNC_ITEMS_PER_ROOT: usize = 20000;

fn get_setting_value(app: &AppHandle, key: &str) -> Option<String> {
    with_db(app, |conn| {
        init_schema(conn)?;
        let mut stmt = conn
            .prepare("SELECT value FROM settings WHERE key=?1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([key]).map_err(|e| e.to_string())?;
        if let Ok(Some(row)) = rows.next() {
            return Ok(Some(row.get::<_, String>(0).map_err(|e| e.to_string())?));
        }
        Ok(None)
    })
    .ok()
    .flatten()
}

fn root_name_of(path: &str) -> String {
    let p = Path::new(path);
    if let Some(file_name) = p.file_name().and_then(|s| s.to_str()) {
        let name = file_name.trim();
        if !name.is_empty() {
            return name.to_string();
        }
    }
    if let Some(component) = p.components().next() {
        let label = component.as_os_str().to_string_lossy().to_string();
        if label.ends_with(':') || label == "/" {
            return label;
        }
    }
    path.to_string()
}

fn normalize_alias(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// 首次运行：用系统常用目录作为默认同步目录并落库；用户主动删除后为空数组时不再回退。
fn load_folder_roots(app: &AppHandle) -> Vec<FolderRoot> {
    match get_setting_value(app, FOLDER_ROOTS_KEY) {
        Some(raw) if raw.trim().is_empty() || raw.trim() == "[]" => Vec::new(),
        Some(raw) => serde_json::from_str::<Vec<FolderRoot>>(&raw).unwrap_or_default(),
        None => {
            let defaults = default_standard_dirs(app);
            if !defaults.is_empty() {
                let _ = save_folder_roots(app, &defaults);
            }
            defaults
        }
    }
}

fn save_folder_roots(app: &AppHandle, roots: &[FolderRoot]) -> Result<(), String> {
    let json = serde_json::to_string(roots).map_err(|e| err_msg("序列化同步目录失败", e))?;
    set_setting_sync(app, FOLDER_ROOTS_KEY, &json)
}

fn default_standard_dirs(app: &AppHandle) -> Vec<FolderRoot> {
    let Some(home) = dirs_home(app) else { return Vec::new() };
    let mut roots = Vec::new();
    for (label, candidates) in [
        ("桌面", &["Desktop", "桌面"][..]),
        ("文档", &["Documents", "文档"][..]),
        ("下载", &["Downloads", "下载"][..]),
        ("图片", &["Pictures", "图片"][..]),
    ] {
        if let Some(path) = candidates.iter().map(|c| home.join(c)).find(|p| p.is_dir()) {
            roots.push(FolderRoot {
                path: path.to_string_lossy().to_string(),
                name: label.to_string(),
                recursive: true,
            });
        }
    }
    roots
}

#[cfg(windows)]
fn should_skip_entry(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            let attrs = meta.file_attributes();
            attrs & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM | FILE_ATTRIBUTE_REPARSE_POINT) != 0
        }
        Err(_) => false,
    }
}

#[cfg(not(windows))]
fn should_skip_entry(path: &Path) -> bool {
    let _ = path;
    false
}

fn scan_folder_items(root: &FolderRoot, out: &mut Vec<SyncedItem>) {
    let root_path = Path::new(&root.path);
    if !root_path.is_dir() {
        return;
    }
    let root_url = root_path.to_string_lossy().to_string();
    let root_alias = normalize_alias(&root.name);
    out.push(SyncedItem {
        name: root.name.clone(),
        alias: root_alias.clone(),
        kind: "folder".into(),
        url: root_url,
        description: "同步目录".into(),
        icon: String::new(),
    });
    if !root.recursive {
        return;
    }
    let mut count = 0usize;
    // 广度优先遍历：先覆盖各目录的浅层内容，避免超深子目录（如 .pnpm-store 缓存）
    // 像之前 DFS 那样独占整棵树的额度，导致其它目录（如下载目录）的文件完全缺失。
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((root_path.to_path_buf(), 1));
    while let Some((dir, depth)) = queue.pop_front() {
        if depth > MAX_SYNC_DEPTH || count >= MAX_SYNC_ITEMS_PER_ROOT {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut sub_dirs: Vec<(PathBuf, String)> = Vec::new();
        for entry in entries.flatten() {
            if count >= MAX_SYNC_ITEMS_PER_ROOT {
                break;
            }
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.is_empty() || should_skip_entry(&path) {
                continue;
            }
            #[cfg(not(windows))]
            if file_name.starts_with('.') {
                continue;
            }
            let is_dir = path.is_dir();
            let kind = if is_dir { "folder" } else { "file" };
            out.push(SyncedItem {
                name: file_name.clone(),
                alias: root_alias.clone(),
                kind: kind.into(),
                url: path.to_string_lossy().to_string(),
                description: format!("来自{}", root.name),
                icon: String::new(),
            });
            count += 1;
            if is_dir {
                sub_dirs.push((path, file_name));
            }
        }
        // 子目录按名称升序入队，广度优先下各分支被均匀、稳定地覆盖
        sub_dirs.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
        for (sub, _) in sub_dirs {
            queue.push_back((sub, depth + 1));
        }
    }
}

#[tauri::command]
fn get_folder_roots(app: AppHandle) -> Vec<FolderRoot> {
    load_folder_roots(&app)
}

#[tauri::command]
fn default_folder_roots(app: AppHandle) -> Vec<FolderRoot> {
    default_standard_dirs(&app)
}

#[tauri::command]
fn add_folder_root(app: AppHandle, path: String) -> Result<Vec<FolderRoot>, String> {
    let trimmed = {
        let value = path.trim();
        let lower = value.to_lowercase();
        if value.ends_with('\\') && !(lower.ends_with(":\\") || lower.ends_with(":/")) {
            value.trim_end_matches('\\').to_string()
        } else {
            value.to_string()
        }
    };
    if trimmed.is_empty() {
        return Err("目录不能为空".into());
    }
    if !Path::new(&trimmed).is_dir() {
        return Err(format!("目录不存在或不可访问：{trimmed}"));
    }
    let mut roots = load_folder_roots(&app);
    if !roots.iter().any(|r| r.path.eq_ignore_ascii_case(&trimmed)) {
        roots.push(FolderRoot {
            path: trimmed.clone(),
            name: root_name_of(&trimmed),
            recursive: true,
        });
        save_folder_roots(&app, &roots)?;
    }
    Ok(roots)
}

#[tauri::command]
fn remove_folder_root(app: AppHandle, path: String) -> Result<Vec<FolderRoot>, String> {
    let mut roots = load_folder_roots(&app);
    roots.retain(|r| !r.path.eq_ignore_ascii_case(&path));
    save_folder_roots(&app, &roots)?;
    Ok(roots)
}

fn drive_entries() -> Vec<DirEntry> {
    let mut list = Vec::new();
    #[cfg(windows)]
    {
        for letter in 'C'..='Z' {
            let path = format!("{letter}:\\");
            if Path::new(&path).is_dir() {
                list.push(DirEntry {
                    name: format!("{letter}:\\"),
                    path: path.clone(),
                    is_dir: true,
                });
            }
        }
    }
    #[cfg(not(windows))]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        list.push(DirEntry {
            name: "/".into(),
            path: home,
            is_dir: true,
        });
    }
    list
}

#[tauri::command]
fn list_drives() -> Vec<DirEntry> {
    drive_entries()
}

fn is_hidden_entry(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            let attrs = meta.file_attributes();
            if attrs & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0 {
                return true;
            }
        }
    }
    false
}

#[tauri::command]
fn list_directory(path: String) -> Result<Vec<DirEntry>, String> {
    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Err("目录不存在或不可访问".into());
    }
    let mut entries = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return Ok(entries) };
    for entry in rd.flatten() {
        let child = entry.path();
        if !child.is_dir() || is_hidden_entry(&child) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.is_empty() {
            continue;
        }
        entries.push(DirEntry {
            name: name.clone(),
            path: child.to_string_lossy().to_string(),
            is_dir: true,
        });
    }
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(entries)
}

#[tauri::command]
async fn sync_folders(app: AppHandle, only_path: Option<String>) -> Result<SyncResult, String> {
    run_in_background(move || {
        let roots = load_folder_roots(&app);
        let mut items = Vec::new();
        for root in roots {
            let matched = match &only_path {
                Some(path) => root.path.eq_ignore_ascii_case(path),
                None => true,
            };
            if matched {
                scan_folder_items(&root, &mut items);
            }
        }
        let mut unique = HashMap::<String, SyncedItem>::new();
        for item in items {
            unique.entry(item.url.to_lowercase()).or_insert(item);
        }
        let items: Vec<SyncedItem> = unique.into_values().collect();
        let result = merge_synced_items(&app, &items)?;
        let millis = now_millis().to_string();
        set_setting_sync(&app, "lastFolderSync", &millis)?;
        let mut patch = HashMap::new();
        patch.insert("lastFolderSync".to_string(), millis);
        let _ = app.emit(SETTINGS_CHANGED_EVENT, patch);
        let _ = app.emit(ITEMS_CHANGED_EVENT, ());
        Ok(result)
    })
    .await
}

/// Chromium 系浏览器（Chrome/Edge/Brave）书签文件顶层没有 children，
/// 收藏按 roots（收藏夹栏 / 其他收藏夹 / 移动设备收藏夹等）分组，需先遍历各组。
type FaviconMap = HashMap<String, String>;

/// 读取浏览器自带的 Favicons 本地图标库，返回 “页面URL -> data:image 图标”。
/// 与收藏夹同目录，离线可用且与浏览器显示的图标一致。
fn load_favicon_map(profile_dir: &Path) -> FaviconMap {
    let favicons = profile_dir.join("Favicons");
    if !favicons.exists() {
        return HashMap::new();
    }
    // 浏览器运行时可能对 Favicons 加排他锁，用 immutable 只读 URI 可绕过锁读取本地图标
    let uri = format!(
        "file:{}?immutable=1",
        favicons.to_string_lossy().replace('\\', "/")
    );
    let Ok(conn) = rusqlite::Connection::open_with_flags(
        uri,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            | rusqlite::OpenFlags::SQLITE_OPEN_URI
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return HashMap::new();
    };
    let _ = conn.busy_timeout(std::time::Duration::from_millis(2000));
    let mut map = FaviconMap::new();
    let Ok(mut stmt) = conn.prepare(
        "SELECT m.page_url, b.image_data FROM icon_mapping m
         JOIN favicon_bitmaps b ON b.icon_id = m.icon_id
         ORDER BY b.width DESC, b.height DESC",
    ) else {
        return map;
    };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
    }) else {
        return map;
    };
    for row in rows.flatten() {
        let (page_url, data) = row;
        if data.is_empty() || data.len() > 256 * 1024 {
            continue;
        }
        let key = normalize_bookmark_url(&page_url);
        if key.is_empty() || map.contains_key(&key) {
            continue;
        }
        let Some(mime) = detect_image_mime(&data) else { continue };
        let encoded = base64::engine::general_purpose::STANDARD.encode(&data);
        map.insert(key, format!("data:image/{mime};base64,{encoded}"));
    }
    map
}

fn normalize_bookmark_url(url: &str) -> String {
    let mut value = url.trim().to_lowercase();
    while value.ends_with('/') {
        value.pop();
    }
    value
}

fn detect_image_mime(data: &[u8]) -> Option<&'static str> {
    if data.len() >= 8 && &data[..8] == b"\x89PNG\r\n\x1a\n" {
        return Some("png");
    }
    if data.starts_with(b"\xff\xd8") {
        return Some("jpeg");
    }
    if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return Some("webp");
    }
    if data.starts_with(b"\x00\x00\x01\x00") {
        return Some("x-icon");
    }
    if data.len() >= 6 && &data[..6] == b"GIF87a" || data.len() >= 6 && &data[..6] == b"GIF89a" {
        return Some("gif");
    }
    None
}

fn url_host(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    let host = rest.split(['/', '?', '#']).next()?.trim();
    let host = host.split('@').last().unwrap_or(host).split(':').next().unwrap_or("").trim();
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn favicon_profile_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let root = PathBuf::from(local);
        for profile_root in [
            root.join("Microsoft/Edge/User Data"),
            root.join("Google/Chrome/User Data"),
            root.join("BraveSoftware/Brave-Browser/User Data"),
        ] {
            let Ok(profiles) = std::fs::read_dir(&profile_root) else { continue };
            for profile in profiles.flatten() {
                let profile_dir = profile.path();
                if !profile_dir.is_dir() {
                    continue;
                }
                let profile_name = profile.file_name().to_string_lossy().to_string();
                if profile_name != "Default" && !profile_name.starts_with("Profile ") {
                    continue;
                }
                if profile_dir.join("Favicons").exists() {
                    dirs.push(profile_dir);
                }
            }
        }
    }
    dirs
}

/// 网络兜底：依次尝试站点根目录 favicon、Google、DuckDuckGo 图标服务，返回 base64 文本。
fn fetch_remote_site_icon(host: &str) -> String {
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$hostName = '{host}'
$tmp = [System.IO.Path]::GetTempFileName()
$ok = $false
foreach ($template in @('https://{0}/favicon.ico', 'https://www.google.com/s2/favicons?domain={0}&sz=64', 'https://icons.duckduckgo.com/ip3/{0}.ico')) {
    $uri = [string]::Format($template, $hostName)
    try {
        Invoke-WebRequest -UseBasicParsing -Uri $uri -OutFile $tmp -TimeoutSec 5 | Out-Null
        if ((Get-Item $tmp).Length -gt 0) {
            $ok = $true
            break
        }
    } catch { }
}
if ($ok) {
    $data = [System.IO.File]::ReadAllBytes($tmp)
    [Console]::OutputEncoding = [System.Text.Encoding]::ASCII
    [Console]::Write([System.Convert]::ToBase64String($data))
}
Remove-Item $tmp -ErrorAction SilentlyContinue
"#;
    let escaped_host = host.replace('\'', "''");
    let script = script.replace("{host}", &escaped_host);
    let mut cmd = std::process::Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &script,
    ]);
    hide_console(&mut cmd);
    let Ok(output) = cmd.output() else { return String::new() };
    if !output.status.success() {
        return String::new();
    }
    let b64 = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) else {
        return String::new();
    };
    let Some(mime) = detect_image_mime(&bytes) else { return String::new() };
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    format!("data:image/{mime};base64,{encoded}")
}

#[tauri::command]
fn resolve_website_icon(url: String) -> String {
    let trimmed = url.trim().to_string();
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return String::new();
    }
    // 1) 优先读浏览器本地图标库（离线、与浏览器一致）
    let page_key = normalize_bookmark_url(&trimmed);
    for dir in favicon_profile_dirs() {
        let map = load_favicon_map(&dir);
        if let Some(icon) = map.get(&page_key) {
            return icon.clone();
        }
        if let Some((base, _)) = page_key.split_once('#') {
            if let Some(icon) = map.get(base) {
                return icon.clone();
            }
        }
    }
    // 2) 本地没有则从网络获取站点图标
    if let Some(host) = url_host(&trimmed) {
        return fetch_remote_site_icon(&host);
    }
    String::new()
}

fn collect_bookmarks(
    json: &serde_json::Value,
    source: &str,
    favicons: &FaviconMap,
    out: &mut Vec<SyncedItem>,
) {
    let roots = json
        .get("roots")
        .and_then(|value| value.as_object())
        .map(|map| map.values().collect::<Vec<_>>())
        .unwrap_or_default();
    if roots.is_empty() {
        // 兜底：兼容无 roots 分组的书签 JSON
        collect_bookmark_branch(json, source, "", favicons, out);
        return;
    }
    for root in roots {
        let folder_name = root.get("name").and_then(|v| v.as_str()).unwrap_or("");
        collect_bookmark_branch(root, source, folder_name, favicons, out);
    }
}

fn collect_bookmark_branch(
    node: &serde_json::Value,
    source: &str,
    folder_path: &str,
    favicons: &FaviconMap,
    out: &mut Vec<SyncedItem>,
) {
    let Some(children) = node.get("children").and_then(|value| value.as_array()) else { return };
    for child in children {
        if child.get("type").and_then(|value| value.as_str()) == Some("url") {
            let url = child.get("url").and_then(|value| value.as_str()).unwrap_or("").trim();
            let name = child.get("name").and_then(|value| value.as_str()).unwrap_or("").trim();
            if !name.is_empty() && (url.starts_with("http://") || url.starts_with("https://")) {
                let folder_desc = if folder_path.is_empty() {
                    String::new()
                } else {
                    format!(" · {folder_path}")
                };
                let page_key = normalize_bookmark_url(url);
                let icon = favicons
                    .get(&page_key)
                    .or_else(|| page_key.split_once('#').and_then(|(base, _)| favicons.get(base)))
                    .cloned()
                    .unwrap_or_default();
                out.push(SyncedItem {
                    name: name.into(),
                    alias: source.to_lowercase(),
                    kind: "website".into(),
                    url: url.into(),
                    description: format!("来自{}收藏夹{}", source, folder_desc),
                    icon,
                });
            }
        }
        // 文件夹节点继续下钻，保留所属收藏夹路径用于描述
        if child.get("children").and_then(|v| v.as_array()).is_some() {
            if let Some(name) = child.get("name").and_then(|v| v.as_str()) {
                let next = if folder_path.is_empty() {
                    name.trim().to_string()
                } else {
                    format!("{folder_path}/{}", name.trim())
                };
                collect_bookmark_branch(child, source, &next, favicons, out);
            }
        }
    }
}

/// 查询系统默认浏览器（http 协议）对应 Chrome/Edge/Brave，用于收藏去重时优先保留默认浏览器。
#[cfg(windows)]
fn http_default_browser_key() -> Option<String> {
    let output = reg_output(&[
        "query",
        r"HKCU\Software\Microsoft\Windows\Shell\Associations\UrlAssociations\http\UserChoice",
        "/v",
        "ProgId",
    ])
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).to_lowercase();
    if text.contains("chromehtml") {
        return Some("chrome".into());
    }
    if text.contains("msedgehtm") {
        return Some("edge".into());
    }
    if text.contains("bravehtml") {
        return Some("brave".into());
    }
    None
}

#[cfg(not(windows))]
fn http_default_browser_key() -> Option<String> {
    None
}

fn browser_base(label: &str) -> &str {
    label.split_whitespace().next().unwrap_or("")
}

fn browser_priority(base: &str, default: Option<&str>) -> u8 {
    let lower = base.to_lowercase();
    if default.map(|d| lower == d).unwrap_or(false) {
        return 0;
    }
    match lower.as_str() {
        "edge" => 1,
        "chrome" => 2,
        "brave" => 3,
        _ => 4,
    }
}

fn is_default_profile(label: &str) -> bool {
    label
        .split_whitespace()
        .nth(1)
        .map(|p| p.eq_ignore_ascii_case("default"))
        .unwrap_or(true)
}

fn bookmark_sources() -> Vec<(String, PathBuf)> {
    let mut sources = Vec::new();
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let root = PathBuf::from(local);
        for (name, profile_root) in [
            ("Edge", root.join("Microsoft/Edge/User Data")),
            ("Chrome", root.join("Google/Chrome/User Data")),
            ("Brave", root.join("BraveSoftware/Brave-Browser/User Data")),
        ] {
            let Ok(profiles) = std::fs::read_dir(&profile_root) else { continue };
            for profile in profiles.flatten() {
                let profile_dir = profile.path();
                if !profile_dir.is_dir() { continue }
                let profile_name = profile.file_name().to_string_lossy().to_string();
                if profile_name != "Default" && !profile_name.starts_with("Profile ") { continue }
                let path = profile_dir.join("Bookmarks");
                if path.exists() { sources.push((format!("{} {}", name, profile_name), path)); }
            }
        }
    }
    // 默认浏览器收藏优先，同名网址只保留一条
    let default = http_default_browser_key();
    sources.sort_by(|a, b| {
        let pa = browser_priority(browser_base(&a.0), default.as_deref());
        let pb = browser_priority(browser_base(&b.0), default.as_deref());
        pa.cmp(&pb)
            .then_with(|| is_default_profile(&b.0).cmp(&is_default_profile(&a.0)))
    });
    sources
}

#[tauri::command]
async fn sync_bookmarks(app: AppHandle) -> Result<SyncResult, String> {
    run_in_background(move || {
        let mut items = Vec::new();
        for (source, path) in bookmark_sources() {
            let text = match std::fs::read_to_string(&path) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
            let profile_dir = path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from(""));
            let favicons = load_favicon_map(&profile_dir);
            collect_bookmarks(&json, &source, &favicons, &mut items);
        }
        let mut unique = HashMap::<String, SyncedItem>::new();
        for item in items {
            unique.entry(item.url.to_lowercase()).or_insert(item);
        }
        let items: Vec<SyncedItem> = unique.into_values().collect();
        let result = merge_synced_items(&app, &items)?;
        let millis = now_millis().to_string();
        set_setting_sync(&app, "lastWebsiteSync", &millis)?;
        let mut patch = HashMap::new();
        patch.insert("lastWebsiteSync".to_string(), millis);
        let _ = app.emit(SETTINGS_CHANGED_EVENT, patch);
        let _ = app.emit(ITEMS_CHANGED_EVENT, ());
        Ok(result)
    })
    .await
}

// ---------- 窗口管理 ----------

#[tauri::command]
fn show_settings_window(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(SETTINGS_WINDOW) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    Ok(())
}

#[tauri::command]
fn hide_settings_window(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(SETTINGS_WINDOW) {
        let _ = w.hide();
    }
    Ok(())
}

#[tauri::command]
fn minimize_settings_window(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(SETTINGS_WINDOW) {
        let _ = w.minimize();
    }
    Ok(())
}

#[tauri::command]
fn hide_launcher(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        let _ = w.hide();
    }
    Ok(())
}

#[tauri::command]
fn focus_launcher(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        let _ = w.show();
        let _ = w.set_focus();
    }
    Ok(())
}

// 拖动启动器窗口时窗口会短暂失去焦点，需要暂缓“失去焦点自动隐藏”，否则一拖就消失。
static LAUNCHER_DRAGGING: AtomicBool = AtomicBool::new(false);
static LAUNCHER_DRAG_UNTIL: AtomicU64 = AtomicU64::new(0);

/// 前端在开始/结束拖动时调用，拖动期间忽略失焦隐藏。
/// 附带 5 秒兜底过期，避免异常情况下永久不再自动隐藏。
#[tauri::command]
fn set_launcher_drag(active: bool) {
    LAUNCHER_DRAGGING.store(active, Ordering::SeqCst);
    if active {
        LAUNCHER_DRAG_UNTIL.store((now_millis() + 5000) as u64, Ordering::SeqCst);
    }
}

fn launcher_dragging() -> bool {
    if !LAUNCHER_DRAGGING.load(Ordering::SeqCst) {
        return false;
    }
    if (now_millis() as u64) > LAUNCHER_DRAG_UNTIL.load(Ordering::SeqCst) {
        LAUNCHER_DRAGGING.store(false, Ordering::SeqCst);
        return false;
    }
    true
}

#[tauri::command]
fn resize_launcher(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        let width = width.clamp(320.0, 720.0);
        let h = height.clamp(48.0, 760.0);
        w.set_size(LogicalSize::new(width, h)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Windows 11 默认会给无边框窗口套用系统圆角，会盖过 CSS 的 border-radius，
/// 导致启动器“搜索框圆角”设置无法生效。这里关闭系统圆角，让外观完全由 CSS 控制。
#[cfg(windows)]
fn disable_windows_corner_rounding(window: &tauri::WebviewWindow) {
    if let Ok(hwnd) = window.hwnd() {
        const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
        const DWMWCP_DONOTROUND: u32 = 1;
        let preference = DWMWCP_DONOTROUND;
        unsafe {
            DwmSetWindowAttribute(
                hwnd.0 as *mut core::ffi::c_void,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &preference,
                core::mem::size_of::<u32>() as u32,
            );
        }
    }
}

#[cfg(not(windows))]
fn disable_windows_corner_rounding(_window: &tauri::WebviewWindow) {}

#[tauri::command]
fn quit_app(app: AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

// ---------- 置顶 / 托盘 / 关闭行为 ----------

fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// 根据“置顶”设置实时应用到启动器窗口。
fn apply_always_on_top(app: &AppHandle) {
    let enabled = read_bool_setting(app, "alwaysOnTop", true);
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        let _ = w.set_always_on_top(enabled);
    }
}

#[tauri::command]
fn set_always_on_top(app: AppHandle, enabled: bool) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        w.set_always_on_top(enabled).map_err(|e| e.to_string())?;
    }
    set_setting_sync(&app, "alwaysOnTop", &enabled.to_string())?;
    let mut patch = HashMap::new();
    patch.insert("alwaysOnTop".to_string(), enabled.to_string());
    let _ = app.emit(SETTINGS_CHANGED_EVENT, patch);
    Ok(())
}

#[tauri::command]
fn set_close_to_tray(app: AppHandle, enabled: bool) -> Result<(), String> {
    set_setting_sync(&app, "closeToTray", &enabled.to_string())?;
    let mut patch = HashMap::new();
    patch.insert("closeToTray".to_string(), enabled.to_string());
    let _ = app.emit(SETTINGS_CHANGED_EVENT, patch);
    Ok(())
}

/// 设置窗口的“关闭”动作：开启驻留托盘时仅隐藏窗口，否则退出整个应用。
#[tauri::command]
fn close_settings_window(app: AppHandle) -> Result<(), String> {
    if read_bool_setting(&app, "closeToTray", true) {
        hide_settings_window(app)
    } else {
        app.exit(0);
        Ok(())
    }
}

/// 创建系统托盘图标与菜单，供驻留后台时唤出启动器、打开设置或退出应用。
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show-launcher", "显示启动器", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "open-settings", "设置", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出应用", true, None::<&str>)?;
    let menu = MenuBuilder::new(app)
        .item(&show_item)
        .item(&settings_item)
        .separator()
        .item(&quit_item)
        .build()?;
    let mut builder = TrayIconBuilder::with_id("nexious-tray")
        .tooltip("Nexious Quick")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show-launcher" => show_main_window(app),
            "open-settings" => {
                let _ = show_settings_window(app.clone());
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_launcher(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let tray = builder.build(app)?;
    let _ = app.manage(tray);
    Ok(())
}

// ---------- 全局快捷键 ----------

fn apply_shortcut(app: &AppHandle, combo: &str) -> Result<(), String> {
    #[cfg(desktop)]
    {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        let gs = app.global_shortcut();
        gs.unregister_all().map_err(|e| e.to_string())?;
        if !combo.trim().is_empty() {
            gs.register(combo.trim()).map_err(|e| {
                format!("快捷键 “{combo}” 注册失败（可能已被其他程序占用）：{e}")
            })?;
        }
    }
    Ok(())
}

#[tauri::command]
fn set_shortcut(app: AppHandle, combo: String) -> Result<(), String> {
    apply_shortcut(&app, &combo)?;
    set_setting_sync(&app, "shortcut", &combo)?;
    let mut patch = HashMap::new();
    patch.insert("shortcut".to_string(), combo);
    let _ = app.emit(SETTINGS_CHANGED_EVENT, patch);
    Ok(())
}

fn toggle_launcher(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        let visible = w.is_visible().unwrap_or(false);
        if visible {
            let _ = w.hide();
        } else {
            let _ = w.show();
            let _ = w.center();
            let _ = w.set_focus();
        }
    }
}

fn main() {
    let shortcut_plugin = {
        use tauri_plugin_global_shortcut::{Builder as ShortcutBuilder, ShortcutState};
        ShortcutBuilder::new()
            .with_handler(|app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    toggle_launcher(app);
                }
            })
            .build()
    };
    tauri::Builder::default()
        // 单实例：后台驻留/自启动后再次运行程序时，不再拉起第二个进程，
        // 而是聚焦到已运行的启动器窗口，避免出现多个后台实例。
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // 第二次实例若来自开机自启（--autostart），保持后台静默；用户主动运行才唤出启动器
            if !argv.iter().any(|arg| arg == "--autostart") {
                show_main_window(app);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(shortcut_plugin)
        .setup(|app| {
            let handle = app.handle().clone();
            with_db(&handle, |conn| init_schema(conn))?;
            let shortcut = with_db(&handle, |conn| {
                init_schema(conn)?;
                let mut stmt = conn.prepare("SELECT value FROM settings WHERE key='shortcut'").map_err(|e| e.to_string())?;
                let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
                if let Ok(Some(row)) = rows.next() {
                    let v: String = row.get(0).map_err(|e| e.to_string())?;
                    if !v.trim().is_empty() {
                        return Ok(v);
                    }
                }
                Ok("Alt+Space".to_string())
            })?;
            if let Err(err) = apply_shortcut(&handle, &shortcut) {
                eprintln!("[nexious] {err}");
            }
            // 开机自启（--autostart）时后台驻留托盘，不弹出启动器，避免开机抢占界面/资源造成卡顿
            let autostart = is_autostart_launch();
            if autostart {
                #[cfg(windows)]
                ensure_autostart_flag();
            }
            // 仅在“失去焦点时隐藏”开启时隐藏启动器；默认保持窗口固定显示。
            {
                let h = handle.clone();
                let started_at = std::time::Instant::now();
                if let Some(main) = h.get_webview_window(MAIN_WINDOW) {
                    // 关闭 Windows 11 系统自动圆角，让“搜索框圆角”设置精确生效
                    disable_windows_corner_rounding(&main);
                    if autostart {
                        let _ = main.hide();
                    } else {
                        let _ = main.show();
                        let _ = main.center();
                        let _ = main.set_focus();
                    }
                    main.on_window_event(move |event| {
                        if let tauri::WindowEvent::Focused(false) = event {
                            if started_at.elapsed() < std::time::Duration::from_secs(2) {
                                return;
                            }
                            // 拖动窗口过程中忽略失焦隐藏，避免拖动被中断
                            if launcher_dragging() {
                                return;
                            }
                            let should_hide = with_db(&h, |conn| {
                                Ok(conn
                                    .query_row(
                                        "SELECT value FROM settings WHERE key='hideOnBlur'",
                                        [],
                                        |row| row.get::<_, String>(0),
                                    )
                                    .ok()
                                    .map(|value| value == "true")
                                    .unwrap_or(false))
                            })
                            .unwrap_or(false);
                            if !should_hide {
                                return;
                            }
                            if let Some(w) = h.get_webview_window(MAIN_WINDOW) {
                                let _ = w.hide();
                            }
                        }
                    });
                }
            }
            // 应用“置顶”设置到启动器窗口
            apply_always_on_top(&handle);
            // 拦截关闭事件：开启“驻留托盘”时，关闭窗口改为隐藏到托盘
            for label in [MAIN_WINDOW, SETTINGS_WINDOW] {
                let h = handle.clone();
                if let Some(win) = h.get_webview_window(label) {
                    win.on_window_event(move |event| {
                        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                            if read_bool_setting(&h, "closeToTray", true) {
                                api.prevent_close();
                                if let Some(w) = h.get_webview_window(label) {
                                    let _ = w.hide();
                                }
                            } else {
                                // 未开启“驻留托盘”：关闭窗口视为退出整个应用
                                h.exit(0);
                            }
                        }
                    });
                }
            }
            // 系统托盘：驻留后台时可唤出启动器 / 打开设置 / 退出应用
            if let Err(err) = setup_tray(app) {
                eprintln!("[nexious] 托盘初始化失败：{err}");
            }
            // 后台自动同步延迟错峰执行，避免开机瞬间与窗口初始化/用户操作争抢资源造成卡顿
            {
                let h = handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(if autostart { 20 } else { 6 }));
                    let auto = with_db(&h, |conn| {
                        Ok(conn
                            .query_row(
                                "SELECT value FROM settings WHERE key='autoSyncApps'",
                                [],
                                |row| row.get::<_, String>(0),
                            )
                            .ok())
                    })
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "true".into());
                    if auto == "false" {
                        return;
                    }
                    let stale = with_db(&h, |conn| {
                        Ok(conn
                            .query_row(
                                "SELECT value FROM settings WHERE key='lastSync'",
                                [],
                                |row| row.get::<_, String>(0),
                            )
                            .ok())
                    })
                    .ok()
                    .flatten()
                    .and_then(|v| v.parse::<u128>().ok())
                    .map(|last| now_millis().saturating_sub(last) > 24 * 3600 * 1000)
                    .unwrap_or(true);
                    if !stale {
                        return;
                    }
                    if let Ok(r) = merge_scanned(&h) {
                        let _ = r;
                    }
                });
            }
            // 启动数据去重：清理历史遗留的重复条目（延迟执行，避开启动高峰）
            {
                let h = handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(if autostart { 15 } else { 4 }));
                    if let Ok(deleted) = dedupe_items(&h) {
                        if deleted > 0 {
                            let _ = h.emit(ITEMS_CHANGED_EVENT, ());
                        }
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            db_load_items,
            db_load_items_by_kind,
            db_page_items,
            search_items,
            db_save_item,
            db_delete_item,
            db_load_settings,
            db_save_settings,
            resolve_website_icon,
            get_setting,
            get_user_dirs,
            get_folder_roots,
            default_folder_roots,
            add_folder_root,
            remove_folder_root,
            list_drives,
            list_directory,
            open_item,
            open_url,
            check_latest_release,
            sync_apps,
            sync_folders,
            sync_bookmarks,
            show_settings_window,
            hide_settings_window,
            minimize_settings_window,
            close_settings_window,
            set_always_on_top,
            set_close_to_tray,
            hide_launcher,
            focus_launcher,
            set_launcher_drag,
            resize_launcher,
            quit_app,
            set_shortcut,
            get_autostart,
            set_autostart
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

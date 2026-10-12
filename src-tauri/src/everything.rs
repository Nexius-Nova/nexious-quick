//! Read Voidtools' local index. Never walk the filesystem recursively.
use super::{FolderRoot, SyncedItem};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
struct IndexedPath {
    path: String,
    folder: bool,
}

#[derive(Serialize, Deserialize)]
struct QueryOutput {
    entries: Vec<IndexedPath>,
    error: Option<String>,
}

fn unavailable() -> String {
    "请先启动 Everything 并等待索引就绪，已有文件数据已保留。".into()
}

// An anchored literal regex avoids Everything operators in folder names becoming queries.
fn root_query(roots: &[FolderRoot]) -> String {
    let alternatives = roots
        .iter()
        .map(|root| {
            let normalized = root.path.replace('/', "\\");
            let mut literal = String::new();
            for ch in normalized.trim_end_matches('\\').chars() {
                if "\\.^$|?*+()[]{}".contains(ch) {
                    literal.push('\\');
                }
                literal.push(ch);
            }
            format!("{literal}(?:\\\\|$)")
        })
        .collect::<Vec<_>>();
    format!("^(?:{})", alternatives.join("|"))
}

#[cfg(windows)]
mod sdk {
    use super::*;
    unsafe extern "system" {
        fn Everything_SetSearchW(value: *const u16);
        fn Everything_SetMatchPath(value: i32);
        fn Everything_SetRegex(value: i32);
        fn Everything_SetMax(value: u32);
        fn Everything_SetRequestFlags(value: u32);
        fn Everything_QueryW(wait: i32) -> i32;
        fn Everything_IsDBLoaded() -> i32;
        fn Everything_GetNumResults() -> u32;
        fn Everything_GetTotResults() -> u32;
        fn Everything_GetResultFullPathNameW(index: u32, buffer: *mut u16, size: u32) -> u32;
        fn Everything_IsFolderResult(index: u32) -> i32;
        fn Everything_GetLastError() -> u32;
        fn Everything_CleanUp();
    }

    pub fn query(search: &str) -> Result<Vec<IndexedPath>, String> {
        struct Cleanup;
        impl Drop for Cleanup {
            fn drop(&mut self) {
                unsafe {
                    Everything_CleanUp();
                }
            }
        }
        let _cleanup = Cleanup;
        unsafe {
            if Everything_IsDBLoaded() == 0 {
                return Err(unavailable());
            }
            let search = search.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
            Everything_SetMatchPath(1);
            Everything_SetRegex(1);
            Everything_SetSearchW(search.as_ptr());
            Everything_SetRequestFlags(0x4); // FULL_PATH_AND_FILE_NAME
            Everything_SetMax(u32::MAX);
            if Everything_QueryW(1) == 0 {
                return Err(format!(
                    "Everything 暂不可用，已有文件数据已保留（{}）。",
                    Everything_GetLastError()
                ));
            }
            let count = Everything_GetNumResults();
            if count != Everything_GetTotResults() {
                return Err("Everything 未返回完整索引，稍后自动重试。".into());
            }
            let mut entries = Vec::with_capacity(count as usize);
            for i in 0..count {
                let size = Everything_GetResultFullPathNameW(i, std::ptr::null_mut(), 0);
                if size == 0 {
                    return Err("Everything 返回了无效路径，已有文件数据已保留。".into());
                }
                let mut buffer = vec![0u16; size as usize + 1];
                let written =
                    Everything_GetResultFullPathNameW(i, buffer.as_mut_ptr(), buffer.len() as u32);
                if written != size {
                    return Err("Everything 索引读取不完整，稍后自动重试。".into());
                }
                let path =
                    String::from_utf16(&buffer[..size as usize]).map_err(|e| e.to_string())?;
                entries.push(IndexedPath {
                    path,
                    folder: Everything_IsFolderResult(i) != 0,
                });
            }
            if Everything_IsDBLoaded() == 0 {
                return Err(unavailable());
            }
            Ok(entries)
        }
    }
}

/// Run before Tauri (and its single-instance handler). Only this worker accesses SDK globals.
pub fn run_worker() -> bool {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--everything-query")) {
        return false;
    }
    let Some(query) = args.next() else {
        return true;
    };
    let Some(destination) = args.next() else {
        return true;
    };
    #[cfg(windows)]
    let result = sdk::query(&query.to_string_lossy());
    #[cfg(not(windows))]
    let result: Result<Vec<IndexedPath>, String> = {
        let _ = query;
        Err(unavailable())
    };
    let response = match result {
        Ok(entries) => QueryOutput {
            entries,
            error: None,
        },
        Err(error) => QueryOutput {
            entries: Vec::new(),
            error: Some(error),
        },
    };
    if let Ok(file) = std::fs::File::create(destination) {
        let mut writer = std::io::BufWriter::new(file);
        let _ = serde_json::to_writer(&mut writer, &response);
        let _ = writer.flush();
    }
    true
}

struct TemporaryOutput(PathBuf);
impl Drop for TemporaryOutput {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn query_index(roots: &[FolderRoot]) -> Result<Vec<IndexedPath>, String> {
    let result_path = TemporaryOutput(std::env::temp_dir().join(format!(
        "nexious-everything-{}-{}.json",
        std::process::id(),
        super::now_millis()
    )));
    let mut command =
        std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command
        .arg("--everything-query")
        .arg(root_query(roots))
        .arg(&result_path.0);
    super::hide_console(&mut command);
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => return Err(unavailable()),
            Ok(None) if start.elapsed() < std::time::Duration::from_secs(30) => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Everything 响应超时，已有文件数据已保留，稍后自动重试。".into());
            }
        }
    }
    let file = std::fs::File::open(&result_path.0).map_err(|_| unavailable())?;
    let response: QueryOutput = serde_json::from_reader(std::io::BufReader::new(file))
        .map_err(|_| "Everything 索引读取不完整，已有文件数据已保留。".to_string())?;
    if let Some(error) = response.error {
        return Err(error);
    }
    Ok(response.entries)
}

fn to_items(roots: &[FolderRoot], entries: Vec<IndexedPath>) -> Vec<SyncedItem> {
    entries
        .into_iter()
        .filter_map(|entry| {
            let root = roots.iter().find(|root| {
                super::path_is_within(&entry.path, &root.path)
                    && (root.recursive || entry.path.eq_ignore_ascii_case(&root.path))
            })?;
            let name = Path::new(&entry.path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.name.clone());
            Some(SyncedItem {
                name,
                alias: super::normalize_alias(&root.name),
                kind: if entry.folder { "folder" } else { "file" }.into(),
                url: entry.path,
                description: format!("来自{}", root.name),
                icon: String::new(),
            })
        })
        .collect()
}

pub fn scan(roots: &[FolderRoot]) -> Result<Vec<SyncedItem>, String> {
    if roots.is_empty() {
        return Ok(Vec::new());
    }
    // Validate roots only. Per-file metadata calls would undo the benefit of Everything.
    // An unavailable drive is not an empty snapshot and must never trigger mass deletion.
    let mut available = Vec::new();
    let mut root_items = Vec::new();
    for root in roots {
        let path = Path::new(&root.path);
        match std::fs::metadata(path) {
            Ok(meta) if meta.is_dir() => {
                available.push(root.clone());
                root_items.push(SyncedItem {
                    name: root.name.clone(),
                    alias: super::normalize_alias(&root.name),
                    kind: "folder".into(),
                    url: root.path.clone(),
                    description: "同步目录".into(),
                    icon: String::new(),
                });
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && path
                        .ancestors()
                        .last()
                        .is_some_and(|volume| volume.is_dir()) => {}
            _ => return Err("部分同步目录暂不可用，已有文件数据已保留。".into()),
        }
    }
    if !available.is_empty() {
        let entries = query_index(&available)?;
        // Empty results cannot establish that a configured folder is indexed by Everything.
        // Require the folder itself (drive roots are absent from Everything's results).
        for root in &available {
            if !entries
                .iter()
                .any(|entry| super::path_is_within(&entry.path, &root.path))
            {
                return Err("所选目录尚未被 Everything 索引，已有文件数据已保留。".into());
            }
        }
        root_items.extend(to_items(&available, entries));
    }
    Ok(root_items)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root(path: &str) -> FolderRoot {
        FolderRoot {
            path: path.into(),
            name: "Work".into(),
            recursive: true,
        }
    }
    #[test]
    fn query_treats_paths_as_literals_and_keeps_root_boundaries() {
        assert_eq!(
            root_query(&[root(r"C:\Work (2026)"), root(r"D:\")]),
            r"^(?:C:\\Work \(2026\)(?:\\|$)|D:(?:\\|$))"
        );
    }
    #[test]
    fn index_has_no_twenty_thousand_item_limit_and_honors_scope() {
        let roots = vec![root(r"C:\Work")];
        let mut entries = (0..25_001)
            .map(|i| IndexedPath {
                path: format!(r"C:\Work\文件 {i}.txt"),
                folder: false,
            })
            .collect::<Vec<_>>();
        entries.push(IndexedPath {
            path: r"C:\Work-other\private.txt".into(),
            folder: false,
        });
        let items = to_items(&roots, entries);
        assert_eq!(items.len(), 25_001);
        assert!(items.iter().all(|item| item.kind == "file"));
    }
    #[test]
    fn index_rename_and_delete_are_reconciled() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::super::init_schema(&conn).unwrap();
        let roots = vec![root(r"C:\Work")];
        for (name, added, removed) in [("before.txt", 1, 0), ("after.txt", 1, 1)] {
            let items = to_items(
                &roots,
                vec![IndexedPath {
                    path: format!(r"C:\Work\{name}"),
                    folder: false,
                }],
            )
            .into_iter()
            .map(super::super::synced_item)
            .collect::<Vec<_>>();
            let result =
                super::super::sync::reconcile(&conn, "folders", &items, |_| false).unwrap();
            assert_eq!((result.added, result.removed), (added, removed));
        }
        assert_eq!(
            super::super::sync::reconcile(&conn, "folders", &[], |_| false)
                .unwrap()
                .removed,
            1
        );
    }
}

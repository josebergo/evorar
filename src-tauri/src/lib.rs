//! evorar —— 纯静、轻快、漂亮的解压缩工具。
//!
//! 产品原则(plan-step.md §2.3):
//! 零运营请求、零后台任务、零捆绑;主程序除用户显式操作外不发起任何网络请求。

pub mod engine;

use engine::{CancelFlag, Progress};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

// ---------------- 任务管理 ----------------

static NEXT_TASK_ID: AtomicU32 = AtomicU32::new(1);

#[derive(Default)]
pub struct TaskRegistry {
    cancels: Mutex<HashMap<u32, CancelFlag>>,
}

fn emit_progress(app: &AppHandle, task: u32, p: &Progress) {
    let _ = app.emit(
        "task-progress",
        serde_json::json!({
            "taskId": task,
            "processedBytes": p.processed_bytes,
            "totalBytes": p.total_bytes,
            "currentFile": p.current_file,
            "filesDone": p.files_done,
            "filesTotal": p.files_total,
        }),
    );
}

fn emit_done(app: &AppHandle, task: u32, result: Result<engine::ExtractStats, String>) {
    let payload = match result {
        Ok(stats) => serde_json::json!({
            "taskId": task,
            "ok": true,
            "stats": {
                "files": stats.files,
                "dirs": stats.dirs,
                "bytes": stats.bytes,
                "outputDir": stats.output_dir,
            },
        }),
        Err(err) => serde_json::json!({
            "taskId": task,
            "ok": false,
            "error": err,
        }),
    };
    let _ = app.emit("task-done", payload);
}

// ---------------- 命令 ----------------

/// 打开压缩包:嗅探格式 + 列出条目 + 给出建议解压目录(智能解压)
#[tauri::command]
fn open_archive(path: String) -> Result<serde_json::Value, String> {
    let p = PathBuf::from(&path);
    let mut arc = engine::open_archive(&p).map_err(|e| e.to_string())?;
    let format = arc.format();
    let entries = arc.list().map_err(|e| e.to_string())?;
    let names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
    let tops = engine::smart::top_level_of(&names);
    let base = p.parent().unwrap_or(PathBuf::from(".").as_path()).to_path_buf();
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let suggested = engine::smart::smart_dest(&base, &stem, &tops);
    let file_count = entries.iter().filter(|e| !e.is_dir).count();
    let total_unpacked: u64 = entries.iter().map(|e| e.size).sum();
    Ok(serde_json::json!({
        "path": path,
        "format": format.display_name(),
        "entries": entries,
        "fileCount": file_count,
        "totalUnpacked": total_unpacked,
        "suggestedDir": suggested.display().to_string(),
        "singleTopDir": tops.len() == 1 && tops[0].is_dir,
    }))
}

/// 判断一个路径是否是受支持的压缩包(拖放时区分「打开包」与「压缩」)
#[tauri::command]
fn is_archive(path: String) -> bool {
    let p = PathBuf::from(&path);
    engine::detect::guess_by_ext(&p).is_some() && engine::detect::sniff(&p).is_ok()
}

/// 启动解压任务(异步,进度走 `task-progress` 事件,结束走 `task-done`)
#[tauri::command]
fn extract_archive(
    app: AppHandle,
    registry: State<'_, TaskRegistry>,
    path: String,
    dest: String,
    password: Option<String>,
) -> Result<u32, String> {
    let p = PathBuf::from(&path);
    let mut arc = engine::open_archive(&p).map_err(|e| e.to_string())?;
    let task = NEXT_TASK_ID.fetch_add(1, Ordering::SeqCst);
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    registry.cancels.lock().insert(task, cancel.clone());
    let destp = PathBuf::from(dest);
    std::fs::create_dir_all(&destp).map_err(|e| format!("创建目标目录失败: {e}"))?;
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = arc.extract(
            &destp,
            password.as_deref(),
            &|p: &Progress| emit_progress(&app2, task, p),
            &cancel,
        )
        .map_err(|e| e.to_string());
        emit_done(&app2, task, result);
    });
    Ok(task)
}

/// 压缩格式选项
#[derive(Debug, Clone, serde::Deserialize)]
struct CompressArgs {
    sources: Vec<String>,
    dest: String,
    format: String, // "zip" | "7z"
    level: String,  // "fast" | "normal" | "max"
}

#[tauri::command]
fn compress_files(
    app: AppHandle,
    registry: State<'_, TaskRegistry>,
    args: CompressArgs,
    password: Option<String>,
) -> Result<u32, String> {
    let sources: Vec<PathBuf> = args.sources.iter().map(PathBuf::from).collect();
    let dest = PathBuf::from(&args.dest);
    let task = NEXT_TASK_ID.fetch_add(1, Ordering::SeqCst);
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    registry.cancels.lock().insert(task, cancel.clone());
    let app2 = app.clone();
    let fmt = args.format.clone();
    let level = args.level.clone();
    let _ = password; // ZIP/7Z 加密创建为 P1 功能,接口已预留
    tauri::async_runtime::spawn_blocking(move || {
        let result = (|| -> std::io::Result<engine::ExtractStats> {
            match (fmt.as_str(), level.as_str()) {
                ("zip", "fast") => engine::zipfmt::compress_zip(&sources, &dest, engine::zipfmt::ZipLevel::Fast, &|p| emit_progress(&app2, task, p), &cancel),
                ("zip", "max") => engine::zipfmt::compress_zip(&sources, &dest, engine::zipfmt::ZipLevel::Max, &|p| emit_progress(&app2, task, p), &cancel),
                ("zip", _) => engine::zipfmt::compress_zip(&sources, &dest, engine::zipfmt::ZipLevel::Normal, &|p| emit_progress(&app2, task, p), &cancel),
                ("7z", "fast") => engine::sevenz::compress_7z(&sources, &dest, engine::sevenz::SevenZLevel::Fast, &|p| emit_progress(&app2, task, p), &cancel),
                ("7z", "max") => engine::sevenz::compress_7z(&sources, &dest, engine::sevenz::SevenZLevel::Max, &|p| emit_progress(&app2, task, p), &cancel),
                ("7z", _) => engine::sevenz::compress_7z(&sources, &dest, engine::sevenz::SevenZLevel::Normal, &|p| emit_progress(&app2, task, p), &cancel),
                _ => Err(std::io::Error::other("不支持的压缩格式")),
            }
        })()
        .map_err(|e| e.to_string());
        emit_done(&app2, task, result);
    });
    Ok(task)
}

#[tauri::command]
fn cancel_task(registry: State<'_, TaskRegistry>, task_id: u32) -> bool {
    if let Some(flag) = registry.cancels.lock().get(&task_id) {
        flag.store(true, Ordering::Relaxed);
        true
    } else {
        false
    }
}

/// 打开目录(资源管理器)—— 解压完成后「打开文件夹」按钮
#[tauri::command]
async fn reveal_path(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err("路径不存在".into());
    }
    let arg = p.as_os_str().to_string_lossy().into_owned();
    std::process::Command::new("explorer")
        .arg(&arg)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 用系统默认浏览器打开外部链接(如 GitHub 仓库)
#[tauri::command]
async fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("仅允许 http(s) 链接".into());
    }
    std::process::Command::new("cmd")
        .args(["/C", "start", "", &url])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 应用版本(关于弹窗展示)
#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(TaskRegistry::default())
        .invoke_handler(tauri::generate_handler![
            open_archive,
            is_archive,
            extract_archive,
            compress_files,
            cancel_task,
            reveal_path,
            open_url,
            app_version,
        ])
        .run(tauri::generate_context!())
        .expect("evorar 启动失败");
}

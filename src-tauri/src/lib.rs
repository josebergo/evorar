//! evorar —— 纯静、轻快、漂亮的解压缩工具。
//!
//! 产品原则(plan-step.md §2.3):
//! 零运营请求、零后台任务、零捆绑;主程序除用户显式操作外不发起任何网络请求。

pub mod engine;
pub mod shell;

use engine::{CancelFlag, Progress};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

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

/// 应用版本(关于弹窗展示)
#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ---------------- 启动参数(文件关联 / 右键菜单回调) ----------------

/// 资源管理器 verb 与文件关联都以命令行方式回调主程序:
/// 进度仍在主窗口呈现且可取消,不做无界面的后台任务。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", content = "paths", rename_all = "kebab-case")]
pub enum LaunchAction {
    Open(Vec<String>),
    Extract(Vec<String>),
    ExtractHere(Vec<String>),
    Compress(Vec<String>),
    CompressZip(Vec<String>),
    Compress7z(Vec<String>),
}

/// 第一个 `--xxx` 识别为动作,其余非 `-` 开头的参数都视为路径
fn parse_launch_args(args: &[String]) -> Option<LaunchAction> {
    let mut flag: Option<String> = None;
    let mut paths = Vec::new();
    for a in args {
        if flag.is_none() && a.starts_with("--") {
            flag = Some(a.clone());
        } else if !a.starts_with('-') && !a.is_empty() {
            paths.push(a.clone());
        }
    }
    if paths.is_empty() {
        return None;
    }
    Some(match flag.as_deref() {
        None | Some("--open") => LaunchAction::Open(paths),
        Some("--extract") => LaunchAction::Extract(paths),
        Some("--extract-here") => LaunchAction::ExtractHere(paths),
        Some("--compress") => LaunchAction::Compress(paths),
        Some("--compress-zip") => LaunchAction::CompressZip(paths),
        Some("--compress-7z") => LaunchAction::Compress7z(paths),
        Some(_) => LaunchAction::Open(paths),
    })
}

/// 首次启动携带的动作;页面加载完成后由前端 `get_pending_launch` 取走
/// (setup 时直接 emit 会早于前端 listener 注册而丢失)
#[derive(Default)]
pub struct PendingLaunch(Mutex<Option<LaunchAction>>);

#[tauri::command]
fn get_pending_launch(state: State<'_, PendingLaunch>) -> Option<LaunchAction> {
    state.0.lock().clone()
}

#[tauri::command]
fn path_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}

#[tauri::command]
fn shell_integration_get() -> serde_json::Value {
    let s = shell::status();
    serde_json::json!({ "association": s.association, "contextMenu": s.context_menu })
}

#[tauri::command]
fn shell_integration_set(association: bool, context_menu: bool) -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("无法定位程序路径:{e}"))?
        .to_string_lossy()
        .into_owned();
    shell::apply(&exe, shell::IntegrationFlags { association, context_menu })
        .map_err(|e| format!("写入注册表失败:{e}"))
}

pub fn run() {
    let initial = parse_launch_args(
        &std::env::args_os()
            .skip(1)
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
    );
    tauri::Builder::default()
        // 单实例:已运行时聚焦主窗口,并转发新实例的启动参数
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
            let rest: Vec<String> = args.iter().skip(1).cloned().collect();
            if let Some(action) = parse_launch_args(&rest) {
                let _ = app.emit("launch-action", &action);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .manage(TaskRegistry::default())
        .manage(PendingLaunch(Mutex::new(initial)))
        .invoke_handler(tauri::generate_handler![
            open_archive,
            is_archive,
            extract_archive,
            compress_files,
            cancel_task,
            reveal_path,
            app_version,
            get_pending_launch,
            path_exists,
            shell_integration_get,
            shell_integration_set,
        ])
        .run(tauri::generate_context!())
        .expect("evorar 启动失败");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_flags_and_paths() {
        let a = parse_launch_args(&["--extract".into(), r"C:\a b\x.zip".into()]).unwrap();
        assert!(matches!(a, LaunchAction::Extract(p) if p == [r"C:\a b\x.zip"]));

        let a = parse_launch_args(&[r"D:\y.7z".into()]).unwrap();
        assert!(matches!(a, LaunchAction::Open(_)));

        // 未知 flag 按打开处理
        let a = parse_launch_args(&["--future".into(), "z.zip".into()]).unwrap();
        assert!(matches!(a, LaunchAction::Open(_)));

        assert!(parse_launch_args(&["--extract".into()]).is_none());
        assert!(parse_launch_args(&[]).is_none());
    }
}

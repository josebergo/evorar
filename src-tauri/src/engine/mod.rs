//! evorar 压缩引擎:格式无关的统一抽象层。
//!
//! 设计原则(来自 plan-step.md):
//! - 引擎与 UI 彻底分离,所有重活可在任意线程执行
//! - 流式处理,内存占用与包大小无关
//! - 进度可观测、任务可取消
//! - 解压时自行控制写出路径(防 zip-slip 路径穿越)

pub mod detect;
pub mod rar;
pub mod sevenz;
pub mod smart;
pub mod tarball;
pub mod zipfmt;

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// 压缩包内一个条目的信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct EntryInfo {
    /// 修正编码后的显示名(包内相对路径)
    pub name: String,
    pub size: u64,
    pub compressed_size: u64,
    pub is_dir: bool,
    pub modified: Option<u64>,
}

/// 引擎支持的归档格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Format {
    Zip,
    SevenZ,
    Rar,
    Tar,
    Gzip,
    Bzip2,
    Xz,
}

impl Format {
    pub fn display_name(&self) -> &'static str {
        match self {
            Format::Zip => "ZIP",
            Format::SevenZ => "7Z",
            Format::Rar => "RAR",
            Format::Tar => "TAR",
            Format::Gzip => "GZIP",
            Format::Bzip2 => "BZIP2",
            Format::Xz => "XZ",
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

/// 进度快照(通过 Tauri 事件推送到前端)
#[derive(Debug, Clone, serde::Serialize)]
pub struct Progress {
    pub processed_bytes: u64,
    pub total_bytes: u64,
    pub current_file: String,
    pub files_done: u32,
    pub files_total: u32,
}

/// 解压结果统计
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ExtractStats {
    pub files: u32,
    pub dirs: u32,
    pub bytes: u64,
    /// 智能解压最终落地的目录
    pub output_dir: String,
}

pub type CancelFlag = Arc<AtomicBool>;
pub type ProgressCb<'a> = &'a dyn Fn(&Progress);

pub(crate) fn check_cancel(cancel: &CancelFlag) -> std::io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(std::io::Error::other("已取消"))
    } else {
        Ok(())
    }
}

/// 归档引擎统一接口
pub trait ArchiveEngine {
    fn format(&self) -> Format;
    fn list(&mut self) -> std::io::Result<Vec<EntryInfo>>;
    /// 解压到 dest;若 smart=true,由上层决定 dest(见 smart 模块),引擎只负责写出。
    fn extract(
        &mut self,
        dest: &Path,
        password: Option<&str>,
        progress: ProgressCb<'_>,
        cancel: &CancelFlag,
    ) -> std::io::Result<ExtractStats>;
}

/// 根据文件头嗅探格式并构造对应引擎
pub fn open_archive(path: &Path) -> std::io::Result<Box<dyn ArchiveEngine + Send>> {
    let fmt = detect::sniff(path)?;
    match fmt {
        Format::Zip => Ok(Box::new(zipfmt::ZipEngine::open(path)?)),
        Format::SevenZ => Ok(Box::new(sevenz::SevenZEngine::open(path)?)),
        Format::Rar => Ok(Box::new(rar::RarEngine::open(path)?)),
        Format::Tar | Format::Gzip | Format::Bzip2 | Format::Xz => {
            Ok(Box::new(tarball::TarballEngine::open(path, fmt)?))
        }
    }
}

/// 工具:压缩包里常见的目录条目判断(zip 用 `/` 结尾,tar 里可能是实体目录)
pub(crate) fn is_dir_name(name: &str) -> bool {
    name.ends_with('/') || name.ends_with('\\')
}

/// 工具:解包相对路径到目标目录(拒绝 `..`、绝对路径、盘符 —— 防 zip-slip)
pub(crate) fn safe_join(dest: &Path, name: &str) -> std::io::Result<PathBuf> {
    let name = name.trim_start_matches('/').trim_start_matches('\\');
    let rel = Path::new(name);
    for comp in rel.components() {
        match comp {
            std::path::Component::Normal(_) => {}
            _ => {
                return Err(std::io::Error::other(format!(
                    "包内含非法路径(已拦截,防路径穿越): {name}"
                )))
            }
        }
    }
    if rel.components().next().is_none() {
        return Err(std::io::Error::other("包内条目路径为空"));
    }
    // 盘符如 "C:" 在 Windows 上会被 Component::Prefix 捕获,上面已拦截
    Ok(dest.join(rel))
}

//! 7Z 引擎:解压(含加密包密码)与压缩(LZMA2)。
//! 依据 sevenz-rust 0.6 实际 API:SevenZReader::open(path, Password) + for_each_entries。

use crate::engine::{
    ArchiveEngine, CancelFlag, EntryInfo, ExtractStats, Format, Progress, ProgressCb, check_cancel,
};
use std::fs;
use std::path::{Path, PathBuf};

fn password_of(pw: Option<&str>) -> sevenz_rust::Password {
    match pw {
        Some(p) => sevenz_rust::Password::from(p),
        None => sevenz_rust::Password::empty(),
    }
}

pub struct SevenZEngine {
    path: PathBuf,
}

impl SevenZEngine {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

fn entries_of(path: &Path, password: Option<&str>) -> std::io::Result<Vec<sevenz_rust::SevenZArchiveEntry>> {
    let sz = sevenz_rust::SevenZReader::open(path, password_of(password))
        .map_err(|e| std::io::Error::other(format!("打开 7Z 失败: {e}")))?;
    Ok(sz.archive().files.clone())
}

impl ArchiveEngine for SevenZEngine {
    fn format(&self) -> Format {
        Format::SevenZ
    }

    fn list(&mut self) -> std::io::Result<Vec<EntryInfo>> {
        let entries = entries_of(&self.path, None)
            .or_else(|_| entries_of(&self.path, Some("")))?;
        Ok(entries
            .into_iter()
            .filter(|e| !e.name.is_empty())
            .map(|e| EntryInfo {
                name: crate::engine::normalize_name(&e.name.replace('\\', "/")),
                size: e.size,
                compressed_size: 0,
                is_dir: e.is_directory,
                modified: None,
            })
            .filter(|e| !crate::engine::junk_entry(&e.name))
            .collect())
    }

    fn extract(
        &mut self,
        dest: &Path,
        password: Option<&str>,
        progress: ProgressCb<'_>,
        cancel: &CancelFlag,
    ) -> std::io::Result<ExtractStats> {
        let entries = entries_of(&self.path, password)?;
        let files_total = entries.iter().filter(|e| !e.is_directory).count() as u32;
        let total_bytes: u64 = entries.iter().map(|e| e.size()).sum();
        let mut sz = sevenz_rust::SevenZReader::open(&self.path, password_of(password))
            .map_err(|e| std::io::Error::other(format!("打开 7Z 失败: {e}")))?;
        let mut stats = ExtractStats::default();
        let mut done = 0u32;
        let dest = dest.to_path_buf();
        sz.for_each_entries(|entry, reader| {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err(sevenz_rust::Error::other("已取消"));
                }
                let name = entry.name.replace('\\', "/");
                if entry.is_directory() || name.is_empty() || crate::engine::junk_entry(&name) {
                    return Ok(true);
                }
                let name = crate::engine::normalize_name(&name);
                let target = crate::engine::safe_join(&dest, &name)
                    .map_err(|e| sevenz_rust::Error::other(e.to_string()))?;
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| sevenz_rust::Error::other(e.to_string()))?;
                }
                progress(&Progress {
                    processed_bytes: stats.bytes,
                    total_bytes,
                    current_file: name,
                    files_done: done,
                    files_total,
                });
                let mut out = fs::File::create(&target)
                    .map_err(|e| sevenz_rust::Error::other(e.to_string()))?;
                std::io::copy(reader, &mut out)
                    .map_err(|e| sevenz_rust::Error::other(e.to_string()))?;
                stats.bytes += entry.size();
                stats.files += 1;
                done += 1;
                Ok(true)
            })
            .map_err(|e| {
                if e.to_string().contains("已取消") {
                    std::io::Error::other("已取消")
                } else {
                    std::io::Error::other(format!("解压 7Z 失败: {e}"))
                }
            })?;
        stats.output_dir = dest.display().to_string();
        Ok(stats)
    }
}

// ---------------- 7Z 创建 ----------------

#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub enum SevenZLevel {
    Fast,
    Normal,
    Max,
}

pub fn compress_7z(
    sources: &[PathBuf],
    dest: &Path,
    _level: SevenZLevel, // LZMA2 字典大小调优为 P1;MVP 用默认档
    progress: ProgressCb<'_>,
    cancel: &CancelFlag,
) -> std::io::Result<ExtractStats> {
    let mut zw = sevenz_rust::SevenZWriter::create(dest)
        .map_err(|e| std::io::Error::other(format!("创建 7Z 失败: {e}")))?;
    let mut stats = ExtractStats::default();
    for src in sources {
        check_cancel(cancel)?;
        if src.is_dir() {
            let base = src.parent().unwrap_or(src);
            for entry in walkdir::WalkDir::new(src).follow_links(false) {
                check_cancel(cancel)?;
                let entry = entry?;
                let rel = entry.path().strip_prefix(base).unwrap_or(entry.path());
                let rel_name = rel.to_string_lossy().replace('\\', "/");
                if rel_name.is_empty() {
                    continue;
                }
                progress(&Progress {
                    processed_bytes: stats.bytes,
                    total_bytes: 0,
                    current_file: rel_name.clone(),
                    files_done: stats.files,
                    files_total: 0,
                });
                let szentry = sevenz_rust::SevenZArchiveEntry::from_path(entry.path(), rel_name);
                if entry.file_type().is_dir() {
                    zw.push_archive_entry::<fs::File>(szentry, None)
                        .map_err(|e| std::io::Error::other(format!("写入 7Z 失败: {e}")))?;
                } else {
                    let mut input = fs::File::open(entry.path())?;
                    zw.push_archive_entry(szentry, Some(&mut input))
                        .map_err(|e| std::io::Error::other(format!("写入 7Z 失败: {e}")))?;
                    stats.bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
                    stats.files += 1;
                }
            }
        } else {
            let name = src
                .file_name()
                .ok_or_else(|| std::io::Error::other("无效的源路径"))?
                .to_string_lossy()
                .replace('\\', "/");
            progress(&Progress {
                processed_bytes: stats.bytes,
                total_bytes: 0,
                current_file: name.clone(),
                files_done: stats.files,
                files_total: 0,
            });
            let szentry = sevenz_rust::SevenZArchiveEntry::from_path(src, name);
            let mut input = fs::File::open(src)?;
            zw.push_archive_entry(szentry, Some(&mut input))
                .map_err(|e| std::io::Error::other(format!("写入 7Z 失败: {e}")))?;
            stats.files += 1;
            stats.bytes += input.metadata()?.len();
        }
    }
    zw.finish()
        .map_err(|e| std::io::Error::other(format!("完成 7Z 失败: {e}")))?;
    stats.output_dir = dest.display().to_string();
    Ok(stats)
}

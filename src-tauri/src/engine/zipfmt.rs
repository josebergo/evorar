//! ZIP 引擎:解压(自带 GBK 文件名修复 + 路径穿越防护)与压缩。
//!
//! zip crate 把文件名解析为 String(UTF-8 或 cp437),中文 GBK 名会乱码,
//! 因此我们读取 `name_raw()` 原始字节自行解码,并用自己计算的路径写出。

use crate::engine::detect::fix_zip_name;
use crate::engine::{
    ArchiveEngine, CancelFlag, EntryInfo, ExtractStats, Format, Progress, ProgressCb,
    check_cancel, is_dir_name, safe_join,
};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub struct ZipEngine {
    path: PathBuf,
    /// (显示名, 原始名字节, 是否目录) —— list/extract 共用一次目录扫描
    entries: Vec<(String, Vec<u8>, bool)>,
    total_bytes: u64,
}

impl ZipEngine {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let f = fs::File::open(path)?;
        let mut za = zip::ZipArchive::new(f)
            .map_err(|e| std::io::Error::other(format!("打开 ZIP 失败: {e}")))?;
        let mut entries = Vec::with_capacity(za.len() as usize);
        let mut total = 0u64;
        for i in 0..za.len() {
            let zf = za
                .by_index(i)
                .map_err(|e| std::io::Error::other(format!("读取 ZIP 条目失败: {e}")))?;
            let raw = zf.name_raw().to_vec();
            let name = fix_zip_name(&raw);
            let is_dir = zf.is_dir() || is_dir_name(&name);
            if !is_dir {
                total += zf.size();
            }
            entries.push((name, raw, is_dir));
        }
        Ok(Self {
            path: path.to_path_buf(),
            entries,
            total_bytes: total,
        })
    }
}

impl ArchiveEngine for ZipEngine {
    fn format(&self) -> Format {
        Format::Zip
    }

    fn list(&mut self) -> std::io::Result<Vec<EntryInfo>> {
        // 重新打开拿元数据(size 等)
        let f = fs::File::open(&self.path)?;
        let mut za = zip::ZipArchive::new(f)
            .map_err(|e| std::io::Error::other(format!("打开 ZIP 失败: {e}")))?;
        let mut out = Vec::with_capacity(self.entries.len());
        for i in 0..za.len() {
            let zf = za
                .by_index(i)
                .map_err(|e| std::io::Error::other(format!("读取 ZIP 条目失败: {e}")))?;
            out.push(EntryInfo {
                name: fix_zip_name(zf.name_raw()),
                size: zf.size(),
                compressed_size: zf.compressed_size(),
                is_dir: zf.is_dir(),
                modified: zf.last_modified().map(|t| {
                    // 转成简易 unix 秒(仅展示用)
                    t.year() as u64 * 31_536_000
                }),
            });
        }
        Ok(out)
    }

    fn extract(
        &mut self,
        dest: &Path,
        password: Option<&str>,
        progress: ProgressCb<'_>,
        cancel: &CancelFlag,
    ) -> std::io::Result<ExtractStats> {
        let f = fs::File::open(&self.path)?;
        let mut za = zip::ZipArchive::new(f)
            .map_err(|e| std::io::Error::other(format!("打开 ZIP 失败: {e}")))?;
        if za.is_empty() {
            return Err(std::io::Error::other("压缩包为空"));
        }
        let files_total = self.entries.iter().filter(|e| !e.2).count() as u32;
        let mut stats = ExtractStats::default();
        let mut done = 0u32;
        let mut processed = 0u64;
        for i in 0..za.len() {
            check_cancel(cancel)?;
            let mut zf = match za.by_index_decrypt(i, password.unwrap_or("").as_bytes()) {
                Ok(zf) => zf,
                Err(zip::result::ZipError::InvalidPassword) => {
                    return Err(std::io::Error::other("该压缩包已加密,需要正确密码"));
                }
                Err(e) => {
                    return Err(std::io::Error::other(format!("读取条目失败: {e}")))
                }
            };
            let raw = zf.name_raw().to_vec();
            let name = fix_zip_name(&raw);
            let is_dir = zf.is_dir() || is_dir_name(&name);
            let target = safe_join(dest, &name)?;
            if is_dir {
                fs::create_dir_all(&target)?;
                stats.dirs += 1;
                continue;
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            progress(&Progress {
                processed_bytes: processed,
                total_bytes: self.total_bytes,
                current_file: name.clone(),
                files_done: done,
                files_total,
            });
            let mut out = fs::File::create(&target)?;
            let mut buf = [0u8; 256 * 1024];
            loop {
                check_cancel(cancel)?;
                let n = zf.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n])?;
                processed += n as u64;
            }
            done += 1;
            stats.files += 1;
            stats.bytes += zf.size();
        }
        stats.output_dir = dest.display().to_string();
        Ok(stats)
    }
}

// ---------------- 压缩(ZIP 创建) ----------------

/// 压缩级别(界面友好名 → deflate level)
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub enum ZipLevel {
    Fast,
    Normal,
    Max,
}

impl ZipLevel {
    fn deflate(self) -> i64 {
        match self {
            ZipLevel::Fast => 1,
            ZipLevel::Normal => 6,
            ZipLevel::Max => 9,
        }
    }
}

fn push_dir_entries(zw: &mut zip::ZipWriter<fs::File>, base: &Path, dir: &Path, opts: zip::write::SimpleFileOptions, progress: ProgressCb<'_>, cancel: &CancelFlag) -> std::io::Result<(u32, u64)> {
    let mut files = 0u32;
    let mut bytes = 0u64;
    for entry in walkdir::WalkDir::new(dir).follow_links(false) {
        check_cancel(cancel)?;
        let entry = entry?;
        let rel = entry
            .path()
            .strip_prefix(base)
            .unwrap_or(entry.path());
        let rel_name = rel
            .to_string_lossy()
            .replace('\\', "/");
        if rel_name.is_empty() {
            continue;
        }
        if entry.file_type().is_dir() {
            zw.add_directory(format!("{rel_name}/"), opts)
                .map_err(|e| std::io::Error::other(format!("写入目录项失败: {e}")))?;
            continue;
        }
        progress(&Progress {
            processed_bytes: 0,
            total_bytes: 0,
            current_file: rel_name.clone(),
            files_done: files,
            files_total: 0,
        });
        zw.start_file(rel_name, opts)
            .map_err(|e| std::io::Error::other(format!("写入文件项失败: {e}")))?;
        let mut input = fs::File::open(entry.path())?;
        std::io::copy(&mut input, zw)?;
        files += 1;
        bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
    }
    Ok((files, bytes))
}

/// 把一批文件/目录压缩为 ZIP
pub fn compress_zip(
    sources: &[PathBuf],
    dest: &Path,
    level: ZipLevel,
    progress: ProgressCb<'_>,
    cancel: &CancelFlag,
) -> std::io::Result<ExtractStats> {
    let file = fs::File::create(dest)?;
    let mut zw = zip::ZipWriter::new(file);
    let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(level.deflate()))
        .large_file(true);
    let mut stats = ExtractStats::default();
    for src in sources {
        let name = src
            .file_name()
            .ok_or_else(|| std::io::Error::other("无效的源路径"))?
            .to_string_lossy()
            .replace('\\', "/");
        if src.is_dir() {
            // 与 WinRAR/7-Zip 一致:压缩文件夹时保留顶层目录名(foo/…)
            let base = src.parent().unwrap_or(src);
            let (f, b) = push_dir_entries(&mut zw, base, src, opts, progress, cancel)?;
            stats.files += f;
            stats.bytes += b;
        } else {
            check_cancel(cancel)?;
            progress(&Progress {
                processed_bytes: 0,
                total_bytes: 0,
                current_file: name.clone(),
                files_done: stats.files,
                files_total: 0,
            });
            zw.start_file(name, opts)
                .map_err(|e| std::io::Error::other(format!("写入文件项失败: {e}")))?;
            let mut input = fs::File::open(src)?;
            std::io::copy(&mut input, &mut zw)?;
            stats.files += 1;
            stats.bytes += input.metadata()?.len();
        }
    }
    zw.finish()
        .map_err(|e| std::io::Error::other(format!("完成 ZIP 失败: {e}")))?;
    stats.output_dir = dest.display().to_string();
    Ok(stats)
}

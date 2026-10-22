//! RAR/RAR5 解压引擎(基于 unrar crate,绑定 RarLab 官方 unrar 源码)。
//! 仅解压 —— RAR 压缩算法为 RarLab 专有,法律上第三方不可创建(见 plan-step.md §3.1)。
//! unrar 0.5 API:Archive::new(&path).open_for_listing() / open_for_processing() 光标模式。

use crate::engine::{
    ArchiveEngine, CancelFlag, EntryInfo, ExtractStats, Format, Progress, ProgressCb,
};
use std::path::{Path, PathBuf};

pub struct RarEngine {
    path: PathBuf,
}

impl RarEngine {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    /// 列出条目(预扫描,用于进度基数与界面列表)
    fn listing(&self, password: Option<&str>) -> std::io::Result<Vec<(String, u64, bool)>> {
        let archive = if let Some(pw) = password {
            unrar::Archive::with_password(&self.path, pw)
        } else {
            unrar::Archive::new(&self.path)
        };
        let iter = archive
            .open_for_listing()
            .map_err(|e| std::io::Error::other(format!("打开 RAR 失败: {e}")))?;
        let mut out = Vec::new();
        for e in iter {
            let entry = e.map_err(|err| std::io::Error::other(format!("读取 RAR 目录失败: {err}")))?;
            let name = crate::engine::normalize_name(
                &entry.filename.to_string_lossy().replace('\\', "/"),
            );
            if crate::engine::junk_entry(&name) {
                continue;
            }
            out.push((name, entry.unpacked_size, entry.is_file()));
        }
        Ok(out)
    }
}

impl ArchiveEngine for RarEngine {
    fn format(&self) -> Format {
        Format::Rar
    }

    fn list(&mut self) -> std::io::Result<Vec<EntryInfo>> {
        let items = self
            .listing(None)
            .or_else(|_| self.listing(Some("")))?;
        Ok(items
            .into_iter()
            .map(|(name, size, is_file)| EntryInfo {
                name,
                size,
                compressed_size: 0,
                is_dir: !is_file,
                modified: None,
            })
            .collect())
    }

    fn extract(
        &mut self,
        dest: &Path,
        password: Option<&str>,
        progress: ProgressCb<'_>,
        cancel: &CancelFlag,
    ) -> std::io::Result<ExtractStats> {
        let listing = self.listing(password)?;
        let files_total = listing.iter().filter(|(_, _, f)| *f).count() as u32;
        let total_bytes: u64 = listing.iter().map(|(_, s, _)| s).sum();
        let mut stats = ExtractStats::default();
        let mut done = 0u32;

        let archive = if let Some(pw) = password {
            unrar::Archive::with_password(&self.path, pw)
        } else {
            unrar::Archive::new(&self.path)
        };
        let mut cursor = archive
            .open_for_processing()
            .map_err(|e| std::io::Error::other(format!("打开 RAR 失败: {e}")))?;
        loop {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(std::io::Error::other("已取消"));
            }
            let Some(header) = cursor
                .read_header()
                .map_err(|e| std::io::Error::other(format!("读取 RAR 失败: {e}")))?
            else {
                break;
            };
            // 先把 header 里的信息拷出,避免 entry 借用阻碍 header 的移动(extract/skip 消费 header)
            let (entry_name, unpacked, is_file) = {
                let entry = header.entry();
                (
                    crate::engine::normalize_name(
                        &entry.filename.to_string_lossy().replace('\\', "/"),
                    ),
                    entry.unpacked_size,
                    entry.is_file(),
                )
            };
            if crate::engine::junk_entry(&entry_name) {
                cursor = header
                    .skip()
                    .map_err(|e| std::io::Error::other(format!("跳过条目失败: {e}")))?;
                continue;
            }
            progress(&Progress {
                processed_bytes: stats.bytes,
                total_bytes,
                current_file: entry_name,
                files_done: done,
                files_total,
            });
            if is_file {
                cursor = header
                    .extract()
                    .map_err(|e| std::io::Error::other(format!("解压 RAR 失败: {e}")))?;
                stats.bytes += unpacked;
                stats.files += 1;
                done += 1;
            } else {
                cursor = header
                    .skip()
                    .map_err(|e| std::io::Error::other(format!("跳过目录项失败: {e}")))?;
                stats.dirs += 1;
            }
        }
        stats.output_dir = dest.display().to_string();
        Ok(stats)
    }
}

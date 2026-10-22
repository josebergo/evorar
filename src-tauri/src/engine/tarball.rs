//! TAR 家族引擎:tar / tar.gz / tar.bz2 / tar.xz,以及裸 .gz/.bz2/.xz 单文件解压。
//!
//! 7z.dll 生态里这些格式统一走流式解包;这里用 tar crate + 解码器组合。

use crate::engine::{
    ArchiveEngine, CancelFlag, EntryInfo, ExtractStats, Format, Progress, ProgressCb,
    check_cancel, safe_join,
};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub struct TarballEngine {
    path: PathBuf,
    fmt: Format,
}

impl TarballEngine {
    pub fn open(path: &Path, fmt: Format) -> std::io::Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
            fmt,
        })
    }

    /// 构造解包流:tar 源 = 原文件;gz/bz2/xz 源 = 解码后的 tar 或单文件
    fn open_stream(&self) -> std::io::Result<Box<dyn Read + Send>> {
        let f = fs::File::open(&self.path)?;
        Ok(match self.fmt {
            Format::Tar => Box::new(f),
            Format::Gzip => Box::new(flate2::read::GzDecoder::new(f)),
            Format::Bzip2 => Box::new(bzip2::read::BzDecoder::new(f)),
            Format::Xz => Box::new(xz2::read::XzDecoder::new(f)),
            _ => unreachable!(),
        })
    }

    /// 判断解码后的内容是否为 tar(gz/xz/bz2 可能是单文件压缩)
    fn sniff_inner_tar(&self) -> std::io::Result<bool> {
        if self.fmt == Format::Tar {
            return Ok(true);
        }
        use std::io::Seek;
        let mut f = fs::File::open(&self.path)?;
        f.seek(std::io::SeekFrom::Start(0))?;
        let mut dec: Box<dyn Read> = match self.fmt {
            Format::Gzip => Box::new(flate2::read::GzDecoder::new(f)),
            Format::Bzip2 => Box::new(bzip2::read::BzDecoder::new(f)),
            Format::Xz => Box::new(xz2::read::XzDecoder::new_multi_decoder(f)),
            _ => unreachable!(),
        };
        let mut buf = [0u8; 262];
        let n = read_fill(&mut dec, &mut buf)?;
        Ok(n >= 262 && &buf[257..262] == b"ustar")
    }
}

fn read_fill<R: Read>(r: &mut R, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut read = 0;
    while read < buf.len() {
        let k = r.read(&mut buf[read..])?;
        if k == 0 {
            break;
        }
        read += k;
    }
    Ok(read)
}

impl TarballEngine {
    fn is_single_file_mode(&self) -> std::io::Result<bool> {
        if self.fmt == Format::Tar {
            return Ok(false);
        }
        self.sniff_inner_tar().map(|is_tar| !is_tar)
    }

    /// 单文件解压:xxx.txt.gz → dest/xxx.txt
    fn extract_single(
        &self,
        dest: &Path,
        progress: ProgressCb<'_>,
        cancel: &CancelFlag,
    ) -> std::io::Result<ExtractStats> {
        let stem = self
            .path
            .file_stem()
            .ok_or_else(|| std::io::Error::other("无法确定解压后文件名"))?
            .to_string_lossy()
            .into_owned();
        let target = dest.join(&stem);
        fs::create_dir_all(dest)?;
        let mut input = self.open_stream()?;
        let mut out = fs::File::create(&target)?;
        progress(&Progress {
            processed_bytes: 0,
            total_bytes: 0,
            current_file: stem.clone(),
            files_done: 0,
            files_total: 1,
        });
        let mut buf = [0u8; 256 * 1024];
        let mut bytes = 0u64;
        loop {
            check_cancel(cancel)?;
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            bytes += n as u64;
        }
        Ok(ExtractStats {
            files: 1,
            dirs: 0,
            bytes,
            output_dir: dest.display().to_string(),
        })
    }
}

impl ArchiveEngine for TarballEngine {
    fn format(&self) -> Format {
        self.fmt
    }

    fn list(&mut self) -> std::io::Result<Vec<EntryInfo>> {
        if self.is_single_file_mode()? {
            let stem = self
                .path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            return Ok(vec![EntryInfo {
                name: stem,
                size: 0,
                compressed_size: self.path.metadata().map(|m| m.len()).unwrap_or(0),
                is_dir: false,
                modified: None,
            }]);
        }
        let stream = self.open_stream()?;
        let mut ar = tar::Archive::new(stream);
        let mut out = Vec::new();
        for entry in ar.entries()? {
            let e = entry?;
            let name = e
                .path()?
                .to_string_lossy()
                .replace('\\', "/");
            out.push(EntryInfo {
                name,
                size: e.header().size()?,
                compressed_size: 0,
                is_dir: e.header().entry_type().is_dir(),
                modified: e.header().mtime().ok(),
            });
        }
        Ok(out)
    }

    fn extract(
        &mut self,
        dest: &Path,
        _password: Option<&str>,
        progress: ProgressCb<'_>,
        cancel: &CancelFlag,
    ) -> std::io::Result<ExtractStats> {
        if self.is_single_file_mode()? {
            return self.extract_single(dest, progress, cancel);
        }
        let stream = self.open_stream()?;
        let mut ar = tar::Archive::new(stream);
        ar.set_preserve_permissions(false);
        ar.set_unpack_xattrs(false);
        // 先收集头部信息用于进度
        let mut stats = ExtractStats::default();
        let mut files_total = 0u32;
        let mut total_bytes = 0u64;
        {
            let stream2 = self.open_stream()?;
            let mut ar2 = tar::Archive::new(stream2);
            for e in ar2.entries()? {
                let e = e?;
                if !e.header().entry_type().is_dir() {
                    files_total += 1;
                    total_bytes += e.header().size()?;
                }
            }
        }
        let mut done = 0u32;
        let mut processed = 0u64;
        for entry in ar.entries()? {
            check_cancel(cancel)?;
            let mut e = entry?;
            let name = e
                .path()?
                .to_string_lossy()
                .replace('\\', "/");
            if e.header().entry_type().is_dir() {
                let target = safe_join(dest, &name)?;
                fs::create_dir_all(target)?;
                stats.dirs += 1;
                continue;
            }
            let target = safe_join(dest, &name)?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            progress(&Progress {
                processed_bytes: processed,
                total_bytes,
                current_file: name.clone(),
                files_done: done,
                files_total,
            });
            let mut out = fs::File::create(&target)?;
            std::io::copy(&mut e, &mut out)?;
            processed += e.header().size()?;
            done += 1;
            stats.files += 1;
            stats.bytes += e.header().size()?;
        }
        stats.output_dir = dest.display().to_string();
        Ok(stats)
    }
}

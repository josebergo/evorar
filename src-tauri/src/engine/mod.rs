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
    /// selection=None 全量解压;Some 时只解选中的条目(选中目录连带其下所有内容),
    /// 进度基数(files_total/total_bytes)同步按选中集合计算。
    fn extract(
        &mut self,
        dest: &Path,
        password: Option<&str>,
        selection: Option<&[String]>,
        progress: ProgressCb<'_>,
        cancel: &CancelFlag,
    ) -> std::io::Result<ExtractStats>;
}

/// 条目是否属于选中集合:名字精确匹配,或位于某个选中目录之下(保留包内相对路径)。
/// 目录名带不带尾斜杠、`\` 还是 `/`,都先归一再比;包内路径区分大小写。
pub(crate) fn entry_selected(name: &str, selection: Option<&[String]>) -> bool {
    let Some(sel) = selection else {
        return true;
    };
    let n = name
        .trim_matches(['/', '\\'])
        .replace('\\', "/");
    if n.is_empty() {
        return false;
    }
    sel.iter().any(|s| {
        let s = s.trim_matches(['/', '\\']).replace('\\', "/");
        !s.is_empty() && (n == s || n.starts_with(&format!("{s}/")))
    })
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

/// macOS 垃圾条目过滤(design-mac.md §5.1):Finder「压缩」产物里的元数据。
/// `__MACOSX/` 整棵跳过,`.DS_Store` 与 AppleDouble `._*` 文件跳过;Windows 同样受益。
pub(crate) fn junk_entry(name: &str) -> bool {
    let n = name.trim_start_matches(['/', '\\']);
    if n == "__MACOSX" || n.starts_with("__MACOSX/") {
        return true;
    }
    let base = n.rsplit(['/', '\\']).next().unwrap_or(n);
    base == ".DS_Store" || (base.starts_with("._") && base.len() > 2)
}

/// 条目名统一 NFC 规范化:老 macOS(HFS+ 时代)zip 常为 NFD 分解形,
/// 不归一会造成「看起来同名实为两个文件」(design-mac.md §5.2)
pub(crate) fn normalize_name(name: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    name.nfc().collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_junk_entries_detected() {
        // Finder「压缩」产物
        assert!(junk_entry("__MACOSX/dir/._meta"));
        assert!(junk_entry("__MACOSX"));
        assert!("/__MACOSX/a.txt" != "" && junk_entry("/__MACOSX/a.txt"));
        assert!(junk_entry("dir/.DS_Store"));
        assert!(junk_entry("docs/._cover.png"));
        // 正常条目不误伤
        assert!(!junk_entry("normal.txt"));
        assert!(!junk_entry("dir/.hidden"));
        assert!(!junk_entry("__MACOSX_like/readme.txt"));
        assert!(!junk_entry(".dotfile.cfg"));
    }

    #[test]
    fn nfc_normalization() {
        // NFD(e + 组合锐音符)→ NFC(单个 é)
        assert_eq!(normalize_name("cafe\u{301}.txt"), "caf\u{e9}.txt");
        assert_eq!(normalize_name("plain.txt"), "plain.txt");
    }

    #[test]
    fn selection_matching() {
        let sel = vec!["docs/".to_string(), "a.txt".to_string()];
        // None = 全量
        assert!(entry_selected("anything", None));
        // 精确匹配(选中名与条目名的尾斜杠风格互不影响)
        assert!(entry_selected("docs/", Some(&sel)));
        assert!(entry_selected("docs", Some(&sel)));
        assert!(entry_selected("a.txt", Some(&sel)));
        // 目录连带其下所有内容
        assert!(entry_selected("docs/sub/b.txt", Some(&sel)));
        assert!(entry_selected("docs\\sub\\b.txt", Some(&sel)));
        // 前缀同名不算子路径;未选中的跳过
        assert!(!entry_selected("docs2/b.txt", Some(&sel)));
        assert!(!entry_selected("b.txt", Some(&sel)));
        // Some(空) = 什么都不解
        assert!(!entry_selected("a.txt", Some(&[])));
    }
}

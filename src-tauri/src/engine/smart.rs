//! 智能解压策略(继承 360压缩 的核心体验):
//! - 包内只有一个顶层条目且是目录 → 直接解到当前目录(不再套娃)
//! - 其他情况 → 解到 `<压缩包同名文件夹>/`
//! 参考 plan-step.md §2.1「智能解压」与 UI 文案「一键解压到当前文件夹」。

use std::path::{Path, PathBuf};

/// 根据包内容决定解压目标目录。
/// `base_dir`:压缩包所在目录;`archive_stem`:压缩包主文件名。
pub fn smart_dest(base_dir: &Path, archive_stem: &str, top_level: &[TopEntry]) -> PathBuf {
    let single_dir = top_level.len() == 1 && top_level[0].is_dir;
    if single_dir {
        // 唯一顶层目录 → 解到压缩包所在目录(内容自然落在同名目录下,无套娃)
        base_dir.to_path_buf()
    } else {
        base_dir.join(sanitize_stem(archive_stem))
    }
}

#[derive(Debug, Clone)]
pub struct TopEntry {
    pub name: String,
    pub is_dir: bool,
}

/// 从完整条目列表计算顶层条目集合
pub fn top_level_of(entries: &[String]) -> Vec<TopEntry> {
    use std::collections::BTreeMap;
    let mut tops: BTreeMap<String, TopEntry> = BTreeMap::new();
    for name in entries {
        let normalized = name.replace('\\', "/");
        let first = normalized.split('/').find(|s| !s.is_empty());
        let Some(first) = first else { continue };
        let is_top_dir = normalized.split('/').filter(|s| !s.is_empty()).count() > 1
            || normalized.ends_with('/');
        let entry = tops.entry(first.to_string()).or_insert(TopEntry {
            name: first.to_string(),
            is_dir: false,
        });
        if is_top_dir {
            entry.is_dir = true;
        }
    }
    tops.into_values().collect()
}

/// 目录名消毒:去掉当前平台文件系统非法字符,防止解压出畸形目录名
/// (Windows 禁 `\ / : * ? " < > |`;macOS 仅禁 `/ :`,Finder 显示为 `-`)
#[cfg(target_os = "windows")]
fn sanitize_stem(stem: &str) -> String {
    sanitize_with(stem, |c| matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
}

#[cfg(not(target_os = "windows"))]
fn sanitize_stem(stem: &str) -> String {
    sanitize_with(stem, |c| matches!(c, '/' | ':'))
}

fn sanitize_with<F: Fn(char) -> bool>(stem: &str, illegal: F) -> String {
    let cleaned: String = stem
        .chars()
        .map(|c| if illegal(c) { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        "evorar_extract".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_top_dir_no_nesting() {
        let entries = vec!["docs/".to_string(), "docs/a.txt".to_string(), "docs/b/".to_string()];
        let tops = top_level_of(&entries);
        assert_eq!(tops.len(), 1);
        assert!(tops[0].is_dir);
        let dest = smart_dest(Path::new("D:/out"), "包名", &tops);
        assert_eq!(dest, PathBuf::from("D:/out"));
    }

    #[test]
    fn multiple_top_entries_wrap() {
        let entries = vec!["a.txt".to_string(), "b.txt".to_string(), "c/".to_string()];
        let tops = top_level_of(&entries);
        assert_eq!(tops.len(), 3);
        let dest = smart_dest(Path::new("D:/out"), "我的压缩包", &tops);
        assert_eq!(dest, PathBuf::from("D:/out/我的压缩包"));
    }

    #[test]
    fn illegal_chars_sanitized() {
        let entries = vec!["a.txt".to_string(), "b.txt".to_string()];
        let tops = top_level_of(&entries);
        let dest = smart_dest(Path::new("D:/out"), "a:b?c", &tops);
        assert_eq!(dest, PathBuf::from("D:/out/a_b_c"));
    }
}

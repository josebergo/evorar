//! 格式嗅探(magic bytes)+ ZIP 文件名编码检测(GBK/UTF-8 自动修复)。
//!
//! 来自竞品探测结论:ZIP 中文乱码是中国用户最大痛点。
//! 策略:严格 UTF-8 校验失败 → 回退 GBK;GBK 也出现替换符 → 保留原始字节损失最小方案。

use crate::engine::Format;
use std::io::Read;
use std::path::Path;

/// 读取文件头用于嗅探
fn head(path: &Path, n: usize) -> std::io::Result<Vec<u8>> {
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; n];
    let mut read = 0;
    while read < n {
        let k = f.read(&mut buf[read..])?;
        if k == 0 {
            break;
        }
        read += k;
    }
    buf.truncate(read);
    Ok(buf)
}

fn head_at(path: &Path, offset: u64, n: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Seek;
    let mut f = std::fs::File::open(path)?;
    f.seek(std::io::SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; n];
    let mut read = 0;
    while read < n {
        let k = f.read(&mut buf[read..])?;
        if k == 0 {
            break;
        }
        read += k;
    }
    buf.truncate(read);
    Ok(buf)
}

/// 通过魔数判断归档格式
pub fn sniff(path: &Path) -> std::io::Result<Format> {
    let h = head(path, 8)?;
    if h.len() < 4 {
        return Err(std::io::Error::other("文件过小或无法读取"));
    }
    if &h[..4] == b"PK\x03\x04" || &h[..4] == b"PK\x05\x06" || &h[..4] == b"PK\x07\x08" {
        return Ok(Format::Zip);
    }
    if h.starts_with(b"7z\xbc\xaf\x27\x1c") {
        return Ok(Format::SevenZ);
    }
    if &h[..4] == b"Rar!" {
        // RAR4: 52 61 72 21 1A 07 00 / RAR5: 52 61 72 21 1A 07 01 00
        return Ok(Format::Rar);
    }
    if h.starts_with(b"\x1f\x8b") {
        return Ok(Format::Gzip);
    }
    if h.starts_with(b"BZh") {
        return Ok(Format::Bzip2);
    }
    if h.starts_with(b"\xfd7zXZ\x00") {
        return Ok(Format::Xz);
    }
    // ustar 魔数位于 257 偏移
    let m = head_at(path, 257, 5)?;
    if m.starts_with(b"ustar") {
        return Ok(Format::Tar);
    }
    // GNU tar 旧格式没有 ustar 魔数,做一次宽松校验:文件名区可打印 + 257 处全零
    if m.iter().all(|&b| b == 0) && h.iter().take(32).all(|&b| b == 0 || (0x20..=0x7e).contains(&b)) {
        return Ok(Format::Tar);
    }
    Err(std::io::Error::other("无法识别的压缩格式(支持 ZIP/7Z/RAR/TAR/GZ/BZ2/XZ)"))
}

/// ZIP 文件名字节 → 显示字符串:自动修复 GBK/UTF-8 乱码。
pub fn fix_zip_name(raw: &[u8]) -> String {
    // 1) 严格 UTF-8(zlib 的 UTF-8 flag 路径)
    if let Ok(s) = std::str::from_utf8(raw) {
        return s.to_string();
    }
    // 2) GBK 解码(国内 ZIP 主流编码,Windows 资源管理器压缩默认 GBK)
    let (decoded, _, had_errors) = encoding_rs::GBK.decode(raw);
    if !had_errors {
        return decoded.into_owned();
    }
    // 3) 双失败:退回无损替换(UTF-8 lossy 保留最大信息)
    String::from_utf8_lossy(raw).into_owned()
}

/// 文件扩展名对应的压缩包格式预判(用于界面提示,最终以魔数为准)
pub fn guess_by_ext(path: &Path) -> Option<Format> {
    let ext = path
        .extension()?
        .to_str()?
        .to_ascii_lowercase();
    let ext: &str = &ext;
    match ext {
        "zip" | "jar" | "apk" | "docx" | "xlsx" | "pptx" | "epub" => Some(Format::Zip),
        "7z" => Some(Format::SevenZ),
        "rar" | "rev" => Some(Format::Rar),
        "tar" => Some(Format::Tar),
        "gz" | "tgz" => Some(Format::Gzip),
        "bz2" | "tbz2" => Some(Format::Bzip2),
        "xz" | "txz" => Some(Format::Xz),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gbk_name_fixed() {
        // "文档.txt" 的 GBK 字节
        let gbk = [0xce, 0xc4, 0xb5, 0xb5, b'.', b't', b'x', b't'];
        assert_eq!(fix_zip_name(&gbk), "文档.txt");
        assert_eq!(fix_zip_name(b"ascii.txt"), "ascii.txt");
        let utf8 = "中文目录/文件.bin".as_bytes();
        assert_eq!(fix_zip_name(utf8), "中文目录/文件.bin");
    }

    #[test]
    fn safe_names() {
        assert!(crate::engine::safe_join(Path::new("d:/out"), "a/b.txt").is_ok());
        assert!(crate::engine::safe_join(Path::new("d:/out"), "../evil.txt").is_err());
        // 盘符是 Windows 概念(Components::Prefix);mac 上 "C:" 只是普通目录名
        #[cfg(target_os = "windows")]
        assert!(crate::engine::safe_join(Path::new("d:/out"), "C:/evil.txt").is_err());
        assert!(crate::engine::safe_join(Path::new("d:/out"), "/abs.txt").is_ok());
    }
}

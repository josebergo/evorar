//! 端到端引擎测试:创建 → 嗅探 → 解压 → 内容校验。
//! 运行:cargo test --test engine_e2e

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("evorar_test_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn sample_tree(base: &Path) {
    fs::write(base.join("hello.txt"), b"hello evorar").unwrap();
    fs::create_dir_all(base.join("docs")).unwrap();
    fs::write(base.join("docs").join("中文文档.txt"), "中文内容测试".as_bytes()).unwrap();
}

fn cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

fn noop_progress(_: &evorar_lib::engine::Progress) {}

fn stats_files(dir: &Path) -> Vec<String> {
    let mut v = Vec::new();
    for e in walkdir::WalkDir::new(dir) {
        let e = e.unwrap();
        if e.file_type().is_file() {
            v.push(e.path().strip_prefix(dir).unwrap().to_string_lossy().replace('\\', "/"));
        }
    }
    v.sort();
    v
}

#[test]
fn zip_roundtrip() {
    let src = tmpdir("zip_src");
    sample_tree(&src);
    let dest = tmpdir("zip_out").join("t.zip");
    evorar_lib::engine::zipfmt::compress_zip(
        &[src.clone()],
        &dest,
        evorar_lib::engine::zipfmt::ZipLevel::Normal,
        &noop_progress,
        &cancel(),
    )
    .unwrap();
    assert!(dest.exists());

    // 嗅探 + 解压
    let fmt = evorar_lib::engine::detect::sniff(&dest).unwrap();
    assert!(matches!(fmt, evorar_lib::engine::Format::Zip));
    let mut arc = evorar_lib::engine::open_archive(&dest).unwrap();
    let entries = arc.list().unwrap();
    assert!(entries.iter().any(|e| e.name.contains("中文文档.txt")), "中文文件名保留");
    let out = tmpdir("zip_extract");
    arc.extract(&out, None, &noop_progress, &cancel()).unwrap();
    let files = stats_files(&out);
    assert!(files.iter().any(|f| f.contains("hello.txt")));
    assert!(fs::read_to_string(out.join(src.file_name().unwrap()).join("docs").join("中文文档.txt"))
        .unwrap()
        .contains("中文内容测试"));
    let _ = fs::remove_dir_all(&src);
}

#[test]
fn sevenz_roundtrip() {
    let src = tmpdir("7z_src");
    sample_tree(&src);
    let dest = tmpdir("7z_out").join("t.7z");
    evorar_lib::engine::sevenz::compress_7z(
        &[src.clone()],
        &dest,
        evorar_lib::engine::sevenz::SevenZLevel::Normal,
        &noop_progress,
        &cancel(),
    )
    .unwrap();
    assert!(dest.exists());

    let fmt = evorar_lib::engine::detect::sniff(&dest).unwrap();
    assert!(matches!(fmt, evorar_lib::engine::Format::SevenZ));
    let mut arc = evorar_lib::engine::open_archive(&dest).unwrap();
    let entries = arc.list().unwrap();
    assert!(entries.iter().any(|e| e.name.contains("hello.txt")));
    let out = tmpdir("7z_extract");
    arc.extract(&out, None, &noop_progress, &cancel()).unwrap();
    assert!(stats_files(&out).iter().any(|f| f.contains("hello.txt")));
    let _ = fs::remove_dir_all(&src);
}

#[test]
fn cancel_flag_aborts_zip_extract() {
    let src = tmpdir("cancel_src");
    sample_tree(&src);
    let dest = tmpdir("cancel_out").join("t.zip");
    evorar_lib::engine::zipfmt::compress_zip(
        &[src],
        &dest,
        evorar_lib::engine::zipfmt::ZipLevel::Fast,
        &noop_progress,
        &cancel(),
    )
    .unwrap();
    let flag = Arc::new(AtomicBool::new(true)); // 立即取消
    let mut arc = evorar_lib::engine::open_archive(&dest).unwrap();
    let out = tmpdir("cancel_extract");
    let err = arc.extract(&out, None, &noop_progress, &flag).unwrap_err();
    assert!(err.to_string().contains("已取消"), "取消生效: {err}");
}

#[test]
fn gz_single_file_extract() {
    // 用 flate2 构造 xxx.txt.gz,验证单文件模式解出 xxx.txt
    let dir = tmpdir("gz");
    let raw = b"single file content";
    let gz = dir.join("note.txt.gz");
    let f = fs::File::create(&gz).unwrap();
    let mut enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
    use std::io::Write;
    enc.write_all(raw).unwrap();
    enc.finish().unwrap();

    let fmt = evorar_lib::engine::detect::sniff(&gz).unwrap();
    assert!(matches!(fmt, evorar_lib::engine::Format::Gzip));
    let mut arc = evorar_lib::engine::open_archive(&gz).unwrap();
    let out = tmpdir("gz_extract");
    arc.extract(&out, None, &noop_progress, &cancel()).unwrap();
    assert_eq!(fs::read(out.join("note.txt")).unwrap(), raw);
}

#[test]
fn gbk_zip_names_fixed() {
    let zip = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gbk.zip");
    let mut arc = evorar_lib::engine::open_archive(&zip).unwrap();
    let entries = arc.list().unwrap();
    assert!(
        entries.iter().any(|e| e.name.contains("文档.txt")),
        "GBK 文件名应被修复为中文,实际: {:?}",
        entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>()
    );
    let out = tmpdir("gbk_extract");
    arc.extract(&out, None, &noop_progress, &cancel()).unwrap();
    assert!(out.join("文档.txt").exists(), "解压出正确中文文件名");
}

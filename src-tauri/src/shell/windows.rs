//! Windows Shell 集成:文件关联 + 资源管理器右键菜单。
//!
//! 全部用 HKCU 注册表 verb 实现(免管理员、无 Shell 扩展 DLL、卸载零残留):
//! 菜单项命令行回调主程序,压缩/解压进度仍在主窗口呈现且可取消,
//! 不引入任何无界面的后台任务(对应 plan-step.md §2.3 产品原则)。
//!
//! 键位布局(HKCU):
//! - `Software\Classes\evorar.Archive`        ProgID:图标 + 打开/智能解压/解压到当前文件夹
//! - `Software\Classes\<.zip …>`              关联扩展名(默认值指向 ProgID,原值备份可还原)
//! - `Software\Classes\{*,Directory,Directory\Background}\shell\evorar.*`
//!                                            所有文件/文件夹/空白处的压缩菜单
//! - `Software\evorar\Shell`                  状态标记 + 关联前原值备份

use std::io;
use winreg::enums::{HKEY_CURRENT_USER, KEY_ALL_ACCESS, KEY_READ, KEY_WRITE};
use winreg::RegKey;

pub const PROG_ID: &str = "evorar.Archive";

/// 纳入文件关联的压缩包扩展名(用户认知里的"压缩包",不含 docx/epub 等 zip 变体)
pub const ARCHIVE_EXTS: &[&str] = &[
    "zip", "7z", "rar", "tar", "tgz", "gz", "bz2", "tbz2", "xz", "txz",
];

const MARKER_ROOT: &str = "Software\\evorar\\Shell";
const BACKUP_ROOT: &str = "Software\\evorar\\Shell\\Backup";
const CLASSES: &str = "Software\\Classes";

// ---------------- 纯函数(布局构造,可单测) ----------------

pub fn icon_value(exe: &str) -> String {
    format!("\"{exe}\",0")
}

/// verb 的 command 行:exe 与 %1/%V 都必须加引号,否则带空格路径会被截断
pub fn verb_command(exe: &str, flag: Option<&str>, target: &str) -> String {
    match flag {
        Some(f) => format!("\"{exe}\" {f} \"{target}\""),
        None => format!("\"{exe}\" \"{target}\""),
    }
}

pub struct VerbSpec {
    /// `shell` 下的键名,如 `evorar.compress`
    pub key: &'static str,
    pub label: &'static str,
    pub flag: Option<&'static str>,
}

/// 所有文件/文件夹/文件夹空白处共用的压缩菜单
pub const CONTEXT_VERBS: &[VerbSpec] = &[
    VerbSpec { key: "evorar.compress", label: "用 evorar 压缩…", flag: Some("--compress") },
    VerbSpec { key: "evorar.compresszip", label: "evorar 压缩为 ZIP", flag: Some("--compress-zip") },
    VerbSpec { key: "evorar.compress7z", label: "evorar 压缩为 7Z", flag: Some("--compress-7z") },
];

/// 压缩包上的菜单(写在 ProgID 下,只有关联的扩展名可见)
pub const PROGID_VERBS: &[VerbSpec] = &[
    VerbSpec { key: "open", label: "用 evorar 打开", flag: None },
    VerbSpec { key: "evorar.here", label: "evorar 解压到当前文件夹", flag: Some("--extract-here") },
    VerbSpec { key: "evorar.smart", label: "evorar 智能解压", flag: Some("--extract") },
];

// ---------------- 对外 API ----------------

pub struct IntegrationFlags {
    pub association: bool,
    pub context_menu: bool,
}

pub struct Status {
    pub association: bool,
    pub context_menu: bool,
}

/// 应用/更新/移除集成(全量重写,exe 移动后重新调用即可刷新命令行路径)
pub fn apply(exe: &str, flags: IntegrationFlags) -> io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if flags.association {
        write_association(&hkcu, exe)?;
    } else {
        remove_association(&hkcu);
        // 备份只服务于「还原关联」,关闭关联时一并清掉
        let _ = hkcu.delete_subkey_all(BACKUP_ROOT);
    }
    if flags.context_menu {
        write_context_menu(&hkcu, exe)?;
    } else {
        remove_context_menu(&hkcu);
    }
    if !flags.association && !flags.context_menu {
        // 全关 = 零残留:标记键整个删除(父键为空时顺手移除)
        let _ = hkcu.delete_subkey_all(MARKER_ROOT);
        let _ = hkcu.delete_subkey("Software\\evorar");
    } else {
        write_markers(&hkcu, exe, &flags)?;
    }
    notify_shell();
    Ok(())
}

pub fn status() -> Status {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let marker = hkcu.open_subkey(MARKER_ROOT).ok();
    let flag = |name: &str| {
        marker
            .as_ref()
            .and_then(|m| m.get_value::<u32, _>(name).ok())
            .unwrap_or(0)
            == 1
    };
    let mut s = Status {
        association: flag("Association"),
        context_menu: flag("ContextMenu"),
    };
    // 标记在但 .zip 默认值已被系统/用户改走 → 如实报告未关联
    if s.association {
        let ours = hkcu
            .open_subkey(format!("{CLASSES}\\.zip"))
            .ok()
            .and_then(|k| k.get_value::<String, _>("").ok())
            .map(|v| v == PROG_ID)
            .unwrap_or(false);
        if !ours {
            s.association = false;
        }
    }
    s
}

/// 启动自愈:集成已开启,但注册表记录的 exe 路径与当前运行的不一致
/// (绿色版更新放到了新位置/改名)时,返回应重写的开关状态,由调用方静默 apply,
/// 避免旧 exe 删除后右键菜单与关联仍指向死路径。
pub fn needs_repair(exe: &str) -> Option<IntegrationFlags> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let m = hkcu.open_subkey(MARKER_ROOT).ok()?;
    let association = m.get_value::<u32, _>("Association").unwrap_or(0) == 1;
    let context_menu = m.get_value::<u32, _>("ContextMenu").unwrap_or(0) == 1;
    if !association && !context_menu {
        return None;
    }
    let recorded: String = m.get_value("Exe").ok()?;
    if recorded.eq_ignore_ascii_case(exe) {
        return None;
    }
    Some(IntegrationFlags { association, context_menu })
}

// ---------------- 关联(ProgID + 扩展名) ----------------

fn write_association(hkcu: &RegKey, exe: &str) -> io::Result<()> {
    let classes = hkcu.create_subkey(CLASSES)?.0;

    let progid = classes.create_subkey(PROG_ID)?.0;
    progid.set_value("", &"evorar 压缩包")?;
    let icon = classes.create_subkey(format!("{PROG_ID}\\DefaultIcon"))?.0;
    icon.set_value("", &icon_value(exe))?;
    for v in PROGID_VERBS {
        write_verb(&classes, &format!("{PROG_ID}\\shell\\{}", v.key), v, exe, "%1")?;
    }

    for ext in ARCHIVE_EXTS {
        let dot = format!(".{ext}");
        backup_ext_default(hkcu, &dot)?;
        let ek = classes.create_subkey(&dot)?.0;
        ek.set_value("", &PROG_ID)?;
        let ow = classes.create_subkey(format!("{dot}\\OpenWithProgids"))?.0;
        ow.set_value(PROG_ID, &"")?;
    }
    Ok(())
}

/// 记录关联前的原始默认值;当前值仍是我们时跳过(避免把备份覆盖成自己的 ProgID)
fn backup_ext_default(hkcu: &RegKey, dot: &str) -> io::Result<()> {
    let current = hkcu
        .open_subkey(format!("{CLASSES}\\{dot}"))
        .ok()
        .and_then(|k| k.get_value::<String, _>("").ok());
    if current.as_deref() == Some(PROG_ID) {
        return Ok(());
    }
    let b = hkcu.create_subkey(format!("{BACKUP_ROOT}\\{dot}"))?.0;
    match current {
        Some(v) => b.set_value("", &v)?,
        None => {
            b.delete_value("")?;
            b.set_value("Absent", &1u32)?
        }
    }
    Ok(())
}

fn remove_association(hkcu: &RegKey) {
    // 删除键需要父句柄带 DELETE 权限(RegDeleteTreeW)
    let Ok(classes) = hkcu.open_subkey_with_flags(CLASSES, KEY_ALL_ACCESS) else {
        return;
    };
    for ext in ARCHIVE_EXTS {
        let dot = format!(".{ext}");
        // 用户在系统里改指了别的程序时不碰默认值
        let current = classes
            .open_subkey(&dot)
            .ok()
            .and_then(|k| k.get_value::<String, _>("").ok());
        if current.as_deref() == Some(PROG_ID) {
            restore_ext_default(hkcu, &dot);
        }
        if let Ok(ow) = classes.open_subkey_with_flags(format!("{dot}\\OpenWithProgids"), KEY_READ | KEY_WRITE) {
            let _ = ow.delete_value(PROG_ID);
            // 键是我们建空的就删掉,不留残壳
            if ow.enum_values().next().is_none() && ow.enum_keys().next().is_none() {
                let _ = classes.delete_subkey(format!("{dot}\\OpenWithProgids"));
            }
        }
    }
    let _ = classes.delete_subkey_all(PROG_ID);
}

/// 还原某扩展名的默认值到关联前;无备份记录则删除默认值(回落到系统询问)
fn restore_ext_default(hkcu: &RegKey, dot: &str) {
    let Ok(ek) = hkcu
        .open_subkey_with_flags(format!("{CLASSES}\\{dot}"), KEY_WRITE | KEY_READ)
    else {
        return;
    };
    match hkcu.open_subkey(format!("{BACKUP_ROOT}\\{dot}")) {
        Ok(b) => {
            if b.get_value::<u32, _>("Absent").unwrap_or(0) == 1 {
                let _ = ek.delete_value("");
            } else if let Ok(v) = b.get_value::<String, _>("") {
                let _ = ek.set_value("", &v);
            } else {
                let _ = ek.delete_value("");
            }
        }
        Err(_) => {
            let _ = ek.delete_value("");
        }
    }
}

// ---------------- 右键菜单(* / Directory / Background) ----------------

fn write_context_menu(hkcu: &RegKey, exe: &str) -> io::Result<()> {
    let classes = hkcu.create_subkey(CLASSES)?.0;
    for base in ["*", "Directory"] {
        for v in CONTEXT_VERBS {
            write_verb(&classes, &format!("{base}\\shell\\{}", v.key), v, exe, "%1")?;
        }
    }
    // 文件夹空白处:参数必须是 %V(当前目录),%1 在这里为空
    for v in CONTEXT_VERBS {
        write_verb(
            &classes,
            &format!("Directory\\Background\\shell\\{}", v.key),
            v,
            exe,
            "%V",
        )?;
    }
    Ok(())
}

fn remove_context_menu(hkcu: &RegKey) {
    let Ok(classes) = hkcu.open_subkey_with_flags(CLASSES, KEY_ALL_ACCESS) else {
        return;
    };
    for base in ["*", "Directory", "Directory\\Background"] {
        for v in CONTEXT_VERBS {
            let _ = classes.delete_subkey_all(format!("{base}\\shell\\{}", v.key));
        }
    }
}

fn write_verb(classes: &RegKey, shell_key: &str, spec: &VerbSpec, exe: &str, target: &str) -> io::Result<()> {
    let k = classes.create_subkey(shell_key)?.0;
    k.set_value("MUIVerb", &spec.label)?;
    k.set_value("Icon", &icon_value(exe))?;
    let c = classes.create_subkey(format!("{shell_key}\\command"))?.0;
    c.set_value("", &verb_command(exe, spec.flag, target))
}

// ---------------- 标记 / 工具 ----------------

fn write_markers(hkcu: &RegKey, exe: &str, flags: &IntegrationFlags) -> io::Result<()> {
    let m = hkcu.create_subkey(MARKER_ROOT)?.0;
    m.set_value("Exe", &exe)?;
    m.set_value("Association", &(flags.association as u32))?;
    m.set_value("ContextMenu", &(flags.context_menu as u32))
}

/// 通知资源管理器关联已变化,免注销立即生效
fn notify_shell() {
    use windows_sys::Win32::UI::Shell::{
        SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST,
    };
    unsafe {
        SHChangeNotify(
            SHCNE_ASSOCCHANGED as i32,
            SHCNF_IDLIST,
            std::ptr::null(),
            std::ptr::null(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真实注册表用例共享一把锁,避免并行测试互踩同一套 HKCU 键
    static REG_GUARD: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

    #[test]
    fn verb_command_quotes_everything() {
        assert_eq!(
            verb_command(r"C:\Program Files\evorar\evorar.exe", Some("--extract"), "%1"),
            "\"C:\\Program Files\\evorar\\evorar.exe\" --extract \"%1\""
        );
        assert_eq!(
            verb_command(r"C:\e.exe", None, "%1"),
            "\"C:\\e.exe\" \"%1\""
        );
        // 文件夹空白处必须用 %V
        assert!(verb_command(r"C:\e.exe", Some("--compress"), "%V").ends_with("\"%V\""));
    }

    #[test]
    fn icon_value_format() {
        assert_eq!(icon_value(r"C:\a b\evorar.exe"), "\"C:\\a b\\evorar.exe\",0");
    }

    #[test]
    fn verb_specs_consistent() {
        let mut keys: Vec<&str> = CONTEXT_VERBS.iter().map(|v| v.key).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), CONTEXT_VERBS.len(), "verb 键名不能重复");
        assert!(CONTEXT_VERBS.iter().all(|v| !v.label.is_empty()));
        assert!(PROGID_VERBS.iter().all(|v| !v.label.is_empty()));
        assert!(PROGID_VERBS.iter().any(|v| v.key == "open"), "ProgID 必须有默认 open 动词");
    }

    #[test]
    fn archive_exts_are_valid() {
        assert!(!ARCHIVE_EXTS.is_empty());
        for e in ARCHIVE_EXTS {
            assert!(!e.contains('.'), "扩展名不带点:{e}");
            assert!(
                e.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
                "扩展名必须小写:{e}"
            );
        }
        let mut sorted = ARCHIVE_EXTS.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ARCHIVE_EXTS.len(), "扩展名不能重复");
    }

    /// 真实 HKCU 注册表往返:写入 → 校验布局 → 移除 → 校验还原。
    /// 全程只动本模块管理的键,结束时恢复原状;中断残留可用设置开关重写/清除。
    #[test]
    fn apply_status_remove_roundtrip() {
        let _reg = REG_GUARD.lock();
        let exe = r"C:\evorar-test-does-not-exist.exe";
        apply(
            exe,
            IntegrationFlags { association: true, context_menu: true },
        )
        .unwrap();

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let dot: String = hkcu
            .open_subkey(format!("{CLASSES}\\.zip"))
            .unwrap()
            .get_value("")
            .unwrap();
        assert_eq!(dot, PROG_ID);
        let cmd: String = hkcu
            .open_subkey(format!("{CLASSES}\\*\\shell\\evorar.compress\\command"))
            .unwrap()
            .get_value("")
            .unwrap();
        assert_eq!(cmd, verb_command(exe, Some("--compress"), "%1"));
        let bg: String = hkcu
            .open_subkey(format!("{CLASSES}\\Directory\\Background\\shell\\evorar.compresszip\\command"))
            .unwrap()
            .get_value("")
            .unwrap();
        assert_eq!(bg, verb_command(exe, Some("--compress-zip"), "%V"));

        let s = status();
        assert!(s.association && s.context_menu);

        apply(
            exe,
            IntegrationFlags { association: false, context_menu: false },
        )
        .unwrap();
        let s = status();
        assert!(!s.association && !s.context_menu);
        assert!(hkcu.open_subkey(format!("{CLASSES}\\{PROG_ID}")).is_err());
        let dot: String = hkcu
            .open_subkey(format!("{CLASSES}\\.zip"))
            .unwrap()
            .get_value("")
            .unwrap();
        assert_ne!(dot, PROG_ID, "移除后默认值必须还原/清空");
        assert!(
            hkcu.open_subkey(format!("{CLASSES}\\*\\shell\\evorar.compress")).is_err(),
            "右键菜单键必须删除"
        );
        assert!(hkcu.open_subkey(MARKER_ROOT).is_err(), "全关时标记键应整体删除");
    }

    /// 绿色版更新场景:exe 换路径后启动,检测到不一致并重写
    #[test]
    fn needs_repair_detects_path_change() {
        let _reg = REG_GUARD.lock();
        let old = r"C:\old\evorar.exe";
        let new = r"C:\new dir\evorar.exe";
        apply(old, IntegrationFlags { association: true, context_menu: true }).unwrap();

        assert!(needs_repair(old).is_none(), "路径一致不需要修复");

        let flags = needs_repair(new).expect("路径变化应触发修复");
        assert!(flags.association && flags.context_menu, "保持原开关状态");

        apply(new, IntegrationFlags { association: true, context_menu: true }).unwrap();
        assert!(needs_repair(new).is_none(), "重写后不再需要修复");

        apply(new, IntegrationFlags { association: false, context_menu: false }).unwrap();
        assert!(needs_repair(new).is_none(), "集成已关不触发修复");
        assert!(needs_repair(old).is_none());
    }
}

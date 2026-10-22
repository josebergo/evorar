//! macOS Shell 集成:声明式,无运行期写入。
//!
//! - 文件关联:`tauri.conf.json > bundle.fileAssociations` 打包期写入
//!   Info.plist 的 CFBundleDocumentTypes;双击压缩包时 evorar 出现在
//!   「打开方式」列表,由用户选择设为默认(Launch Services 模型)。
//! - 右键菜单:NSServices 服务菜单为后续项(design-mac.md §4.2 方案 A),
//!   当前版本未提供。
//!
//! 因此 `apply` 在 macOS 恒定失败(前端不展示可写开关),`status` 如实回报。

use std::io;

pub struct IntegrationFlags {
    pub association: bool,
    pub context_menu: bool,
}

pub struct Status {
    pub association: bool,
    pub context_menu: bool,
}

pub fn apply(_exe: &str, _flags: IntegrationFlags) -> io::Result<()> {
    Err(std::io::Error::other(
        "macOS 下文件关联由系统「打开方式」管理,请在访达 → 显示简介 → 打开方式 中选择 evorar",
    ))
}

pub fn status() -> Status {
    Status {
        association: false,
        context_menu: false,
    }
}

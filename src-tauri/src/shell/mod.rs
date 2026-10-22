//! Shell 集成的平台抽象层。
//!
//! - Windows:HKCU 注册表 verb(见 `windows.rs`)
//! - macOS:文件关联在打包期由 Info.plist 声明(design-mac.md §4.1),
//!   运行期只读状态、不写系统;右键服务菜单(NSServices)为后续项
//!
//! Tauri command 的 IPC 契约(`shell_integration_get/set`)两平台一致,
//! 前端按返回的 `platform` 字段适配 UI。

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub use windows as platform;

#[cfg(target_os = "macos")]
pub use macos as platform;

pub use platform::{apply, status, IntegrationFlags, Status};

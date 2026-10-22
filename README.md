<img src="design/icon.png" width="128" height="128" alt="Evorar 艾呀压图标" />

# Evorar 艾呀压

**[English](README.en.md) | 简体中文**

纯静、轻快、漂亮的解压缩工具。**零广告、零后台驻留、零捆绑,完全开源。**

![Tauri](https://img.shields.io/badge/Tauri-2.x-blue) ![Rust](https://img.shields.io/badge/Rust-stable-orange) ![License](https://img.shields.io/badge/License-MIT-green)

## 为什么做

| 竞品 | 问题 |
|------|------|
| 360压缩 | 云查杀 + 推广配置拉取 + 每 3 小时后台更新任务 + 捆绑下载 |
| WinRAR | 收费、试用弹窗、界面老旧 |
| Bandizip 免费版 | 7.37 起出现桌面弹窗广告 |

evorar 的承诺:**除用户显式操作外零网络请求、零计划任务、零常驻进程;关闭窗口即进程退出。**

## 功能

- **格式**:创建 ZIP / 7Z;解压 ZIP / 7Z / RAR(含 RAR5)/ TAR / GZ / BZ2 / XZ
- **智能解压**:唯一顶层目录 → 直接解到当前目录不套娃;多顶层条目 → 自动创建同名文件夹
- **资源管理器右键菜单**:文件/文件夹右键「用 evorar 压缩…/压缩为 ZIP/压缩为 7Z」,压缩包右键「智能解压/解压到当前文件夹」(HKCU 注册表实现,免管理员)
- **文件关联**:一键设为 ZIP / 7Z / RAR / TAR 等的默认打开方式并显示 evorar 图标,原默认程序自动备份、关闭开关即还原(设置 → ⚙ 集成)
- **中文乱码修复**:ZIP 内 GBK/UTF-8 文件名自动检测修复(国内用户痛点)
- **安全**:解压路径穿越(zip-slip)拦截,非法路径直接拒绝
- **轻快**:单文件绿色版约 4MB,启动内存约 35MB(低于 WinRAR / 360压缩)
- 现代界面:Fluent 风格、亮暗主题跟随系统、拖放即用、进度可取消

> RAR 仅支持解压:RAR 压缩算法为 RarLab 专有,法律不允许第三方工具创建 RAR。
>
> Windows 11 下右键菜单项位于「显示更多选项」(Shift+F10)经典菜单;若某扩展名已被系统记住其他默认程序,在「打开方式 → 选择其他应用」里选 evorar 并设为始终即可。

## 下载

从 [Releases](https://github.com/josebergo/evorar/releases) 获取:

- `evorar.exe` — Windows 便携版,单文件免安装,双击即用
- `evorar_x64-setup.exe` — Windows NSIS 安装包(开始菜单/桌面快捷方式/可卸载)
- `evorar_aarch64.dmg` — macOS 安装包(Apple Silicon),拖入 Applications 即装,删除 .app 即卸载

要求 Windows 10(2004+)/ Windows 11(系统自带 WebView2),或 macOS 10.15+(Apple Silicon)。

> **macOS 首次运行**:DMG 未经公证时 Gatekeeper 会拦截,右键点击应用选「打开」,或终端执行
> `xattr -d com.apple.quarantine /Applications/evorar.app` 后再打开。
> 在访达中把压缩包的「打开方式」设为 evorar 即可双击直达。

## 构建

```bash
pnpm install          # 前端依赖
pnpm tauri dev        # 开发模式(热重载)
pnpm tauri build --no-bundle   # 发布构建(前端内嵌;NSIS 安装包另行手工打包)
cd src-tauri && cargo test   # 引擎测试(含 GBK 乱码端到端用例)
```

> ⚠️ 发布 exe 必须经 `pnpm tauri build` 构建。直接 `cargo build --release` 会得到连 `localhost:5173` 的开发版 exe,脱离 `pnpm tauri dev` 运行时首页报 ERR_CONNECTION_REFUSED。

要求:Rust stable(MSVC)、Node 20+、pnpm。

## 架构

```
code/
├── src/                  # 前端(WebView2 渲染层,零框架依赖)
│   ├── main.ts           # 状态机:空态/浏览态/压缩态;启动动作与集成设置
│   └── style.css         # Fluent 风格 + 亮暗主题
└── src-tauri/
    ├── src/lib.rs        # Tauri commands、任务队列、进度事件、启动参数
    ├── src/shell.rs      # Windows Shell 集成(注册表 verb:右键菜单 + 文件关联)
    └── src/engine/       # 压缩引擎(与 UI 完全解耦,可独立测试)
        ├── mod.rs        # 统一 trait + zip-slip 防护
        ├── detect.rs     # 魔数嗅探 + GBK/UTF-8 编码修复
        ├── zipfmt.rs     # ZIP 引擎
        ├── sevenz.rs     # 7Z 引擎(LZMA2)
        ├── tarball.rs    # TAR/GZ/BZ2/XZ
        ├── rar.rs        # RAR 解压(unrar)
        └── smart.rs      # 智能解压策略
```

## 路线图

- [x] MVP:解压/压缩/智能解压/GBK 乱码修复/进度取消
- [x] P1:资源管理器右键菜单(注册表 verb)、文件关联与图标
- [x] macOS 版:Apple Silicon DMG、声明式文件关联、mac 垃圾条目过滤(`__MACOSX/`、`.DS_Store`、`._*`)与 NFD 文件名归一(CI 构建)
- [ ] P1 余项:包内预览、AES-256 加密、分卷;macOS Finder 服务菜单
- [ ] P2:自解压 SFX、恢复记录、多语言界面

## 许可

[MIT](LICENSE)

RAR 解压基于 [unrar](https://www.rarlab.com/rar_add.htm) 源码,遵循其许可条款(允许免费解压用途);其余依赖均为开源组件。

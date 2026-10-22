# 🗜️ evorar

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
- **中文乱码修复**:ZIP 内 GBK/UTF-8 文件名自动检测修复(国内用户痛点)
- **安全**:解压路径穿越(zip-slip)拦截,非法路径直接拒绝
- **轻快**:单文件绿色版约 4MB,启动内存约 35MB(低于 WinRAR / 360压缩)
- 现代界面:Fluent 风格、亮暗主题跟随系统、拖放即用、进度可取消

> RAR 仅支持解压:RAR 压缩算法为 RarLab 专有,法律不允许第三方工具创建 RAR。

## 下载

从 [Releases](https://github.com/josebergo/evorar/releases) 获取:

- `evorar.exe` — 便携版,单文件免安装,双击即用
- `evorar_x64-setup.exe` — NSIS 安装包(开始菜单/桌面快捷方式/可卸载)

要求 Windows 10(2004+)/ Windows 11(系统自带 WebView2 运行时)。

## 构建

```bash
pnpm install          # 前端依赖
pnpm tauri dev        # 开发模式(热重载)
pnpm tauri build      # 发布构建
cd src-tauri && cargo test   # 引擎测试(含 GBK 乱码端到端用例)
```

要求:Rust stable(MSVC)、Node 20+、pnpm。

## 架构

```
code/
├── src/                  # 前端(WebView2 渲染层,零框架依赖)
│   ├── main.ts           # 状态机:空态/浏览态/压缩态
│   └── style.css         # Fluent 风格 + 亮暗主题
└── src-tauri/
    ├── src/lib.rs        # Tauri commands、任务队列、进度事件
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
- [ ] P1:右键菜单 Shell 扩展、文件关联、包内预览、AES-256 加密、分卷
- [ ] P2:自解压 SFX、恢复记录、多语言界面

## 许可

[MIT](LICENSE)

RAR 解压基于 [unrar](https://www.rarlab.com/rar_add.htm) 源码,遵循其许可条款(允许免费解压用途);其余依赖均为开源组件。

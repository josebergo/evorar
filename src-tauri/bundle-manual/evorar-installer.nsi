; evorar NSIS 安装脚本(简中界面、单 exe 绿色安装)
; 构建:makensis.exe /INPUTCHARSET UTF8 evorar-installer.nsi
; Shell 集成全部写 HKCU(免管理员、与应用内设置面板共用同一份键位,见 src/shell.rs)
Unicode true
ManifestDPIAware true
Name "Evorar 艾呀压"
OutFile "evorar_0.3.2_x64-setup.exe"
InstallDir "$PROGRAMFILES64\evorar"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!insertmacro GetSize

!define MUI_ABORTWARNING
!define MUI_ICON "icon.ico"
!define MUI_UNICON "icon.ico"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "SimpChinese"

; 写一个右键 verb(MUIVerb + Icon + command)
!macro WriteVerb Base Key Label Args Target
  WriteRegStr HKCU "Software\Classes\${Base}\shell\${Key}" "MUIVerb" "${Label}"
  WriteRegStr HKCU "Software\Classes\${Base}\shell\${Key}" "Icon" "$\"$INSTDIR\evorar.exe$\",0"
  !if "${Args}" == ""
    WriteRegStr HKCU "Software\Classes\${Base}\shell\${Key}\command" "" "$\"$INSTDIR\evorar.exe$\" $\"${Target}$\""
  !else
    WriteRegStr HKCU "Software\Classes\${Base}\shell\${Key}\command" "" "$\"$INSTDIR\evorar.exe$\" ${Args} $\"${Target}$\""
  !endif
!macroend

; 备份扩展名当前默认值(已是我们时不覆盖备份)
!macro BackupExt Ext
  StrCpy $0 ""
  ClearErrors
  ReadRegStr $0 HKCU "Software\Classes\.${Ext}" ""
  ${If} $0 != "evorar.Archive"
    ${If} $0 == ""
      WriteRegDWORD HKCU "Software\evorar\Shell\Backup\.${Ext}" "Absent" 1
    ${Else}
      WriteRegStr HKCU "Software\evorar\Shell\Backup\.${Ext}" "" "$0"
    ${EndIf}
  ${EndIf}
!macroend

; 关联一个扩展名:备份 → 默认值指向 ProgID → 进「打开方式」列表
!macro AssociateExt Ext
  !insertmacro BackupExt "${Ext}"
  WriteRegStr HKCU "Software\Classes\.${Ext}" "" "evorar.Archive"
  WriteRegStr HKCU "Software\Classes\.${Ext}\OpenWithProgids" "evorar.Archive" ""
!macroend

; 卸载时还原扩展名默认值(仅当仍指向我们),并移出「打开方式」列表
!macro RestoreExt Ext
  StrCpy $2 ""
  ClearErrors
  ReadRegStr $2 HKCU "Software\Classes\.${Ext}" ""
  ${If} $2 == "evorar.Archive"
    StrCpy $0 ""
    ClearErrors
    ReadRegStr $0 HKCU "Software\evorar\Shell\Backup\.${Ext}" ""
    ${If} ${Errors}
      DeleteRegValue HKCU "Software\Classes\.${Ext}" ""
    ${Else}
      WriteRegStr HKCU "Software\Classes\.${Ext}" "" "$0"
    ${EndIf}
  ${EndIf}
  DeleteRegValue HKCU "Software\Classes\.${Ext}\OpenWithProgids" "evorar.Archive"
!macroend

Section "evorar 主程序(必需)"
  SectionIn RO
  SetOutPath "$INSTDIR"
  File "evorar.exe"
  File "icon.ico"

  ; 开始菜单与桌面快捷方式
  CreateDirectory "$SMPROGRAMS\evorar"
  CreateShortCut "$SMPROGRAMS\evorar\evorar.lnk" "$INSTDIR\evorar.exe"
  CreateShortCut "$DESKTOP\evorar.lnk" "$INSTDIR\evorar.exe"

  ; 卸载信息(设置→应用里显示发布者与大小)
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\evorar" "DisplayName" "Evorar 艾呀压"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\evorar" "Publisher" "码和数字 (Mh Digit)"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\evorar" "DisplayVersion" "0.3.2"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\evorar" "DisplayIcon" "$INSTDIR\evorar.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\evorar" "UninstallString" "$INSTDIR\Uninstall.exe"
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\evorar" "NoModify" 1
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\evorar" "NoRepair" 1
  ; 实测安装目录体积(KB)写入 EstimatedSize,应用列表的「大小」不再为空
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\evorar" "EstimatedSize" $0
SectionEnd

Section "资源管理器集成(右键菜单与文件关联)" SEC_SHELL
  ; 压缩包 ProgID:图标 + 打开/智能解压/解压到当前文件夹
  WriteRegStr HKCU "Software\Classes\evorar.Archive" "" "evorar 压缩包"
  WriteRegStr HKCU "Software\Classes\evorar.Archive\DefaultIcon" "" "$\"$INSTDIR\evorar.exe$\",0"
  !insertmacro WriteVerb "evorar.Archive" "open" "用 evorar 打开" "" "%1"
  !insertmacro WriteVerb "evorar.Archive" "evorar.here" "evorar 解压到当前文件夹" "--extract-here" "%1"
  !insertmacro WriteVerb "evorar.Archive" "evorar.smart" "evorar 智能解压" "--extract" "%1"

  ; 扩展名关联
  !insertmacro AssociateExt "zip"
  !insertmacro AssociateExt "7z"
  !insertmacro AssociateExt "rar"
  !insertmacro AssociateExt "tar"
  !insertmacro AssociateExt "tgz"
  !insertmacro AssociateExt "gz"
  !insertmacro AssociateExt "bz2"
  !insertmacro AssociateExt "tbz2"
  !insertmacro AssociateExt "xz"
  !insertmacro AssociateExt "txz"

  ; 所有文件 / 文件夹 / 文件夹空白处的压缩菜单(空白处参数为 %V)
  !insertmacro WriteVerb "*" "evorar.compress" "用 evorar 压缩…" "--compress" "%1"
  !insertmacro WriteVerb "*" "evorar.compresszip" "evorar 压缩为 ZIP" "--compress-zip" "%1"
  !insertmacro WriteVerb "*" "evorar.compress7z" "evorar 压缩为 7Z" "--compress-7z" "%1"
  !insertmacro WriteVerb "Directory" "evorar.compress" "用 evorar 压缩…" "--compress" "%1"
  !insertmacro WriteVerb "Directory" "evorar.compresszip" "evorar 压缩为 ZIP" "--compress-zip" "%1"
  !insertmacro WriteVerb "Directory" "evorar.compress7z" "evorar 压缩为 7Z" "--compress-7z" "%1"
  !insertmacro WriteVerb "Directory\Background" "evorar.compress" "用 evorar 压缩…" "--compress" "%V"
  !insertmacro WriteVerb "Directory\Background" "evorar.compresszip" "evorar 压缩为 ZIP" "--compress-zip" "%V"
  !insertmacro WriteVerb "Directory\Background" "evorar.compress7z" "evorar 压缩为 7Z" "--compress-7z" "%V"

  ; 状态标记(应用内「集成」面板据此显示开关状态)
  WriteRegStr HKCU "Software\evorar\Shell" "Exe" "$INSTDIR\evorar.exe"
  WriteRegDWORD HKCU "Software\evorar\Shell" "Association" 1
  WriteRegDWORD HKCU "Software\evorar\Shell" "ContextMenu" 1
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\evorar.exe"
  Delete "$INSTDIR\icon.ico"
  Delete "$INSTDIR\Uninstall.exe"
  Delete "$SMPROGRAMS\evorar\evorar.lnk"
  RMDir "$SMPROGRAMS\evorar"
  Delete "$DESKTOP\evorar.lnk"
  RMDir "$INSTDIR"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\evorar"

  ; 还原文件关联(仅当默认值仍指向 evorar)
  !insertmacro RestoreExt "zip"
  !insertmacro RestoreExt "7z"
  !insertmacro RestoreExt "rar"
  !insertmacro RestoreExt "tar"
  !insertmacro RestoreExt "tgz"
  !insertmacro RestoreExt "gz"
  !insertmacro RestoreExt "bz2"
  !insertmacro RestoreExt "tbz2"
  !insertmacro RestoreExt "xz"
  !insertmacro RestoreExt "txz"

  ; 清理右键菜单与 ProgID、状态标记
  DeleteRegKey HKCU "Software\Classes\evorar.Archive"
  DeleteRegKey HKCU "Software\Classes\*\shell\evorar.compress"
  DeleteRegKey HKCU "Software\Classes\*\shell\evorar.compresszip"
  DeleteRegKey HKCU "Software\Classes\*\shell\evorar.compress7z"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\evorar.compress"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\evorar.compresszip"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\evorar.compress7z"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\evorar.compress"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\evorar.compresszip"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\evorar.compress7z"
  DeleteRegKey HKCU "Software\evorar\Shell"
SectionEnd

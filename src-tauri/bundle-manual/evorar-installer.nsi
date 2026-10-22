; evorar NSIS 安装脚本(简中界面、单 exe 绿色安装)
; 构建:makensis.exe /INPUTCHARSET UTF8 evorar-installer.nsi
Unicode true
ManifestDPIAware true
Name "evorar"
OutFile "evorar_0.1.0_x64-setup.exe"
InstallDir "$PROGRAMFILES64\evorar"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

!include "MUI2.nsh"

!define MUI_ABORTWARNING
!define MUI_ICON "icon.ico"
!define MUI_UNICON "icon.ico"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "SimpChinese"

Section "evorar 主程序(必需)"
  SectionIn RO
  SetOutPath "$INSTDIR"
  File "evorar.exe"
  File "icon.ico"

  ; 开始菜单与桌面快捷方式
  CreateDirectory "$SMPROGRAMS\evorar"
  CreateShortCut "$SMPROGRAMS\evorar\evorar.lnk" "$INSTDIR\evorar.exe"
  CreateShortCut "$DESKTOP\evorar.lnk" "$INSTDIR\evorar.exe"

  ; 卸载信息
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\evorar" "DisplayName" "evorar"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\evorar" "DisplayVersion" "0.1.0"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\evorar" "DisplayIcon" "$INSTDIR\evorar.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\evorar" "UninstallString" "$INSTDIR\Uninstall.exe"
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\evorar" "NoModify" 1
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\evorar" "NoRepair" 1
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
SectionEnd

; Path of Titans VR Launcher - NSIS installer
; Per-user install (no admin), bundled UEVR DLLs, Start Menu + Desktop shortcuts,
; full Add/Remove Programs uninstall.
;
; Compiled by build.ps1 via makensis. Overridable defines:
;   /DVERSION=<ver>       product version
;   /DEXE_NAME=<file>     launcher exe filename
;   /DSTAGE_DIR=<dir>     staging folder with exe + DLLs to install
;   /DOUT_FILE=<file>     output Setup .exe path

!include "MUI2.nsh"

!ifndef VERSION
  !define VERSION "0.1.0"
!endif
!ifndef EXE_NAME
  !define EXE_NAME "pot-vr-launcher.exe"
!endif
!ifndef STAGE_DIR
  !define STAGE_DIR "${__FILEDIR__}\stage"
!endif

!define APP_NAME      "Path of Titans VR Launcher"
!define APP_PUB       "cybershrimp23"
!define APP_ID        "PoTVRLauncher"
!define REG_UNINST    "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}"

Name "${APP_NAME}"
Unicode true
!ifndef OUT_FILE
  OutFile "${__FILEDIR__}\..\dist\PoTVRLauncher-Setup-${VERSION}.exe"
!else
  OutFile "${OUT_FILE}"
!endif

; Per-user install: no UAC prompt, installs into the current user's profile.
InstallDir "$LOCALAPPDATA\Programs\Path of Titans VR Launcher"
InstallDirRegKey HKCU "${REG_UNINST}" "InstallLocation"
RequestExecutionLevel user

; ---- Modern UI pages ----
!define MUI_ABORTWARNING
!define MUI_ICON "${NSISDIR}\Contrib\Graphics\Icons\modern-install.ico"
!define MUI_UNICON "${NSISDIR}\Contrib\Graphics\Icons\modern-uninstall.ico"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES

!define MUI_FINISHPAGE_RUN "$INSTDIR\${EXE_NAME}"
!define MUI_FINISHPAGE_RUN_TEXT "Run ${APP_NAME}"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; ---- Install ----
Section "Application" SecApp
  SetOutPath "$INSTDIR"
  ; Everything staged by build.ps1 (exe + UEVRBackend.dll + openvr_api.dll +
  ; UEVRPluginNullifier.dll [+ openxr_loader.dll if present]).
  File "${STAGE_DIR}\pot-vr-launcher.exe"
  File "${STAGE_DIR}\UEVRBackend.dll"
  File "${STAGE_DIR}\openvr_api.dll"
  File /nonfatal "${STAGE_DIR}\UEVRPluginNullifier.dll"
  File /nonfatal "${STAGE_DIR}\openxr_loader.dll"

  ; Start Menu + Desktop shortcuts
  CreateDirectory "$SMPROGRAMS\${APP_NAME}"
  CreateShortCut "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk" "$INSTDIR\${EXE_NAME}"
  CreateShortCut "$SMPROGRAMS\${APP_NAME}\Uninstall ${APP_NAME}.lnk" "$INSTDIR\Uninstall.exe"
  CreateShortCut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\${EXE_NAME}"

  ; Uninstaller + Add/Remove Programs registration
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr  HKCU "${REG_UNINST}" "DisplayName"     "${APP_NAME}"
  WriteRegStr  HKCU "${REG_UNINST}" "DisplayVersion"  "${VERSION}"
  WriteRegStr  HKCU "${REG_UNINST}" "Publisher"       "${APP_PUB}"
  WriteRegStr  HKCU "${REG_UNINST}" "InstallLocation" "$INSTDIR"
  WriteRegStr  HKCU "${REG_UNINST}" "DisplayIcon"     "$INSTDIR\${EXE_NAME}"
  WriteRegStr  HKCU "${REG_UNINST}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKCU "${REG_UNINST}" "NoModify" 1
  WriteRegDWORD HKCU "${REG_UNINST}" "NoRepair" 1
SectionEnd

; ---- Uninstall ----
Section "Uninstall"
  ; Program files
  Delete "$INSTDIR\pot-vr-launcher.exe"
  Delete "$INSTDIR\UEVRBackend.dll"
  Delete "$INSTDIR\openvr_api.dll"
  Delete "$INSTDIR\UEVRPluginNullifier.dll"
  Delete "$INSTDIR\openxr_loader.dll"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir  "$INSTDIR"

  ; Shortcuts
  Delete "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}\Uninstall ${APP_NAME}.lnk"
  RMDir  "$SMPROGRAMS\${APP_NAME}"
  Delete "$DESKTOP\${APP_NAME}.lnk"

  ; Registry (program settings under %APPDATA%\PoTVRLauncher are intentionally kept)
  DeleteRegKey HKCU "${REG_UNINST}"
SectionEnd

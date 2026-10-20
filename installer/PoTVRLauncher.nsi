; Path of Titans VR Launcher - NSIS installer
; Per-user install (no admin), bundled UEVR DLLs, Start Menu + Desktop shortcuts.
; Version-specific install folder so multiple versions can coexist, with a
; registry record of every install and an "remove the old version?" prompt.
;
; Compiled by build.ps1 via makensis. Overridable defines:
;   /DVERSION=<ver>       product version
;   /DEXE_NAME=<file>     launcher exe filename
;   /DSTAGE_DIR=<dir>     staging folder with exe + DLLs to install
;   /DOUT_FILE=<file>     output Setup .exe path

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"

!ifndef VERSION
  !define VERSION "0.1.0"
!endif
!ifndef EXE_NAME
  !define EXE_NAME "pot-vr-launcher.exe"
!endif
!ifndef STAGE_DIR
  !define STAGE_DIR "${__FILEDIR__}\stage"
!endif

!define APP_NAME   "Path of Titans VR Launcher"
!define APP_PUB    "cybershrimp23"
!define APP_ID     "PoTVRLauncher"
; Our bookkeeping key: one subkey per installed version under \Versions.
!define APP_REG    "Software\${APP_PUB}\${APP_NAME}"
!define VERSIONS   "${APP_REG}\Versions"
!define THIS_VER   "${VERSIONS}\${VERSION}"
; Add/Remove Programs entry, per version, so each shows separately.
!define ARP_KEY    "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID} ${VERSION}"

Name "${APP_NAME}"
Unicode true
!ifndef OUT_FILE
  OutFile "${__FILEDIR__}\..\dist\PoTVRLauncher-Setup-${VERSION}.exe"
!else
  OutFile "${OUT_FILE}"
!endif

; Version-specific default folder => side-by-side installs never collide.
InstallDir "$LOCALAPPDATA\Programs\${APP_NAME} ${VERSION}"
RequestExecutionLevel user

; Global: set by the startup prompt (1 = remove previously installed versions).
Var REMOVE_OLD

; ---- Modern UI pages ----
!define MUI_ABORTWARNING
!define MUI_ICON "${__FILEDIR__}\..\assets\icon.ico"
!define MUI_UNICON "${__FILEDIR__}\..\assets\icon.ico"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${__FILEDIR__}\LICENSE.txt"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\${EXE_NAME}"
!define MUI_FINISHPAGE_RUN_TEXT "Run ${APP_NAME}"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

; ---- Startup: detect prior installs and offer to remove them ----
Function .onInit
  SetRegView 64
  StrCpy $REMOVE_OLD 0

  ; Any previously installed version recorded?
  EnumRegKey $R0 HKCU "${VERSIONS}" 0
  StrCmp $R0 "" done            ; nothing installed -> no prompt
    MessageBox MB_YESNO|MB_ICONQUESTION "A previously installed version of ${APP_NAME} was detected.$\r$\n$\r$\nClick Yes to REMOVE the old installation(s), then install v${VERSION}.$\r$\nClick No to keep them and install v${VERSION} side-by-side." /SD IDNO IDYES setremove IDNO keepboth
    keepboth:
      StrCpy $REMOVE_OLD 0
      Goto done
    setremove:
      StrCpy $REMOVE_OLD 1
  done:
FunctionEnd

; ---- Remove previously installed versions (runs their uninstallers) ----
Function RemoveOldVersions
  ${If} $REMOVE_OLD == 1
    StrCpy $R9 0                ; loop guard
    loop:
      ${If} $R9 >= 16
        Goto done
      ${EndIf}
      EnumRegKey $R0 HKCU "${VERSIONS}" 0     ; first remaining version
      StrCmp $R0 "" done
      ReadRegStr $R1 HKCU "${VERSIONS}\$R0" "UninstallExe"
      ReadRegStr $R2 HKCU "${VERSIONS}\$R0" "InstallDir"
      ${If} $R1 != ""
      ${AndIf} ${FileExists} "$R1"
        ; /S = silent; _? points at its install dir so it can self-remove
        ExecWait '"$R1" /S _?=$R2'
      ${EndIf}
      DeleteRegKey HKCU "${VERSIONS}\$R0"     ; ensure it's gone regardless
      IntOp $R9 $R9 + 1
      Goto loop
    done:
  ${EndIf}
FunctionEnd

; ---- Install ----
Section "Application" SecApp
  Call RemoveOldVersions

  SetOutPath "$INSTDIR"
  File "${STAGE_DIR}\pot-vr-launcher.exe"
  File "${STAGE_DIR}\UEVRBackend.dll"
  File "${STAGE_DIR}\openvr_api.dll"
  File /nonfatal "${STAGE_DIR}\UEVRPluginNullifier.dll"
  File /nonfatal "${STAGE_DIR}\openxr_loader.dll"
  File /nonfatal "${STAGE_DIR}\vr_image.png"

  ; Start Menu + Desktop shortcuts, version-tagged so multiple coexist.
  CreateDirectory "$SMPROGRAMS\${APP_NAME}"
  CreateShortCut "$SMPROGRAMS\${APP_NAME}\${APP_NAME} ${VERSION}.lnk" "$INSTDIR\${EXE_NAME}"
  CreateShortCut "$SMPROGRAMS\${APP_NAME}\Uninstall ${APP_NAME} ${VERSION}.lnk" "$INSTDIR\Uninstall.exe"
  CreateShortCut "$DESKTOP\${APP_NAME} ${VERSION}.lnk" "$INSTDIR\${EXE_NAME}"

  ; Uninstaller
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  ; Our per-version install-location registry record.
  WriteRegStr  HKCU "${THIS_VER}" "DisplayName"   "${APP_NAME} ${VERSION}"
  WriteRegStr  HKCU "${THIS_VER}" "DisplayVersion" "${VERSION}"
  WriteRegStr  HKCU "${THIS_VER}" "InstallDir"     "$INSTDIR"
  WriteRegStr  HKCU "${THIS_VER}" "UninstallExe"   "$INSTDIR\Uninstall.exe"

  ; Add/Remove Programs (per version).
  WriteRegStr  HKCU "${ARP_KEY}" "DisplayName"     "${APP_NAME} ${VERSION}"
  WriteRegStr  HKCU "${ARP_KEY}" "DisplayVersion"  "${VERSION}"
  WriteRegStr  HKCU "${ARP_KEY}" "Publisher"       "${APP_PUB}"
  WriteRegStr  HKCU "${ARP_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr  HKCU "${ARP_KEY}" "DisplayIcon"     "$INSTDIR\${EXE_NAME}"
  WriteRegStr  HKCU "${ARP_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKCU "${ARP_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${ARP_KEY}" "NoRepair" 1
SectionEnd

; ---- Optional: register "Path of Titans VR" in SteamVR Home ----
Section "Add to SteamVR Home" SecVRHome
  ; The launcher writes pottvr.vrmanifest next to the exe and adds it to the
  ; OpenVR path registry ("manifest_paths"). SteamVR must restart to pick it up.
  nsExec::ExecToLog '"$INSTDIR\${EXE_NAME}" --register'
  Pop $0
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecApp}    "The Path of Titans VR Launcher and the bundled UEVR backend (required)."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecVRHome} "Adds a 'Path of Titans VR' tile to SteamVR Home so you can start the game straight from your headset. Requires Steam + SteamVR (restart SteamVR after installing)."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

; ---- Uninstall (this version only; other versions are left alone) ----
Section "Uninstall"
  SetRegView 64

  ; Drop our SteamVR Home registration before deleting the exe.
  nsExec::ExecToLog '"$INSTDIR\${EXE_NAME}" --unregister'
  Pop $0

  Delete "$INSTDIR\pot-vr-launcher.exe"
  Delete "$INSTDIR\pottvr.vrmanifest"
  Delete "$INSTDIR\vr_image.png"
  Delete "$INSTDIR\UEVRBackend.dll"
  Delete "$INSTDIR\openvr_api.dll"
  Delete "$INSTDIR\UEVRPluginNullifier.dll"
  Delete "$INSTDIR\openxr_loader.dll"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir  "$INSTDIR"

  Delete "$SMPROGRAMS\${APP_NAME}\${APP_NAME} ${VERSION}.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}\Uninstall ${APP_NAME} ${VERSION}.lnk"
  Delete "$DESKTOP\${APP_NAME} ${VERSION}.lnk"
  ; Also drop un-versioned shortcuts left by pre-0.2 installs.
  Delete "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}\Uninstall ${APP_NAME}.lnk"
  Delete "$DESKTOP\${APP_NAME}.lnk"
  ; Only remove the start-menu folder if it's now empty (RMDir won't recurse).
  RMDir  "$SMPROGRAMS\${APP_NAME}"

  DeleteRegKey HKCU "${ARP_KEY}"
  DeleteRegKey HKCU "${THIS_VER}"

  ; If no other versions remain, drop the root key.
  EnumRegKey $R0 HKCU "${VERSIONS}" 0
  StrCmp $R0 "" clean_root
    Goto end
  clean_root:
    DeleteRegKey HKCU "${VERSIONS}"
    DeleteRegKey HKCU "${APP_REG}"
  end:
SectionEnd

; SPDX-License-Identifier: GPL-3.0-or-later
;
; NSIS hooks for the Windows installer (tauri.bundle.conf.json).
;
; Installing, an update included, first stops paste-get if it runs: the tray and `pasteget`
; both, so that their files can be replaced and the new version is the one that runs.
; Uninstalling stops them too, then removes what paste-get registered for itself: starting at
; login, its notification identity (desktop/src/notify.rs) and the icon it shows. Settings and
; paired devices, in %APPDATA%\paste-get, are kept.

; Ends this user's pasteget-tray.exe and pasteget.exe, if they run. Registers are kept.
!macro PASTEGET_STOP
  Push $0
  Push $1
  ReadEnvStr $0 USERNAME
  ; Without a user name the filter would match everyone's.
  ${If} $0 != ""
    nsExec::Exec '"$SYSDIR\taskkill.exe" /F /FI "USERNAME eq $0" /IM pasteget-tray.exe /IM pasteget.exe'
    Pop $1
    ; Let Windows release their files.
    Sleep 1000
  ${EndIf}
  Pop $1
  Pop $0
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro PASTEGET_STOP
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro PASTEGET_STOP
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Starting at login: the value desktop/src/autostart.rs writes for the default profile. Tauri's
  ; own uninstaller removes the one named after the product, the same name today; this keeps it
  ; removed if either changes. Kept on an update, like Tauri's.
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "paste-get"
  ${EndIf}
  DeleteRegKey HKCU "Software\Classes\AppUserModelId\io.github.nexoryx.pasteget"
  Delete "$LOCALAPPDATA\paste-get\notification-icon.png"
  RMDir "$LOCALAPPDATA\paste-get"
!macroend

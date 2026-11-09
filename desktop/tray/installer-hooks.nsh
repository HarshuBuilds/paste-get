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

; Removes starting at login, for every profile: the values desktop/src/autostart.rs writes in
; the Run key, "paste-get" for the default profile and "paste-get (<profile>)" for the others.
!macro PASTEGET_REMOVE_LOGIN_ENTRIES
  Push $0
  Push $1
  Push $2
  StrCpy $0 0
  ${Do}
    ClearErrors
    EnumRegValue $1 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" $0
    ; No more values.
    ${If} ${Errors}
    ${OrIf} $1 == ""
      ${Break}
    ${EndIf}
    StrCpy $2 $1 11
    ${If} $1 == "paste-get"
    ${OrIf} $2 == "paste-get ("
      ClearErrors
      DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" $1
      ; The next value takes this one's index, unless this one stayed.
      ${If} ${Errors}
        IntOp $0 $0 + 1
      ${EndIf}
    ${Else}
      IntOp $0 $0 + 1
    ${EndIf}
  ${Loop}
  Pop $2
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
  ; Starting at login, for every profile. Tauri's own uninstaller removes only the value named
  ; after the product, the default profile's today. Kept on an update, like Tauri's.
  ${If} $UpdateMode <> 1
    !insertmacro PASTEGET_REMOVE_LOGIN_ENTRIES
  ${EndIf}
  DeleteRegKey HKCU "Software\Classes\AppUserModelId\io.github.nexoryx.pasteget"
  Delete "$LOCALAPPDATA\paste-get\notification-icon.png"
  RMDir "$LOCALAPPDATA\paste-get"
!macroend

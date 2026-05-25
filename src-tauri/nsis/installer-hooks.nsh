; ============================================================
; Ramz Launcher - Custom NSIS Installer Hooks
; ============================================================
!macro NSIS_HOOK_PREINSTALL
  ; Silent uninstall of previous versions
  ReadRegStr $R0 SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ramz Launcher" "UninstallString"
  ${If} $R0 != ""
    ExecWait '$R0 /S _?=$INSTDIR'
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Launch the launcher after install/upgrade
  IfFileExists "$INSTDIR\ramz-launcher.exe" 0 done
    nsis_tauri_utils::RunAsUser "$INSTDIR\ramz-launcher.exe" ""
    Goto done
  done:
!macroend

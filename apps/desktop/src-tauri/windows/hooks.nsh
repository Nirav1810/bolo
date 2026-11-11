; Bolo's additions to Tauri's per-user NSIS installer (M3 ticket 11).
;
; Bolo keeps its files outside Tauri's own app-data folders, so the uninstaller handles
; them here. None of this runs when the uninstaller runs as part of an update.
;   %LOCALAPPDATA%\Bolo   speech models, dictation history, logs: asked about (No when silent)
;   %APPDATA%\Bolo        settings, and the API keys in Credential Manager: removed only
;                         when "Delete the application data" is ticked

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    ; "Start with Windows" would point at the removed exe.
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Bolo"

    SetShellVarContext current
    IfFileExists "$LOCALAPPDATA\Bolo\*.*" 0 bolo_data_done
      MessageBox MB_YESNO|MB_ICONQUESTION "Also delete Bolo's downloaded speech models and your dictation history?$\r$\n$\r$\nThey are in $LOCALAPPDATA\Bolo." /SD IDNO IDNO bolo_data_done
      RmDir /r "$LOCALAPPDATA\Bolo"
    bolo_data_done:

    ${If} $DeleteAppDataCheckboxState = 1
      RmDir /r "$APPDATA\Bolo"
      ; Credential Manager entries are named <provider>.bolo.
      nsExec::Exec 'cmdkey.exe /delete:anthropic.bolo'
      Pop $0
      nsExec::Exec 'cmdkey.exe /delete:deepgram.bolo'
      Pop $0
      nsExec::Exec 'cmdkey.exe /delete:groq.bolo'
      Pop $0
      nsExec::Exec 'cmdkey.exe /delete:sarvam.bolo'
      Pop $0
    ${EndIf}
  ${EndIf}
!macroend

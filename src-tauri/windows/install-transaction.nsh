; Derive/create each destination parent from the resource's relative filename.
; Keep path assembly in NSIS macros: a backslash before a Handlebars expression
; escapes interpolation and can silently leave literal {{this}} directories.
!macro ExtractInstallerFile source relative
  StrCpy $HelperDetail "create_payload_root: $PLUGINSDIR\payload"
  ClearErrors
  SetOutPath "$PLUGINSDIR\payload"
  Call CheckPayloadExtraction
  ${GetParent} "$PLUGINSDIR\payload\${relative}" $0
  StrCpy $HelperDetail "create_payload_directory: $0"
  ClearErrors
  CreateDirectory "$0"
  Call CheckPayloadExtraction
  StrCpy $HelperDetail "extract_payload_file: ${relative}"
  ClearErrors
  ; Do not offer Abort/Retry/Ignore for a required package file. Check the error
  ; immediately and terminate through the installer's single failure message.
  SetOverwrite try
  File /a "/oname=${relative}" "${source}"
  SetOverwrite lastused
  Call CheckPayloadExtraction
!macroend

!macro LogMissingInstallerFile relative
  ${IfNot} ${FileExists} "$ExistingInstallDir\${relative}"
    StrCpy $HelperDetail "restore_missing_file: ${relative}"
    Call LogInstallerStage
  ${EndIf}
!macroend

; CopyFileW has no Shell overwrite/retry dialogs and captures its own Win32
; failure immediately. Keep process status separate from filesystem failures.
!macro CopyInstallerFile source destination
  System::Call 'kernel32::CopyFileW(w "${source}", w "${destination}", i0) i.r0 ?e'
  Pop $HelperNativeError
  ${If} $0 = 0
    SetErrors
    Return
  ${EndIf}
  StrCpy $HelperNativeError 0
!macroend

; Snapshot only package-owned files, never the whole application/data directory.
; Each macro leaves errors set on failure. Callers own failure/repair policy.
!macro BackupInstallerFile relative
  StrCpy $0 "$INSTDIR\${relative}"
  Call ValidateInstallerPath
  ${If} ${Errors}
    Return
  ${EndIf}
  StrCpy $0 "$InstallBackup\files\${relative}"
  Call ValidateInstallerPath
  ${If} ${Errors}
    Return
  ${EndIf}
  StrCpy $0 "$InstallBackup\absent\${relative}"
  Call ValidateInstallerPath
  ${If} ${Errors}
    Return
  ${EndIf}
  ${IfNot} ${FileExists} "$InstallBackup\files\${relative}"
  ${AndIfNot} ${FileExists} "$InstallBackup\absent\${relative}"
    ClearErrors
    ${If} ${FileExists} "$INSTDIR\${relative}"
      ${GetParent} "$InstallBackup\files\${relative}" $0
      CreateDirectory "$0"
      ${If} ${Errors}
        Return
      ${EndIf}
      !insertmacro CopyInstallerFile "$INSTDIR\${relative}" "$InstallBackup\files\${relative}"
    ${Else}
      ${GetParent} "$InstallBackup\absent\${relative}" $0
      CreateDirectory "$0"
      FileOpen $0 "$InstallBackup\absent\${relative}" w
      FileClose $0
    ${EndIf}
    ${If} ${Errors}
      Return
    ${EndIf}
  ${EndIf}
!macroend

!macro ApplyInstallerFile relative
  ${GetParent} "$INSTDIR\${relative}" $0
  ClearErrors
  CreateDirectory "$0"
  ${If} ${Errors}
    Return
  ${EndIf}
  !insertmacro CopyInstallerFile "$PLUGINSDIR\payload\${relative}" "$INSTDIR\${relative}"
  ${If} ${Errors}
    Return
  ${EndIf}
!macroend

!macro RestoreInstallerFile relative
  ClearErrors
  ${If} ${FileExists} "$InstallBackup\files\${relative}"
    !insertmacro CopyInstallerFile "$InstallBackup\files\${relative}" "$INSTDIR\${relative}"
  ${ElseIf} ${FileExists} "$InstallBackup\absent\${relative}"
    Delete "$INSTDIR\${relative}"
  ${Else}
    SetErrors
  ${EndIf}
  ${If} ${Errors}
    Return
  ${EndIf}
!macroend

!macro WriteInstallerReg kind key name value
  ClearErrors
  WriteReg${kind} SHCTX "${key}" "${name}" "${value}"
  ${If} ${Errors}
    StrCpy $HelperDetail "installation_registry_write_failed"
    Goto install_post_failed
  ${EndIf}
!macroend

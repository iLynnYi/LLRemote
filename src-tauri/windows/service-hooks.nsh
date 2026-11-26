; Keep native backslashes: Windows NSIS splits include paths only on "\".
!include "${__FILEDIR__}\install-safety.nsh"
!include "${__FILEDIR__}\install-transaction.nsh"

Var UninstallTargetSid
Var UninstallHelperPath
Var UninstallDirectoryArgs
Var UninstallLog
Var UninstallWarningCount
Var UninstallWarningText
Var UninstallWarningsLog
Var UninstallReportIndex
Var UninstallReportPath
Var UninstallHelperExit

!macro LLREMOTE_UNINSTALL_REASON
  ${If} $R0 = 101
    StrCpy $R1 "$(UninstallOwnerUnknown)"
  ${ElseIf} $R0 = 102
    StrCpy $R1 "$(UninstallOwnerMismatch)"
  ${ElseIf} $R0 = 103
    StrCpy $R1 "$(UninstallOwnerLoginRequired)"
  ${ElseIf} $R0 = 104
    StrCpy $R1 "$(UninstallOwnerContextFailed)"
  ${ElseIf} $R0 = 105
    StrCpy $R1 "$(UninstallServiceOwnerUnknown)"
  ${ElseIf} $R0 = 106
    StrCpy $R1 "$(UninstallProfileUnavailable)"
  ${ElseIf} $R0 = 107
    StrCpy $R1 "$(UninstallProcessStopFailed)"
  ${ElseIf} $R0 = 108
    StrCpy $R1 "$(UninstallServiceAuthorizationRequired)"
  ${ElseIf} $R0 = 109
    StrCpy $R1 "$(UninstallBackgroundStopFailed)"
  ${ElseIf} $R0 = 110
    StrCpy $R1 "$(UninstallShortcutCleanupFailed)"
  ${ElseIf} $R0 = 111
    StrCpy $R1 "$(UninstallStartupCleanupFailed)"
  ${ElseIf} $R0 = 112
    StrCpy $R1 "$(UninstallRegistryCleanupFailed)"
  ${ElseIf} $R0 = 113
    StrCpy $R1 "$(UninstallCredentialCleanupFailed)"
  ${ElseIf} $R0 = 114
    StrCpy $R1 "$(UninstallDataCleanupFailed)"
  ${ElseIf} $R0 = 115
    StrCpy $R1 "$(UninstallDataInUse)"
  ${ElseIf} $R0 = 116
    StrCpy $R1 "$(UninstallDataAccessDenied)"
  ${ElseIf} $R0 = 117
    StrCpy $R1 "$(UninstallDataUnsafePath)"
  ${ElseIf} $R0 = 118
    StrCpy $R1 "$(UninstallServiceCleanupFailed)"
  ${ElseIf} $R0 = 119
    StrCpy $R1 "$(UninstallServiceAccessDenied)"
  ${ElseIf} $R0 = 120
    StrCpy $R1 "$(UninstallServiceTimeout)"
  ${ElseIf} $R0 = 121
    StrCpy $R1 "$(UninstallHelperMissing)"
  ${ElseIf} $R0 = 122
    StrCpy $R1 "$(UninstallHelperCopyFailed)"
  ${ElseIf} $R0 = 123
    StrCpy $R1 "$(UninstallHelperLaunchFailed)"
  ${ElseIf} $R0 = 124
    StrCpy $R1 "$(UninstallProcessOwnerUnknown)"
  ${ElseIf} $R0 = 125
    StrCpy $R1 "$(UninstallProcessAccessDenied)"
  ${ElseIf} $R0 = 126
    StrCpy $R1 "$(UninstallProcessQueryFailed)"
  ${ElseIf} $R0 = 127
    StrCpy $R1 "$(UninstallProcessStopTimeout)"
  ${ElseIf} $R0 = 128
    StrCpy $R1 "$(UninstallOwnerTaskTimeout)"
  ${ElseIf} $R0 = 129
    StrCpy $R1 "$(UninstallInvalidConfiguration)"
  ${ElseIf} $R0 = 130
    StrCpy $R1 "$(UninstallAdminRequired)"
  ${ElseIf} $R0 = 131
    StrCpy $R1 "$(UninstallServiceStopFailed)"
  ${ElseIf} $R0 = 132
    StrCpy $R1 "$(UninstallServiceCredentialFailed)"
  ${ElseIf} $R0 = 133
    StrCpy $R1 "$(UninstallReportFailed)"
  ${ElseIf} $R0 = 134
    StrCpy $R1 "$(UninstallFilesFailed)"
  ${Else}
    StrCpy $R1 "$(UninstallUnknownFailure)"
  ${EndIf}
!macroend

!macro LLREMOTE_UNINSTALL_FAILURE
  !insertmacro LLREMOTE_UNINSTALL_LOG "removal_failed: exit=$R0"
  !insertmacro LLREMOTE_UNINSTALL_REASON
  MessageBox MB_ICONSTOP|MB_OK "$R1$\r$\n$\r$\n$(UninstallFailureContext)" /SD IDOK
  SetErrorLevel 1
  Abort
!macroend

!macro LLREMOTE_UNINSTALL_WARNING_REASON
  ${If} $R0 = 106
    StrCpy $R1 "$(UninstallWarnProfile)"
  ${ElseIf} $R0 = 105
    StrCpy $R1 "$(UninstallWarnUnownedServiceData)"
  ${ElseIf} $R0 = 119
    StrCpy $R1 "$(UninstallWarnProtectedData)"
  ${ElseIf} $R0 = 103
    StrCpy $R1 "$(UninstallWarnCredentialsOffline)"
  ${ElseIf} $R0 = 113
    StrCpy $R1 "$(UninstallWarnCredentials)"
  ${ElseIf} $R0 = 132
    StrCpy $R1 "$(UninstallWarnServiceCredentials)"
  ${ElseIf} $R0 = 115
    StrCpy $R1 "$(UninstallWarnDataInUse)"
  ${ElseIf} $R0 = 116
    StrCpy $R1 "$(UninstallWarnDataAccess)"
  ${ElseIf} $R0 = 117
    StrCpy $R1 "$(UninstallWarnDataPath)"
  ${ElseIf} $R0 = 110
    StrCpy $R1 "$(UninstallWarnShortcut)"
  ${ElseIf} $R0 = 111
    StrCpy $R1 "$(UninstallWarnRegistry)"
  ${ElseIf} $R0 = 112
    StrCpy $R1 "$(UninstallWarnRegistry)"
  ${ElseIf} $R0 = 133
    StrCpy $R1 "$(UninstallWarnReport)"
  ${Else}
    StrCpy $R1 "$(UninstallWarnOther)"
  ${EndIf}
!macroend

!macro LLREMOTE_UNINSTALL_WARN code location
  StrCpy $R0 "${code}"
  StrCpy $R2 "${location}"
  !insertmacro LLREMOTE_UNINSTALL_WARNING_REASON
  IntOp $UninstallWarningCount $UninstallWarningCount + 1
  ${If} $UninstallWarningCount <= 3
    StrCpy $UninstallWarningText "$UninstallWarningText$\r$\n$R1$\r$\n$R2"
  ${EndIf}
  DetailPrint "$R1 : $R2"
  ${If} $UninstallWarningsLog == ""
    GetTempFileName $UninstallWarningsLog "$TEMP"
    StrCpy $R8 "$UninstallWarningsLog.txt"
    Rename "$UninstallWarningsLog" "$R8"
    ${IfNot} ${Errors}
      StrCpy $UninstallWarningsLog "$R8"
    ${EndIf}
    FileOpen $R7 "$UninstallWarningsLog" w
    ${IfNot} ${Errors}
      FileWriteUTF16LE /BOM $R7 "$(UninstallCleanupLogTitle)$\r$\n$INSTDIR$\r$\n"
      FileClose $R7
    ${EndIf}
  ${EndIf}
  FileOpen $R7 "$UninstallWarningsLog" a
  ${IfNot} ${Errors}
    FileSeek $R7 0 END
    FileWriteUTF16LE $R7 "$R1$\r$\n$R2$\r$\nCode: $R0$\r$\n$\r$\n"
    FileClose $R7
  ${EndIf}
  ClearErrors
!macroend

!macro LLREMOTE_UNINSTALL_READ_REPORT
  ClearErrors
  ReadINIStr $R4 "$UninstallReportPath" "cleanup" "version"
  ${If} ${Errors}
  ${OrIf} $R4 != "1"
    !insertmacro LLREMOTE_UNINSTALL_WARN 133 "$UninstallReportPath"
  ${Else}
    ReadINIStr $R4 "$UninstallReportPath" "cleanup" "reboot"
    ${If} $R4 == "1"
      SetRebootFlag true
    ${EndIf}
    ReadINIStr $R4 "$UninstallReportPath" "cleanup" "count"
    ${If} $R4 == ""
    ${OrIf} $R4 < 0
    ${OrIf} $R4 > 256
      !insertmacro LLREMOTE_UNINSTALL_WARN 133 "$UninstallReportPath"
    ${Else}
      StrCpy $R5 0
      ${DoWhile} $R5 < $R4
        ClearErrors
        ReadINIStr $R0 "$UninstallReportPath" "warning$R5" "code"
        ReadINIStr $R6 "$UninstallReportPath" "warning$R5" "path"
        ${If} ${Errors}
        ${OrIf} $R0 == ""
          StrCpy $R0 133
          StrCpy $R6 "$UninstallReportPath"
        ${EndIf}
        !insertmacro LLREMOTE_UNINSTALL_WARN "$R0" "$R6"
        IntOp $R5 $R5 + 1
      ${Loop}
      ${If} $R4 > 0
        IntOp $UninstallReportIndex $UninstallReportIndex + 1
        CopyFiles /SILENT "$UninstallReportPath" "$UninstallWarningsLog.$UninstallReportIndex.ini"
        FileOpen $R7 "$UninstallWarningsLog" a
        ${IfNot} ${Errors}
          FileSeek $R7 0 END
          FileWriteUTF16LE $R7 "$UninstallWarningsLog.$UninstallReportIndex.ini$\r$\n"
          FileClose $R7
        ${EndIf}
      ${ElseIf} $UninstallHelperExit = 200
      ${OrIf} $UninstallHelperExit = 202
        !insertmacro LLREMOTE_UNINSTALL_WARN 133 "$UninstallReportPath"
      ${EndIf}
    ${EndIf}
  ${EndIf}
  ClearErrors
!macroend

!macro LLREMOTE_UNINSTALL_COMPLETION
  StrCpy $R3 0
  IfRebootFlag 0 +2
    StrCpy $R3 1
  ${If} $R3 = 1
    StrCpy $R1 "$(UninstallRebootTitle)"
  ${Else}
    StrCpy $R1 "$(UninstallWarningTitle)"
  ${EndIf}
  ${If} $UninstallWarningCount > 0
    ${If} ${FileExists} "$UninstallWarningsLog"
      StrCpy $R2 "$(UninstallWarningsLogLocation)"
    ${Else}
      StrCpy $R2 "$(UninstallWarningsLogUnavailable)"
    ${EndIf}
    MessageBox MB_ICONEXCLAMATION|MB_OK "$R1$UninstallWarningText$\r$\n$\r$\n$R2" /SD IDOK
  ${ElseIf} $R3 = 1
    MessageBox MB_ICONINFORMATION|MB_OK "$(UninstallRebootTitle)" /SD IDOK
  ${EndIf}
!macroend

; Fresh current-user installs never register a service. Existing services in
; either scope update through their own authorization path. No listener starts.
!macro NSIS_HOOK_POSTINSTALL
  ; Payload has been verified before any service replacement. Failures from
  ; here leave an explicitly marked repairable installation, not a success page.
  StrCpy $HelperPath "$INSTDIR\${MAINBINARYNAME}.exe"
  StrCpy $HelperTimeout 180000
  StrCpy $InstallStage "login_registration"
  StrCpy $HelperArgs "--finish-update"
  Call RunInstallerHelper
  ${If} $HelperExit != 0
    Goto install_post_failed
  ${EndIf}
  ${If} $UpgradeHadService = 1
    Goto service_postinstall_update
  ${EndIf}
  IfShellVarContextAll service_postinstall_update service_postinstall_done
  service_postinstall_update:
    ; Mark uncertainty BEFORE invoking a mutator: a crash or partial service
    ; registration must never trigger an unsafe automatic application rollback.
    WriteINIStr "$INSTDIR\.llremote-install-recovery.ini" "recovery" "stage" "service_update"
    ${If} ${Errors}
      StrCpy $HelperDetail "service_recovery_record_could_not_be_written"
      Goto install_post_failed
    ${EndIf}
    StrCpy $InstallStage "service_update"
    StrCpy $HelperArgs "--installer-service"
    Call RunInstallerHelper
    ${If} $HelperExit != 0
      Goto install_post_failed
    ${EndIf}
  service_postinstall_done:
!macroend

!macro LLREMOTE_UNINSTALL_LOG detail
  DetailPrint "${detail}"
  ${If} $UninstallLog != ""
    Push $R7
    FileOpen $R7 "$UninstallLog" a
    ${IfNot} ${Errors}
      FileSeek $R7 0 END
      FileWriteUTF16LE $R7 "${detail}$\r$\n"
      FileClose $R7
    ${EndIf}
    Pop $R7
    ClearErrors
  ${EndIf}
!macroend

!macro LLREMOTE_DELETE_UNINSTALL_FILE relative options
  StrCpy $0 "$INSTDIR\${relative}"
  Call un.ValidateInstallPath
  ${If} ${Errors}
    StrCpy $R0 134
    !insertmacro LLREMOTE_UNINSTALL_FAILURE
  ${EndIf}
  ${If} ${FileExists} "$INSTDIR\${relative}"
    ClearErrors
    Delete ${options} "$INSTDIR\${relative}"
    ${If} ${Errors}
      StrCpy $R0 134
      !insertmacro LLREMOTE_UNINSTALL_FAILURE
    ${EndIf}
  ${Else}
    !insertmacro LLREMOTE_UNINSTALL_LOG "already_absent: ${relative}"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Updates retain both runtimes, login registration, configuration and secrets.
  ; Personal cleanup always targets the verified owner/original operator,
  ; including when runas uses a different administrator's credentials.
  ${If} $UpdateMode <> 1
    InitPluginsDir
    StrCpy $UninstallReportPath "$PLUGINSDIR\uninstall-prepare.ini"
    StrCpy $R3 ""
    ${If} $UninstallTargetSid != ""
      StrCpy $R3 "$R3 --target-user $UninstallTargetSid"
    ${EndIf}
    ${If} $DeleteAppDataCheckboxState == 1
      StrCpy $R3 "$R3 --delete-data"
    ${EndIf}
    ClearErrors
    ExecWait '"$UninstallHelperPath" --prepare-uninstall$R3$UninstallDirectoryArgs --uninstall-result "$UninstallReportPath"' $R0
    ${If} ${Errors}
      StrCpy $R0 123
      !insertmacro LLREMOTE_UNINSTALL_FAILURE
    ${ElseIf} $R0 != 0
    ${AndIf} $R0 != 200
    ${AndIf} $R0 != 201
    ${AndIf} $R0 != 202
      !insertmacro LLREMOTE_UNINSTALL_FAILURE
    ${EndIf}
    ${If} $R0 = 201
    ${OrIf} $R0 = 202
      SetRebootFlag true
    ${EndIf}
    StrCpy $UninstallHelperExit $R0
    !insertmacro LLREMOTE_UNINSTALL_READ_REPORT
    !insertmacro LLREMOTE_UNINSTALL_LOG "prepared_removal: $UninstallHelperExit"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    StrCpy $UninstallReportPath "$PLUGINSDIR\uninstall-finish.ini"
    ClearErrors
    ${If} $DeleteAppDataCheckboxState = 1
      ExecWait '"$UninstallHelperPath" --finish-uninstall --target-user $UninstallTargetSid --delete-data$UninstallDirectoryArgs --uninstall-result "$UninstallReportPath"' $R0
    ${Else}
      ExecWait '"$UninstallHelperPath" --finish-uninstall --target-user $UninstallTargetSid$UninstallDirectoryArgs --uninstall-result "$UninstallReportPath"' $R0
    ${EndIf}
    ${If} ${Errors}
      !insertmacro LLREMOTE_UNINSTALL_WARN 123 "$INSTDIR"
    ${ElseIf} $R0 != 0
    ${AndIf} $R0 != 200
    ${AndIf} $R0 != 201
    ${AndIf} $R0 != 202
      !insertmacro LLREMOTE_UNINSTALL_WARN "$R0" "$INSTDIR"
    ${Else}
      StrCpy $UninstallHelperExit $R0
      ${If} $R0 = 201
      ${OrIf} $R0 = 202
        SetRebootFlag true
      ${EndIf}
      !insertmacro LLREMOTE_UNINSTALL_READ_REPORT
    ${EndIf}
  ${EndIf}
!macroend

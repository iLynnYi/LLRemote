!macro LLREMOTE_VALIDATE_INSTALL_PATH
  ${DoWhile} $0 != ""
    System::Call 'kernel32::GetFileAttributesW(w r0) i.r1 ?e'
    Pop $2
    ${If} $1 = -1
      ${If} $2 != 2
      ${AndIf} $2 != 3
        SetErrors
        Return
      ${EndIf}
    ${Else}
      IntOp $1 $1 & 0x400
      ${If} $1 != 0
        SetErrors
        Return
      ${EndIf}
    ${EndIf}
    ${GetParent} "$0" $0
  ${Loop}
  ClearErrors
!macroend

; Shared by the real installer and the executable regression harness.
; NSIS is x86 even when the payload is x64/ARM64. Never use its process
; architecture or environment variables as the system/payload architecture.
!macro LLREMOTE_INSTALLER_FUNCTIONS
Var InstallLog
Var InstallStage
Var InstallSystem
Var InstallNativeMachine
Var InstallNativeArchitecture
Var InstallHasRuntime
Var InstallHasApp
Var InstallBackup
Var InstallUncertain
Var InstallRecoveryMessage
Var InstallerPrompt
Var HelperPath
Var HelperArgs
Var HelperTimeout
Var HelperExit
Var HelperNativeError
Var HelperExplanation
Var HelperDetail
Var HelperReport
Var HelperInherit
Var InstallMutex
Var PeMachine
Var MachineSupported

LangString InstallerDialogCaption ${LANG_SIMPCHINESE} "$(^Name) 安装"
LangString InstallerDialogCaption ${LANG_ENGLISH} "$(^Name) Setup"
LangString InstallerFailed ${LANG_SIMPCHINESE} "安装未完成$\r$\n$\r$\n$HelperExplanation"
LangString InstallerFailed ${LANG_ENGLISH} "Installation did not complete$\r$\n$\r$\n$HelperExplanation"
LangString InstallerIncompatible ${LANG_SIMPCHINESE} "当前 Windows 无法运行此安装包。需要 Windows 10 1809 或更新版本，且安装包架构必须与 Windows 系统原生架构完全一致。请使用适合此系统的安装包。"
LangString InstallerIncompatible ${LANG_ENGLISH} "This package cannot run on this Windows system. Windows 10 1809 or later and an exact match with the native Windows architecture are required. Use a compatible package."
LangString InstallerArchitectureMismatch ${LANG_SIMPCHINESE} "请使用 $InstallNativeArchitecture 版本的安装包$\r$\n$\r$\n此电脑需要 $InstallNativeArchitecture 版本，当前打开的是 ${ARCH} 版本。$\r$\n$\r$\n请下载 Windows $InstallNativeArchitecture 安装包后重新安装。"
LangString InstallerArchitectureMismatch ${LANG_ENGLISH} "Please use the $InstallNativeArchitecture installer$\r$\n$\r$\nThis computer needs the $InstallNativeArchitecture version. You opened the ${ARCH} version.$\r$\n$\r$\nDownload the Windows $InstallNativeArchitecture installer and try again."
LangString InstallerExtractionError ${LANG_SIMPCHINESE} "无法完整解压安装文件。请检查磁盘空间和临时文件夹的写入权限；如被安全软件拦截，请查看其记录后重试。"
LangString InstallerExtractionError ${LANG_ENGLISH} "The installation files could not be extracted. Check free disk space and access to the temporary folder. If security software blocked the installer, review its notification before retrying."
LangString InstallerPartial ${LANG_SIMPCHINESE} "应用文件已写入，但安装尚未全部完成。共享保持停止；请保留日志，重新运行相同或更新版本安装包进行修复。"
LangString InstallerPartial ${LANG_ENGLISH} "Application files were written, but installation is incomplete. Sharing remains stopped. Keep the log and repair using the same or a newer installer."
LangString InstallerRestored ${LANG_SIMPCHINESE} "本次应用文件变更已撤回。已停止的共享不会自动恢复，请在原版本中手动开始共享。"
LangString InstallerRestored ${LANG_ENGLISH} "Application file changes were rolled back. Stopped sharing is not restarted automatically; start it manually in the previous version."

LangString InstallerEnvironmentError ${LANG_SIMPCHINESE} "安装程序未能完成操作。请重新运行安装程序；若仍然失败，请联系支持人员。"
LangString InstallerEnvironmentError ${LANG_ENGLISH} "The installer could not complete an operation. Run it again. If the problem continues, contact support."
LangString InstallerOwnerError ${LANG_SIMPCHINESE} "无法确认或保存这份安装所属的账户。请使用原安装账户重试，并检查安装目录的写入权限。"
LangString InstallerOwnerError ${LANG_ENGLISH} "The installation owner could not be verified or saved. Retry using the original account and check write access to the installation folder."
LangString InstallerCancelledUAC ${LANG_SIMPCHINESE} "已取消管理员授权。安装未完成；需要更新已有服务时，请允许本次授权。"
LangString InstallerCancelledUAC ${LANG_ENGLISH} "Administrator approval was cancelled. Approve maintenance when updating an existing service."
LangString InstallerDeniedUAC ${LANG_SIMPCHINESE} "系统策略或权限拒绝了管理员操作。请联系管理员检查应用控制策略和服务权限。"
LangString InstallerDeniedUAC ${LANG_ENGLISH} "Windows policy or permissions denied administrator maintenance. Ask an administrator to check application-control policy and service access."
LangString InstallerWebViewError ${LANG_SIMPCHINESE} "未检测到可供此应用使用的 WebView2 Runtime。请联网重试，或通过 Microsoft 官方离线安装程序安装运行时后重试。"
LangString InstallerWebViewError ${LANG_ENGLISH} "A usable WebView2 Runtime was not found. Retry online or install the runtime using Microsoft's official offline installer."
LangString InstallerBusy ${LANG_SIMPCHINESE} "另一个安装或维护进程尚未退出。请等待其完成后重试，避免同时修改应用和服务。"
LangString InstallerBusy ${LANG_ENGLISH} "Another installer or maintenance process has not exited. Wait for it to finish before retrying."

LangString InstallerPayloadError ${LANG_SIMPCHINESE} "安装包内部程序损坏，或实际架构与安装包标注不一致。请重新获取正确的安装包。"
LangString InstallerPayloadError ${LANG_ENGLISH} "The packaged executable is damaged or its architecture differs from the package label. Obtain a correct package."
LangString InstallerLoadError ${LANG_SIMPCHINESE} "安装包内程序无法在此系统正常启动。请保留诊断日志，核对系统环境与安装包；本次不会替换旧应用。"
LangString InstallerLoadError ${LANG_ENGLISH} "The packaged application cannot start on this system. Keep the log and check the environment and package. The previous application has not been replaced."
LangString InstallerStopError ${LANG_SIMPCHINESE} "无法安全停止旧应用或共享。请结束远程会话、退出应用并停止共享后重试。"
LangString InstallerStopError ${LANG_ENGLISH} "The previous application or sharing could not stop safely. End remote sessions, exit the application and stop sharing before retrying."
LangString InstallerWriteError ${LANG_SIMPCHINESE} "无法写入安装文件。请检查安装文件夹的权限和磁盘空间，并关闭可能占用文件的应用后重试。"
LangString InstallerWriteError ${LANG_ENGLISH} "The installation files could not be written. Check folder access and free disk space, then close applications that may be using the files and try again."

Function LogInstallerStage
  IntFmt $7 "0x%08X" $HelperExit
  DetailPrint "$InstallStage: $HelperDetail (system=$HelperNativeError, exit=$HelperExit/$7)"
  FileOpen $8 "$InstallLog" a
  FileSeek $8 0 END
  FileWrite $8 "system=$InstallSystem; native_machine=$InstallNativeMachine; package=${ARCH}$\r$\n"
  FileWrite $8 "$InstallStage: $HelperDetail (system=$HelperNativeError, exit=$HelperExit/$7)$\r$\n"
  FileClose $8
FunctionEnd

Function FailInstaller
  Call LogInstallerStage
  StrCpy $HelperExplanation "$(InstallerEnvironmentError)"
  ${If} $InstallStage == "system_compatibility"
    StrCpy $HelperExplanation "$(InstallerIncompatible)"
  ${ElseIf} $InstallStage == "architecture_mismatch"
    StrCpy $HelperExplanation "$(InstallerArchitectureMismatch)"
  ${ElseIf} $InstallStage == "extract_payload"
    StrCpy $HelperExplanation "$(InstallerExtractionError)"
  ${ElseIf} $InstallStage == "installer_lock"
    StrCpy $HelperExplanation "$(InstallerBusy)"
  ${ElseIf} $HelperExit = 85
    StrCpy $HelperExplanation "$(InstallerCancelledUAC)"
  ${ElseIf} $HelperExit = 86
  ${OrIf} $HelperExit = 87
    StrCpy $HelperExplanation "$(InstallerDeniedUAC)"
  ${ElseIf} $HelperExit = 94
  ${OrIf} $InstallStage == "webview2_install"
  ${OrIf} $InstallStage == "webview2_update"
    StrCpy $HelperExplanation "$(InstallerWebViewError)"
  ${ElseIf} $InstallStage == "payload_architecture"
    StrCpy $HelperExplanation "$(InstallerPayloadError)"
  ${ElseIf} $InstallStage == "preflight_runtime"
    StrCpy $HelperExplanation "$(InstallerLoadError)"
  ${ElseIf} $InstallStage == "stop_sharing"
  ${OrIf} $InstallStage == "close_application"
    StrCpy $HelperExplanation "$(InstallerStopError)"
  ${ElseIf} $InstallStage == "destination_check"
  ${OrIf} $InstallStage == "backup_application"
  ${OrIf} $InstallStage == "replace_application"
    StrCpy $HelperExplanation "$(InstallerWriteError)"
  ${ElseIf} $InstallStage == "register_installation_owner"
    StrCpy $HelperExplanation "$(InstallerOwnerError)"
  ${EndIf}
  ${If} $InstallRecoveryMessage != ""
    StrCpy $HelperExplanation "$HelperExplanation$\r$\n$\r$\n$InstallRecoveryMessage"
  ${EndIf}
  ${If} $InstallStage == "architecture_mismatch"
    StrCpy $InstallerPrompt "$(InstallerArchitectureMismatch)"
  ${Else}
    StrCpy $InstallerPrompt "$(InstallerFailed)"
  ${EndIf}
  Call ShowInstallerFailureDialog
  SetErrorLevel 1
  Quit
FunctionEnd

; Keep the native message box, but give its caption the same application icon
; and standard icon-to-text spacing as the installer. Whitespace-only caption
; prefixes are trimmed by Windows and cannot reliably provide an inset.
Function ShowInstallerFailureDialog
  IfSilent installer_dialog_done
  System::Call 'kernel32::GetModuleHandleW(p0) p.r3'
  System::Call 'user32::LoadIconW(p r3, p103) p.R6'
  ${If} $R6 = 0
    System::Call 'user32::LoadIconW(p0, p32512) p.R6'
  ${EndIf}
  ; A thread-local CBT hook updates only this message box as it is activated.
  ; System callback objects must be resumed/freed as objects, not raw addresses.
  System::Get '(i.r0, p.r1, p.r2) psR9'
  Pop $R8
  ${If} $R8 == "error"
  ${OrIf} $R8 == 0
    MessageBox MB_ICONSTOP|MB_OK "$InstallerPrompt" /SD IDOK
    Return
  ${EndIf}
  System::Call 'kernel32::GetCurrentThreadId() i.r3'
  System::Call 'user32::SetWindowsHookExW(i5, k R8, p0, i r3) p.R7'
  StrCpy $R9 ""
  System::Call 'user32::MessageBoxW(p $HWNDPARENT, w "$InstallerPrompt", w "$(InstallerDialogCaption)", i0x10) i.R5'
  installer_dialog_callback:
    StrCpy $R4 $R9 8
    ${If} $R4 == "callback"
      ${If} $0 = 5
        ; HCBT_ACTIVATE: stop observing before changing the frame. Removing
        ; WS_EX_DLGMODALFRAME lets Windows draw the small caption icon normally.
        System::Call 'user32::UnhookWindowsHookEx(p R7)'
        StrCpy $R7 0
        System::Call 'user32::GetWindowLongW(p r1, i-20) i.r3'
        IntOp $3 $3 & 0xFFFFFFFE
        System::Call 'user32::SetWindowLongW(p r1, i-20, i r3)'
        System::Call 'user32::SendMessageW(p r1, i0x80, p0, p R6)'
        System::Call 'user32::SetWindowPos(p r1, p0, i0, i0, i0, i0, i0x37)'
      ${EndIf}
      System::Call 'user32::CallNextHookEx(p R7, i r0, p r1, p r2) p.r4'
      Push $4
      StrCpy $R9 ""
      System::Call $R8
      Goto installer_dialog_callback
    ${EndIf}
  ${If} $R7 != 0
    System::Call 'user32::UnhookWindowsHookEx(p R7)'
  ${EndIf}
  System::Free $R8
  installer_dialog_done:
FunctionEnd

Function PreflightCompatibility
  ClearErrors
  CreateDirectory "$TEMP\LLRemote\Installer"
  GetTempFileName $InstallLog "$TEMP\LLRemote\Installer"
  ${If} ${Errors}
    StrCpy $InstallStage "temporary_directory_check"
    StrCpy $HelperDetail "temporary_directory_is_not_writable"
    Call FailInstaller
  ${EndIf}
  StrCpy $HelperExit -1
  StrCpy $HelperNativeError 0
  StrCpy $InstallStage "system_compatibility"
  StrCpy $InstallSystem "unknown"
  StrCpy $HelperDetail "$(InstallerIncompatible)"
  ; RtlGetVersion is independent of application compatibility/version shims.
  System::Alloc 284
  Pop $0
  System::Call '*$0(i 284)'
  System::Call 'ntdll::RtlGetVersion(p r0) i.r1'
  System::Call '*$0(i, i.r2, i.r3, i.r4)'
  System::Free $0
  StrCpy $InstallSystem "Windows $2.$3 build $4"
  ${If} $1 != 0
  ${OrIf} $2 < 10
  ${OrIf} $4 < 17763
    Call FailInstaller
  ${EndIf}
  System::Call 'kernel32::GetModuleHandleW(w "kernel32.dll") p.r0'
  System::Call 'kernel32::GetProcAddress(p r0, m "IsWow64Process2") p.r1'
  ${If} $1 = 0
    Call FailInstaller
  ${EndIf}
  ; GetProcAddress returns a raw API address, not a System::Get procedure.
  ; The :: prefix is required; calling the address as a procedure crashes.
  ; Initialize the 32-bit output slots to zero before writing USHORT values.
  ; Input 0 + output r2 is *i0r2, not *i0.r2 (which discards output).
  System::Call '::$1(p -1, *i0r2, *i0r3) i.r4'
  StrCpy $InstallNativeMachine $3
  StrCpy $InstallNativeArchitecture "unknown"
  ${Switch} $InstallNativeMachine
    ${Case} 34404
      StrCpy $InstallNativeArchitecture "x64"
      StrCpy $InstallSystem "$InstallSystem; x64"
      ${Break}
    ${Case} 43620
      StrCpy $InstallNativeArchitecture "ARM64"
      StrCpy $InstallSystem "$InstallSystem; ARM64"
      ${Break}
    ${Case} 332
      StrCpy $InstallNativeArchitecture "x86"
      StrCpy $InstallSystem "$InstallSystem; x86"
      ${Break}
    ${Default}
      StrCpy $InstallSystem "$InstallSystem; native_machine=$3"
  ${EndSwitch}
  ${If} $4 = 0
    Call FailInstaller
  ${EndIf}
  !if "${ARCH}" == "x64"
    StrCpy $5 34404
  !else if "${ARCH}" == "arm64"
    StrCpy $5 43620
  !else if "${ARCH}" == "x86"
    StrCpy $5 332
  !else
    !error "Unsupported Windows payload architecture"
  !endif
  StrCpy $PeMachine $5
  Call CheckPayloadMachineSupport
  ${If} $MachineSupported != 1
    StrCpy $InstallStage "architecture_mismatch"
    StrCpy $HelperDetail "native=$InstallNativeArchitecture; package=${ARCH}; exact_architecture_required"
    Call FailInstaller
  ${EndIf}
  StrCpy $HelperDetail "compatible"
  Call LogInstallerStage
FunctionEnd

Function CheckPayloadMachineSupport
  ; Product policy requires native packages, even when Windows can emulate
  ; another architecture. Compare the payload with the OS, never the x86 stub.
  StrCpy $MachineSupported 0
  ${If} $InstallNativeMachine = $PeMachine
    ${If} $PeMachine = 332
    ${OrIf} $PeMachine = 34404
    ${OrIf} $PeMachine = 43620
      StrCpy $MachineSupported 1
    ${EndIf}
  ${EndIf}
FunctionEnd

Function CheckPayloadExtraction
  ${If} ${Errors}
    System::Call 'kernel32::GetLastError() i.r0'
    StrCpy $HelperNativeError $0
    StrCpy $HelperExit -1
    Call FailInstaller
  ${EndIf}
FunctionEnd

Function LockInstaller
  StrCpy $InstallStage "installer_lock"
  ; Elevated SCM helpers reopen SYNCHRONIZE-only handles across UAC, where
  ; Windows does not inherit handles. Other users may retain, never mutate, it.
  System::Call 'advapi32::ConvertStringSecurityDescriptorToSecurityDescriptorW(w "D:P(A;;0x00100000;;;WD)(A;;GA;;;SY)(A;;GA;;;BA)", i1, *p.r6, p0) i.r0'
  ${If} $0 = 0
    StrCpy $HelperDetail "unable_to_secure_installer_lock"
    Call FailInstaller
  ${EndIf}
  System::Call '*(i12, p r6, i1) p.r7'
  System::Call 'kernel32::CreateMutexW(p r7, i0, w "Global\LLRemote.Installer.v1") p.r0 ?e'
  Pop $1
  System::Free $7
  System::Call 'kernel32::LocalFree(p r6)'

  ${If} $0 = 0
  ${OrIf} $1 = 183
    StrCpy $HelperNativeError $1
    StrCpy $HelperDetail "another_installer_or_maintenance_process_is_running"
    Call FailInstaller
  ${EndIf}
  StrCpy $InstallMutex $0
  ; Helpers retain the lock even if their parent times out. Another installer
  ; must not restore/copy files while the old mutator is still running.
  System::Call 'kernel32::SetHandleInformation(p r0, i1, i1) i.r1'
  ${If} $1 = 0
    StrCpy $HelperDetail "unable_to_inherit_installer_lock"
    Call FailInstaller
  ${EndIf}
  StrCpy $HelperInherit 1
FunctionEnd

; Reject existing junction/symlink components before copying or deleting. Paths
; are package-owned destinations or backup paths, never user data locations.
Function ValidateInstallerPath
  !insertmacro LLREMOTE_VALIDATE_INSTALL_PATH
FunctionEnd

; Read and validate the actual staged payload's PE machine before executing it.
Function VerifyPayloadArchitecture
  StrCpy $InstallStage "payload_architecture"
  StrCpy $HelperDetail "invalid_or_mismatched_payload"
  ClearErrors
  FileOpen $0 "$HelperPath" r
  ${If} ${Errors}
    Call FailInstaller
  ${EndIf}
  FileSeek $0 0 END $6
  FileSeek $0 0 SET
  FileReadByte $0 $1
  FileReadByte $0 $2
  ${If} $1 != 77
  ${OrIf} $2 != 90
  ${OrIf} $6 < 64
    FileClose $0
    Call FailInstaller
  ${EndIf}
  FileSeek $0 60 SET
  FileReadByte $0 $1
  FileReadByte $0 $2
  FileReadByte $0 $3
  FileReadByte $0 $4
  IntOp $2 $2 << 8
  IntOp $3 $3 << 16
  IntOp $4 $4 << 24
  IntOp $1 $1 | $2
  IntOp $1 $1 | $3
  IntOp $1 $1 | $4
  IntOp $5 $1 + 6
  ${If} $1 < 64
  ${OrIf} $5 < $1
  ${OrIf} $5 > $6
    FileClose $0
    Call FailInstaller
  ${EndIf}
  FileSeek $0 $1 SET
  FileReadByte $0 $1
  FileReadByte $0 $2
  FileReadByte $0 $3
  FileReadByte $0 $4
  FileReadByte $0 $5
  FileReadByte $0 $6
  FileClose $0
  IntOp $6 $6 << 8
  IntOp $PeMachine $5 | $6
  !if "${ARCH}" == "x64"
    StrCpy $7 34404
  !else if "${ARCH}" == "arm64"
    StrCpy $7 43620
  !else
    StrCpy $7 332
  !endif
  ${If} $1 != 80
  ${OrIf} $2 != 69
  ${OrIf} $3 != 0
  ${OrIf} $4 != 0
  ${OrIf} $PeMachine != $7
    Call FailInstaller
  ${EndIf}
FunctionEnd

; Capture CreateProcess's own last error immediately, independently from the
; child's exit/loader status. Suppress loader dialogs and require a versioned
; handshake, so a successful process launch is not mistaken for a ready app.
Function RunInstallerHelper
  StrCpy $HelperExit -1
  StrCpy $HelperNativeError 0
  StrCpy $HelperDetail "process_launch_failed"
  StrCpy $HelperReport "$PLUGINSDIR\maintenance.ini"
  Delete "$HelperReport"
  System::Call 'kernel32::SetErrorMode(i 0x8003) i.r9'
  System::Call '*(i 68, p0, p0, p0, i0, i0, i0, i0, i0, i0, i0, i0, &i2 0, &i2 0, p0, p0, p0, p0) p.r1'
  System::Call '*(p0, p0, i0, i0) p.r2'
  System::Call `kernel32::CreateProcessW(p0, w '"$HelperPath" $HelperArgs --installer-report "$HelperReport"', p0, p0, i $HelperInherit, i 0x08000000, p0, p0, p r1, p r2) i.r0 ?e`
  Pop $HelperNativeError
  System::Call '*$2(p.r3, p.r4, i, i)'
  System::Free $1
  System::Free $2
  ${If} $0 != 0
    StrCpy $HelperNativeError 0
    System::Call 'kernel32::CloseHandle(p r4)'
    System::Call 'kernel32::WaitForSingleObject(p r3, i $HelperTimeout) i.r0'
    ${If} $0 = 0
      System::Call 'kernel32::GetExitCodeProcess(p r3, *i .r0) i.r1 ?e'
      Pop $HelperNativeError
      ${If} $1 != 0
        StrCpy $HelperNativeError 0
        StrCpy $HelperExit $0
        StrCpy $HelperDetail "process_exited_without_maintenance_report"
        ReadINIStr $1 "$HelperReport" "maintenance" "protocol"
        ReadINIStr $2 "$HelperReport" "maintenance" "result"
        ${If} $1 == 1
        ${AndIf} $2 == $HelperExit
          ReadINIStr $HelperDetail "$HelperReport" "maintenance" "detail"
        ${ElseIf} $HelperExit = 0
        ${OrIf} $HelperExit = 10
        ${OrIf} $HelperExit = 11
        ${OrIf} $HelperExit = 12
        ${OrIf} $HelperExit = 13
          StrCpy $HelperExit 89
        ${EndIf}
      ${EndIf}
    ${Else}
      ; Never roll back underneath a mutator that may still be running.
      StrCpy $InstallUncertain 1
      StrCpy $HelperDetail "maintenance_timeout_or_wait_failed; do not retry until the process exits"
      StrCpy $HelperNativeError $0
    ${EndIf}
    System::Call 'kernel32::CloseHandle(p r3)'
  ${EndIf}
  System::Call 'kernel32::SetErrorMode(i r9)'
  ${If} $HelperDetail == "process_exited_without_maintenance_report"
    ${Switch} $HelperExit
      ${Case} 80
        StrCpy $HelperDetail "invalid_installer_arguments"
        ${Break}
      ${Case} 85
        StrCpy $HelperDetail "windows_service_uac_cancelled"
        ${Break}
      ${Case} 86
        StrCpy $HelperDetail "windows_service_elevation_denied_by_policy"
        ${Break}
      ${Case} 87
        StrCpy $HelperDetail "windows_service_access_denied"
        ${Break}
      ${Case} 88
        StrCpy $HelperDetail "windows_service_ipc_timeout"
        ${Break}
      ${Case} 89
        StrCpy $HelperDetail "maintenance_report_missing_or_unwritable"
        ${Break}
    ${EndSwitch}
  ${EndIf}
  Call LogInstallerStage
FunctionEnd

Function ProbeInstallerPayload
  InitPluginsDir
  StrCpy $InstallStage "extract_payload"
  !insertmacro ExtractInstallerFile "${MAINBINARYSRCPATH}" "${MAINBINARYNAME}.exe"
  StrCpy $HelperPath "$PLUGINSDIR\payload\${MAINBINARYNAME}.exe"
  Call VerifyPayloadArchitecture
  StrCpy $InstallStage "preflight_runtime"
  StrCpy $HelperArgs "--installer-check"
  StrCpy $HelperTimeout 15000
  Call RunInstallerHelper
  ${If} $HelperExit != 0
    Call FailInstaller
  ${EndIf}
FunctionEnd

Function ReadInstallerState
  StrCpy $InstallStage "existing_runtime_check"
  StrCpy $HelperArgs "--installer-state"
  Call RunInstallerHelper
  StrCpy $UpgradeHadService 0
  StrCpy $InstallHasRuntime 0
  ${Switch} $HelperExit
    ${Case} 0
      ${Break}
    ${Case} 10
      StrCpy $UpgradeHadService 1
      ${Break}
    ${Case} 11
    ${Case} 13
      StrCpy $UpgradeHadService 1
      StrCpy $InstallHasRuntime 1
      ${Break}
    ${Case} 12
      StrCpy $InstallHasRuntime 1
      ${Break}
    ${Default}
      Call FailInstaller
  ${EndSwitch}
FunctionEnd
!macroend

; Based on tauri-bundler v2.9.3 NSIS template (MIT OR Apache-2.0).
; https://github.com/tauri-apps/tauri/blob/tauri-bundler-v2.9.3/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi
Unicode true
ManifestDPIAware true
; Add in `dpiAwareness` `PerMonitorV2` to manifest for Windows 10 1607+ (note this should not affect lower versions since they should be able to ignore this and pick up `dpiAware` `true` set by `ManifestDPIAware true`)
; Currently undocumented on NSIS's website but is in the Docs folder of source tree, see
; https://github.com/kichik/nsis/blob/5fc0b87b819a9eec006df4967d08e522ddd651c9/Docs/src/attributes.but#L286-L300
; https://github.com/tauri-apps/tauri/pull/10106
ManifestDPIAwareness PerMonitorV2
!if "{{compression}}" == "none"
  SetCompress off
!else
  ; Set the compression algorithm. We default to LZMA.
  SetCompressor /SOLID "{{compression}}"
!endif

; Keep above !include to stay ahead of any plugin command
; see https://github.com/tauri-apps/tauri/pull/15422#discussion_r3289239624
{{#if signed_plugins_path}}
!addplugindir "{{signed_plugins_path}}"
{{/if}}
!include MUI2.nsh
!include FileFunc.nsh
!include nsDialogs.nsh
!include x64.nsh
!include WordFunc.nsh
!include "utils.nsh"
!include "FileAssociation.nsh"
!include "Win\COM.nsh"
!include "Win\Propkey.nsh"
!include "StrFunc.nsh"
${StrCase}
${StrLoc}

{{#if installer_hooks}}
!include "{{installer_hooks}}"
{{/if}}

!define WEBVIEW2APPGUID "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
!define MANUFACTURER "{{manufacturer}}"
!define PRODUCTNAME "{{product_name}}"
!define VERSION "{{version}}"
!define VERSIONWITHBUILD "{{version_with_build}}"
!define HOMEPAGE "{{homepage}}"
!define INSTALLMODE "{{install_mode}}"
!define LICENSE "{{license}}"
!define INSTALLERICON "{{installer_icon}}"
!define SIDEBARIMAGE "{{sidebar_image}}"
!define HEADERIMAGE "{{header_image}}"
!define UNINSTALLERICON "{{uninstaller_icon}}"
!define UNINSTALLERHEADERIMAGE "{{uninstaller_header_image}}"
!define MAINBINARYNAME "{{main_binary_name}}"
!define MAINBINARYSRCPATH "{{main_binary_path}}"
!define BUNDLEID "{{bundle_id}}"
!define COPYRIGHT "{{copyright}}"
!define OUTFILE "{{out_file}}"
!define ARCH "{{arch}}"
!define ADDITIONALPLUGINSPATH "{{additional_plugins_path}}"
!define ALLOWDOWNGRADES "{{allow_downgrades}}"
!define DISPLAYLANGUAGESELECTOR "{{display_language_selector}}"
!define WEBVIEW2INSTALLERPATH "$%LLREMOTE_WEBVIEW2_INSTALLER%"
!if "${WEBVIEW2INSTALLERPATH}" == ""
  !error "Build through pnpm installer:windows to supply the native WebView2 offline installer."
!endif
!define MINIMUMWEBVIEW2VERSION "{{minimum_webview2_version}}"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCTNAME}"
!define MANUKEY "Software\${MANUFACTURER}"
!define MANUPRODUCTKEY "${MANUKEY}\${PRODUCTNAME}"
!define UNINSTALLERSIGNCOMMAND "{{uninstaller_sign_cmd}}"
!define ESTIMATEDSIZE "{{estimated_size}}"
!define STARTMENUFOLDER "{{start_menu_folder}}"
Var PassiveMode
Var UpdateMode
Var NoShortcutMode
Var ExistingInstall
Var ExistingVersion
Var ExistingComparison
Var ExistingInstallDir
Var UpgradeHadService

Name "${PRODUCTNAME}"
BrandingText "${COPYRIGHT}"
OutFile "${OUTFILE}"
; Keep download and extraction details visible during long operations.
ShowInstDetails show

; We don't actually use this value as default install path,
; it's just for nsis to append the product name folder in the directory selector
; https://nsis.sourceforge.io/Reference/InstallDir
!define PLACEHOLDER_INSTALL_DIR "placeholder\${PRODUCTNAME}"
InstallDir "${PLACEHOLDER_INSTALL_DIR}"
VIProductVersion "${VERSIONWITHBUILD}"
VIAddVersionKey "ProductName" "${PRODUCTNAME}"
VIAddVersionKey "FileDescription" "${PRODUCTNAME}"
VIAddVersionKey "LegalCopyright" "${COPYRIGHT}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
# additional plugins
!addplugindir "${ADDITIONALPLUGINSPATH}"

; Uninstaller signing command
!if "${UNINSTALLERSIGNCOMMAND}" != ""
  !uninstfinalize '${UNINSTALLERSIGNCOMMAND}'
!endif

; Start without elevation. An administrator choosing AllUsers resumes this
; same executable with runas, after the choice and before any file writes.
RequestExecutionLevel user
Var MultiUser.InstallMode
Var ScopeAccountType
Var ScopeResumed
Var ScopeCurrentRadio
Var ScopeAllRadio
Var ScopeExistingCurrent
Var ScopeExistingMachine
; Installer icon
!if "${INSTALLERICON}" != ""
  !define MUI_ICON "${INSTALLERICON}"
!endif

; Installer sidebar image
!if "${SIDEBARIMAGE}" != ""
  !define MUI_WELCOMEFINISHPAGE_BITMAP "${SIDEBARIMAGE}"
!endif

; Enable header images for installer and uninstaller pages when either image is configured.
!if "${HEADERIMAGE}" != ""
  !define MUI_HEADERIMAGE
!else if "${UNINSTALLERHEADERIMAGE}" != ""
  !define MUI_HEADERIMAGE
!endif
; Installer header image
!if "${HEADERIMAGE}" != ""
  !define MUI_HEADERIMAGE_BITMAP "${HEADERIMAGE}"
!endif

; Uninstaller header image
!if "${UNINSTALLERHEADERIMAGE}" != ""
  !define MUI_HEADERIMAGE_UNBITMAP "${UNINSTALLERHEADERIMAGE}"
!endif

; Uninstaller icon
!if "${UNINSTALLERICON}" != ""
  !define MUI_UNICON "${UNINSTALLERICON}"
!endif
; Define registry key to store installer language
!define MUI_LANGDLL_REGISTRY_ROOT "HKCU"
!define MUI_LANGDLL_REGISTRY_KEY "${MANUPRODUCTKEY}"
!define MUI_LANGDLL_REGISTRY_VALUENAME "Installer Language"

; Installer pages, must be ordered as they appear
; 1. Welcome Page
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassiveOrResume
!insertmacro MUI_PAGE_WELCOME
; 2. License Page (if defined)
!if "${LICENSE}" != ""
  !define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassiveOrResume
  !insertmacro MUI_PAGE_LICENSE "${LICENSE}"
!endif

; 3. Scope is selected before elevation, in this installer's own wizard.
Page custom ScopePage ScopeLeave
; 4. Custom page to ask user if he wants to reinstall/uninstall
;    only if a previous installation was detected
Var ReinstallPageCheck
Page custom PageReinstall PageLeaveReinstall
Function PageReinstall
  Call DetectExistingInstallation
  ${If} $ExistingInstall != 1
  ${OrIf} $PassiveMode = 1
  ${OrIf} $UpdateMode = 1
    Abort
  ${EndIf}
  !insertmacro MUI_HEADER_TEXT "$(alreadyInstalled)" "$(chooseMaintenanceOption)"
  nsDialogs::Create 1018
  Pop $R4
  ${IfThen} $(^RTL) = 1 ${|} nsDialogs::SetRTL $(^RTL) ${|}
  ${NSD_CreateLabel} 0 0 100% 32u "$(UpgradeDescription)"
  Pop $R1
  ${If} $ExistingComparison = 0
    ${NSD_CreateRadioButton} 30u 50u -30u 12u "$(RepairAction)"
    Pop $R2
    ${NSD_CreateRadioButton} 30u 74u -30u 12u "$(uninstallApp)"
    Pop $R3
  ${Else}
    ${NSD_CreateRadioButton} 30u 50u -30u 12u "$(UpgradeAction)"
    Pop $R2
    ${NSD_CreateRadioButton} 30u 74u -30u 12u "$(CancelInstallation)"
    Pop $R3
  ${EndIf}
  ${NSD_OnClick} $R2 PageReinstallUpdateSelection
  ${NSD_OnClick} $R3 PageReinstallUpdateSelection
  ${If} $ReinstallPageCheck = 2
    ${NSD_Check} $R3
  ${Else}
    ${NSD_Check} $R2
  ${EndIf}
  nsDialogs::Show
FunctionEnd
Function PageReinstallUpdateSelection
  ${NSD_GetState} $R2 $R1
  ${If} $R1 == ${BST_CHECKED}
    StrCpy $ReinstallPageCheck 1
  ${Else}
    StrCpy $ReinstallPageCheck 2
  ${EndIf}
FunctionEnd
Function PageLeaveReinstall
  StrCpy $R1 1
  ${If} $PassiveMode <> 1
  ${AndIfNot} ${Silent}
    ${NSD_GetState} $R2 $R1
  ${EndIf}
  ${If} $UpdateMode = 1
    Return
  ${EndIf}
  ; Upgrade and repair never launch the old uninstaller. Only the explicit
  ; same-version Uninstall choice is allowed to clean up the installation.
  ${If} $R1 = 1
    Return
  ${EndIf}
  ${If} $ExistingComparison <> 0
    Quit
  ${EndIf}
  Call LaunchPackagedUninstaller
  Quit
FunctionEnd

; 5. Choose install directory page
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipDirectoryForExisting
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE ValidateMachineDirectory
!insertmacro MUI_PAGE_DIRECTORY
; 6. Start menu shortcut page
Var AppStartMenuFolder
!if "${STARTMENUFOLDER}" != ""
  !define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
  !define MUI_STARTMENUPAGE_DEFAULTFOLDER "${STARTMENUFOLDER}"
!else
  !define MUI_PAGE_CUSTOMFUNCTION_PRE Skip
!endif
!insertmacro MUI_PAGE_STARTMENU Application $AppStartMenuFolder

; 7. Installation page
!insertmacro MUI_PAGE_INSTFILES
; 8. Finish page
;
; Don't auto jump to finish page after installation page,
; because the installation page has useful info that can be used debug any issues with the installer.
!define MUI_FINISHPAGE_NOAUTOCLOSE
; Use show readme button in the finish page as a button create a desktop shortcut
!define MUI_FINISHPAGE_SHOWREADME
!define MUI_FINISHPAGE_SHOWREADME_TEXT "$(createDesktop)"
!define MUI_FINISHPAGE_SHOWREADME_FUNCTION CreateOrUpdateDesktopShortcut
; Show run app after installation.
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_CHECKED
!define MUI_FINISHPAGE_RUN_FUNCTION RunMainBinary
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_FINISH
Function RunMainBinary
  nsis_tauri_utils::RunAsUser "$INSTDIR\${MAINBINARYNAME}.exe" ""
FunctionEnd
; Uninstaller Pages
; 1. Confirm uninstall page
Var DeleteAppDataCheckbox
Var DeleteAppDataCheckboxState
Var UninstallScope
Var UninstallUserLabel
Var UninstallScopeLabel
Var UninstallServiceLabel
Var UninstallResultFile
Function un.ConfirmShow
  Call un.SkipIfPassive
  !insertmacro MUI_HEADER_TEXT "$(^UninstallCaption)" ""
  nsDialogs::Create 1018
  Pop $0
  ${If} $0 == error
    Abort
  ${EndIf}
  ${NSD_CreateLabel} 0 0 100% 12u "$UninstallScopeLabel"
  Pop $0
  ${NSD_CreateLabel} 0 17u 66u 12u "$(UninstallAccountLabel)"
  Pop $0
  ${NSD_CreateText} 70u 14u -70u 16u "$UninstallUserLabel"
  Pop $0
  SendMessage $0 ${EM_SETREADONLY} 1 0
  ${NSD_CreateLabel} 0 38u 66u 12u "$(UninstallPathLabel)"
  Pop $0
  ${NSD_CreateText} 70u 35u -70u 16u "$INSTDIR"
  Pop $0
  SendMessage $0 ${EM_SETREADONLY} 1 0
  ${NSD_CreateLabel} 0 58u 100% 28u "$UninstallServiceLabel"
  Pop $0
  ${NSD_CreateLabel} 0 89u 100% 12u "$(UninstallPreserveFiles)"
  Pop $0
  ${NSD_CreateCheckbox} 0 104u 100% 24u "$(CompleteUninstallLabel)"
  Pop $DeleteAppDataCheckbox
  ${NSD_SetState} $DeleteAppDataCheckbox $DeleteAppDataCheckboxState
  GetDlgItem $0 $HWNDPARENT 1
  ${NSD_SetText} $0 "$(^UninstallBtn)"
  nsDialogs::Show
FunctionEnd
Function un.ConfirmLeave
  ${NSD_GetState} $DeleteAppDataCheckbox $DeleteAppDataCheckboxState
FunctionEnd
UninstPage custom un.ConfirmShow un.ConfirmLeave
; 2. Uninstalling Page
!insertmacro MUI_UNPAGE_INSTFILES

;Languages
{{#each languages}}
!insertmacro MUI_LANGUAGE "{{this}}"
{{/each}}
!insertmacro MUI_RESERVEFILE_LANGDLL
{{#each language_files}}
  !include "{{this}}"
{{/each}}

LangString CompleteUninstallLabel ${LANG_SIMPCHINESE} "同时删除上述用户的设置、缓存和保存的密码"
LangString CompleteUninstallLabel ${LANG_ENGLISH} "Also delete this user's settings, caches and saved passwords"
LangString UninstallAccountLabel ${LANG_SIMPCHINESE} "个人清理账户"
LangString UninstallAccountLabel ${LANG_ENGLISH} "Cleanup account"
LangString UninstallPathLabel ${LANG_SIMPCHINESE} "安装位置"
LangString UninstallPathLabel ${LANG_ENGLISH} "Install location"
LangString UninstallPreserveFiles ${LANG_SIMPCHINESE} "其他账户的数据以及截图、录像等用户文件会保留。"
LangString UninstallPreserveFiles ${LANG_ENGLISH} "Other accounts' data and user-created files will be kept."
LangString UninstallScopeUser ${LANG_SIMPCHINESE} "个人安装"
LangString UninstallScopeUser ${LANG_ENGLISH} "per-user installation"
LangString UninstallScopeAll ${LANG_SIMPCHINESE} "所有用户共用的安装"
LangString UninstallScopeAll ${LANG_ENGLISH} "all-users installation"
LangString UninstallServiceRemove ${LANG_SIMPCHINESE} "此安装关联的整机共享服务也会移除，可能影响其他用户。"
LangString UninstallServiceRemove ${LANG_ENGLISH} "The machine sharing service associated with this installation will also be removed, affecting other users."
LangString UninstallServiceKeep ${LANG_SIMPCHINESE} "没有关联的共享服务需要移除；其他安装的服务会保留。"
LangString UninstallServiceKeep ${LANG_ENGLISH} "No associated sharing service needs removal; services owned by other installations will be kept."
LangString UninstallServiceAuthorizationRequired ${LANG_SIMPCHINESE} "此程序关联的系统共享服务需要管理员授权才能移除。本次卸载已取消，程序和个人数据未删除。请联系管理员协助卸载。"
LangString UninstallServiceAuthorizationRequired ${LANG_ENGLISH} "Removing the system sharing service associated with this application requires administrator approval. Uninstall was cancelled; application files and personal data have not been deleted. Contact an administrator for help."
LangString UninstallOwnerUnknown ${LANG_SIMPCHINESE} "无法确认这份安装所属的账户或安装记录。卸载已中止。请由管理员运行卸载程序；仍失败时，请先以原账户使用新版安装包修复安装。"
LangString UninstallOwnerUnknown ${LANG_ENGLISH} "The installation owner or registration could not be verified. Uninstall was stopped. Run as administrator; if this still fails, repair the installation using the original account and a current installer."
LangString UninstallOwnerLoginRequired ${LANG_SIMPCHINESE} "删除该用户保存的密码需要其登录 Windows。请让上述用户登录后重试，或取消勾选个人数据清理后进行普通卸载。尚未开始本次清理。"
LangString UninstallOwnerLoginRequired ${LANG_ENGLISH} "Deleting this user's saved passwords requires that user to be signed in to Windows. Sign in and retry, or keep personal data by clearing the checkbox. Cleanup has not started."
LangString UninstallOwnerContextFailed ${LANG_SIMPCHINESE} "Windows 未能以目标账户运行个人清理，卸载已中止。请确认该账户仍登录且系统允许计划任务运行，或切换到该账户卸载；无需重复申请管理员授权。"
LangString UninstallOwnerContextFailed ${LANG_ENGLISH} "Windows could not run personal cleanup as the target user. Keep that user signed in and allow Task Scheduler to run, or uninstall from that account. Repeating administrator approval is not required."
LangString UninstallProfileUnavailable ${LANG_SIMPCHINESE} "无法打开安装所属账户的配置目录或注册表，卸载已中止。请检查该账户的用户配置是否完整，或让该账户登录后重试。"
LangString UninstallProfileUnavailable ${LANG_ENGLISH} "The installation owner's profile folder or registry could not be opened. Check that the profile is intact, or sign in to that account and retry."
LangString UninstallServiceOwnerUnknown ${LANG_SIMPCHINESE} "无法确认现有共享服务属于哪份安装，卸载已中止。请先在原应用的设置中更新或修复服务；也可由管理员单独卸载服务后重试。"
LangString UninstallServiceOwnerUnknown ${LANG_ENGLISH} "The sharing service's owning installation could not be verified. Uninstall was stopped. Update or repair the service in the original app, or have an administrator remove the service separately and retry."
LangString UninstallFilesFailed ${LANG_SIMPCHINESE} "部分程序文件未能删除，卸载未完成。请关闭使用此安装的程序和共享后台，并检查文件权限后重试。"
LangString UninstallFilesFailed ${LANG_ENGLISH} "Some application files could not be deleted. Uninstall is incomplete. Close applications and sharing processes using this installation, check file permissions, and retry."
LangString UninstallProcessStopFailed ${LANG_SIMPCHINESE} "无法关闭仍在使用这份安装的主程序，卸载已中止。请关闭该程序后重试；跨账户卸载请使用管理员权限。"
LangString UninstallProcessStopFailed ${LANG_ENGLISH} "The main application using this installation could not be closed. Uninstall was stopped. Close it and retry; use administrator privileges when uninstalling across accounts."
LangString UninstallOwnerMismatch ${LANG_SIMPCHINESE} "卸载目标账户与安装记录不一致。请从原安装目录重新卸载；仍失败时，请以安装所属账户修复安装后重试。"
LangString UninstallOwnerMismatch ${LANG_ENGLISH} "The target account does not match the installation record. Run uninstall from the original directory, or repair the installation as its owner and retry."
LangString UninstallBackgroundStopFailed ${LANG_SIMPCHINESE} "目标账户的共享后台未能退出。请在安装所属账户中停止本机共享并退出托盘后台后重试；仍无响应时可重启 Windows。"
LangString UninstallBackgroundStopFailed ${LANG_ENGLISH} "The target sharing background process could not exit. Stop local sharing and exit its tray process in the owner account, then retry. Restart Windows if unresponsive."
LangString UninstallShortcutCleanupFailed ${LANG_SIMPCHINESE} "目标账户的桌面、开始菜单或登录快捷方式清理失败。请登录安装所属账户，检查快捷方式是否损坏及目录的删除权限后重试。"
LangString UninstallShortcutCleanupFailed ${LANG_ENGLISH} "Target desktop, Start menu or startup shortcuts could not be cleaned. Sign in to the owner account, check for damaged shortcuts and directory delete permissions, then retry."
LangString UninstallStartupCleanupFailed ${LANG_SIMPCHINESE} "无法移除目标账户的登录自启动记录。请登录安装所属账户，关闭 LL Remote 的登录自启动后重试；若注册表受策略保护，请联系管理员。"
LangString UninstallStartupCleanupFailed ${LANG_ENGLISH} "Target startup registration could not be removed. Sign in to the owner account, disable LL Remote login startup and retry. Contact an administrator if registry policy blocks it."
LangString UninstallRegistryCleanupFailed ${LANG_SIMPCHINESE} "无法读取或更新目标账户的安装注册表记录。请让该账户登录后重试；仍失败时，请由管理员检查注册表访问权限，或以原账户修复安装。"
LangString UninstallRegistryCleanupFailed ${LANG_ENGLISH} "The target installation registry could not be read or updated. Sign in to that account and retry. Have an administrator check registry access, or repair as the owner if it still fails."
LangString UninstallCredentialCleanupFailed ${LANG_SIMPCHINESE} "目标账户保存的密码未能删除。请登录安装所属账户并确认 Windows 凭据管理器可用后重试；也可取消个人数据清理，保留数据进行普通卸载。"
LangString UninstallCredentialCleanupFailed ${LANG_ENGLISH} "Target saved passwords could not be deleted. Sign in to the owner account and check Windows Credential Manager, then retry. Alternatively, clear personal-data cleanup to keep data and uninstall normally."
LangString UninstallDataCleanupFailed ${LANG_SIMPCHINESE} "应用的设置或缓存未能删除。请检查应用数据目录是否可访问及磁盘状态，处理后重试；若只需移除程序，可取消勾选个人数据清理。"
LangString UninstallDataCleanupFailed ${LANG_ENGLISH} "Application settings or caches could not be deleted. Check access to the data directories and disk condition, then retry. Clear personal-data cleanup if you only need to remove the application."
LangString UninstallDataInUse ${LANG_SIMPCHINESE} "应用的数据文件正在被其他进程使用或锁定。请退出安装所属账户中的 LL Remote、托盘共享后台及占用文件的程序后重试；必要时重启 Windows。"
LangString UninstallDataInUse ${LANG_ENGLISH} "Application data files are in use or locked. Exit LL Remote, its sharing tray process and other programs using the files in the owner account, then retry. Restart Windows if needed."
LangString UninstallDataAccessDenied ${LANG_SIMPCHINESE} "Windows 拒绝删除应用的数据或缓存目录。请登录安装所属账户，检查这些目录的删除权限；需要时请管理员协助后重试。"
LangString UninstallDataAccessDenied ${LANG_ENGLISH} "Windows denied deletion of application data or caches. Sign in to the owner account and check directory delete permissions. Ask an administrator for help if needed, then retry."
LangString UninstallDataUnsafePath ${LANG_SIMPCHINESE} "应用数据目录包含符号链接或重解析点，为避免删除外部文件已停止清理。请核对并恢复实际数据目录，或取消个人数据清理后卸载。"
LangString UninstallDataUnsafePath ${LANG_ENGLISH} "Application data directories contain symbolic links or reparse points. Cleanup stopped to protect external files. Verify and restore the actual data directories, or clear personal-data cleanup and retry."
LangString UninstallServiceCleanupFailed ${LANG_SIMPCHINESE} "关联的系统共享服务未能完整移除。请由管理员在本机共享设置中修复或单独卸载该服务，然后重试应用卸载。"
LangString UninstallServiceCleanupFailed ${LANG_ENGLISH} "The associated system sharing service could not be fully removed. Have an administrator repair or separately uninstall it in Local Sharing settings, then retry."
LangString UninstallServiceAccessDenied ${LANG_SIMPCHINESE} "Windows 拒绝访问或修改关联的系统共享服务。请由管理员运行卸载；已提权仍失败时，请联系系统管理员检查服务权限和安全策略。"
LangString UninstallServiceAccessDenied ${LANG_ENGLISH} "Windows denied access to the associated sharing service. Run uninstall as administrator. If already elevated, ask your system administrator to check service permissions and security policy."
LangString UninstallServiceTimeout ${LANG_SIMPCHINESE} "关联的系统共享服务未及时响应。请由管理员停止该服务后重试；服务无响应时可重启 Windows 后再卸载。"
LangString UninstallServiceTimeout ${LANG_ENGLISH} "The associated sharing service did not respond in time. Have an administrator stop it and retry. Restart Windows if the service is unresponsive."
LangString UninstallHelperMissing ${LANG_SIMPCHINESE} "卸载所需的程序文件缺失或不可用。请使用相同或更新版本的安装包，在原目录修复安装后重新卸载。"
LangString UninstallHelperMissing ${LANG_ENGLISH} "A program file required for uninstall is missing or unavailable. Repair the original installation with the same or a newer installer, then uninstall again."
LangString UninstallHelperCopyFailed ${LANG_SIMPCHINESE} "无法在临时目录解压卸载辅助程序。请检查磁盘空间和临时目录的写入权限后重试。"
LangString UninstallHelperCopyFailed ${LANG_ENGLISH} "The uninstall helper could not be extracted. Check free disk space and write access to the temporary folder, then retry."
LangString UninstallHelperLaunchFailed ${LANG_SIMPCHINESE} "卸载辅助程序未能启动。请检查安全软件是否拦截了卸载，再用安装包重试。"
LangString UninstallHelperLaunchFailed ${LANG_ENGLISH} "The uninstall helper could not start. Check whether security software blocked it and retry with the setup package."
LangString UninstallProcessOwnerUnknown ${LANG_SIMPCHINESE} "无法确认运行中主程序的所属账户，已停止卸载。请让安装所属账户退出 LL Remote 后重试；跨账户卸载请使用管理员权限。"
LangString UninstallProcessOwnerUnknown ${LANG_ENGLISH} "The running main application account could not be verified. Have the owner exit LL Remote and retry. Use administrator privileges for cross-account uninstall."
LangString UninstallProcessAccessDenied ${LANG_SIMPCHINESE} "Windows 拒绝访问运行中的主程序，无法关闭它。请先在安装所属账户中退出 LL Remote 后重试；跨账户需管理员权限，仍失败时请检查进程访问策略。"
LangString UninstallProcessAccessDenied ${LANG_ENGLISH} "Windows denied access to the running main application. Exit LL Remote in the owner account and retry. Cross-account uninstall needs administrator privileges; check process access policy if it still fails."
LangString UninstallProcessQueryFailed ${LANG_SIMPCHINESE} "无法读取运行中主程序的实际路径，已停止卸载以避免关闭其他程序。请退出安装所属账户中的 LL Remote 后重试。"
LangString UninstallProcessQueryFailed ${LANG_ENGLISH} "The running main application path could not be read. Uninstall stopped to avoid closing another program. Exit LL Remote in the owner account and retry."
LangString UninstallProcessStopTimeout ${LANG_SIMPCHINESE} "主程序收到关闭操作后仍未及时退出。请在安装所属账户中退出 LL Remote；若无响应，请结束该实例或重启 Windows 后重试。"
LangString UninstallProcessStopTimeout ${LANG_ENGLISH} "The main application did not exit in time. Exit LL Remote in the owner account. End that instance or restart Windows if unresponsive, then retry."
LangString UninstallOwnerTaskTimeout ${LANG_SIMPCHINESE} "目标账户中的清理辅助进程运行超时。请让该账户保持登录并退出 LL Remote 后重试；仍失败时请重启 Windows 或联系管理员检查计划任务。"
LangString UninstallOwnerTaskTimeout ${LANG_ENGLISH} "The target account cleanup helper timed out. Keep that account signed in, exit LL Remote and retry. Restart Windows or ask an administrator to check Task Scheduler if it still fails."
LangString UninstallInvalidConfiguration ${LANG_SIMPCHINESE} "卸载所需的安装配置或调用参数无效。请保留原安装目录，使用相同或更新版本修复安装后，再从系统应用列表卸载。"
LangString UninstallInvalidConfiguration ${LANG_ENGLISH} "Required uninstall configuration or arguments are invalid. Keep the original installation directory, repair with the same or a newer version, then uninstall from Windows Apps settings."
LangString UninstallAdminRequired ${LANG_SIMPCHINESE} "此卸载操作尚未获得所需的管理员权限。请由管理员运行卸载程序，或联系管理员协助。"
LangString UninstallAdminRequired ${LANG_ENGLISH} "This uninstall operation has not obtained the required administrator privileges. Run uninstall as administrator or contact an administrator for help."
LangString UninstallServiceStopFailed ${LANG_SIMPCHINESE} "关联的系统共享服务未能停止。请由管理员在本机共享设置或 Windows 服务中停止它后重试；无响应时可重启 Windows。"
LangString UninstallServiceStopFailed ${LANG_ENGLISH} "The associated sharing service could not be stopped. Have an administrator stop it in Local Sharing settings or Windows Services, then retry. Restart Windows if unresponsive."
LangString UninstallServiceCredentialFailed ${LANG_SIMPCHINESE} "系统共享服务的访问密码未能清理。请由管理员修复共享服务及其凭据存储后重试，或联系管理员处理服务卸载。"
LangString UninstallServiceCredentialFailed ${LANG_ENGLISH} "The sharing service access password could not be cleared. Have an administrator repair the service and credential store, then retry or ask the administrator to remove the service."
LangString UninstallReportFailed ${LANG_SIMPCHINESE} "无法写入卸载预检结果，尚未开始清理。请检查 Windows 临时目录的写入权限和可用空间，然后重新运行卸载。"
LangString UninstallReportFailed ${LANG_ENGLISH} "The uninstall inspection result could not be written. Cleanup has not started. Check Windows temporary-directory write permissions and free space, then retry."
LangString UninstallUnknownFailure ${LANG_SIMPCHINESE} "卸载辅助程序返回未识别的错误，卸载未完成。请保留原目录，使用相同或更新版本修复后重试；仍失败时请提供下方错误码和安装目录。"
LangString UninstallUnknownFailure ${LANG_ENGLISH} "The uninstall helper returned an unrecognized error. Uninstall is incomplete. Keep the directory, repair with the same or a newer version and retry. Report the error code and install directory below if it still fails."
LangString UninstallFailureContext ${LANG_SIMPCHINESE} "卸载未完成。$\r$\n安装目录：$INSTDIR$\r$\n错误码：$R0"
LangString UninstallFailureContext ${LANG_ENGLISH} "Uninstall is incomplete.$\r$\nInstall directory: $INSTDIR$\r$\nError code: $R0"
LangString UninstallWarningTitle ${LANG_SIMPCHINESE} "LL Remote 已卸载，部分数据或安装痕迹未能清理。"
LangString UninstallWarningTitle ${LANG_ENGLISH} "LL Remote was uninstalled. Some data or installation traces remain."
LangString UninstallCleanupLogTitle ${LANG_SIMPCHINESE} "LL Remote 卸载清理记录"
LangString UninstallCleanupLogTitle ${LANG_ENGLISH} "LL Remote uninstall cleanup report"
LangString UninstallRebootTitle ${LANG_SIMPCHINESE} "需要重启 Windows 才能完成卸载；部分程序文件已安排重启删除。请先重启，再重新安装。"
LangString UninstallRebootTitle ${LANG_ENGLISH} "Restart Windows to finish uninstalling. Some program files are scheduled for removal at restart. Restart before reinstalling."
LangString UninstallWarningsLogLocation ${LANG_SIMPCHINESE} "完整残留清单：$UninstallWarningsLog"
LangString UninstallWarningsLogLocation ${LANG_ENGLISH} "Full residue report: $UninstallWarningsLog"
LangString UninstallWarningsLogUnavailable ${LANG_SIMPCHINESE} "完整清单未能保存，请记录上述残留位置。"
LangString UninstallWarningsLogUnavailable ${LANG_ENGLISH} "The complete report could not be saved. Please record the residue locations above."
LangString UninstallWarnCredentialsOffline ${LANG_SIMPCHINESE} "保存的密码仍保留：目标账户未登录或会话不可用。请登录该账户后在 Windows 凭据管理器中清理。"
LangString UninstallWarnCredentialsOffline ${LANG_ENGLISH} "Saved passwords remain because the owner account was unavailable. Sign in to that account and clean its Windows Credential Manager."
LangString UninstallWarnCredentials ${LANG_SIMPCHINESE} "部分保存的密码未清理。请登录安装所属账户，在 Windows 凭据管理器中检查并清理。"
LangString UninstallWarnCredentials ${LANG_ENGLISH} "Some saved passwords remain. Sign in to the owner account and check its Windows Credential Manager."
LangString UninstallWarnServiceCredentials ${LANG_SIMPCHINESE} "服务已移除，但未能确认服务密码清理完成。请联系管理员清理服务凭据。"
LangString UninstallWarnServiceCredentials ${LANG_ENGLISH} "The service was removed, but clearing its password could not be confirmed. Ask an administrator to clean the service credentials."
LangString UninstallWarnDataInUse ${LANG_SIMPCHINESE} "以下数据仍被占用或锁定。退出占用程序后可手动删除此位置。"
LangString UninstallWarnDataInUse ${LANG_ENGLISH} "The following data is still in use or locked. Exit the program using it, then remove this location manually."
LangString UninstallWarnDataAccess ${LANG_SIMPCHINESE} "以下数据因删除权限不足而保留。请检查目录权限，或由管理员协助删除。"
LangString UninstallWarnDataAccess ${LANG_ENGLISH} "The following data remains because deletion was denied. Check directory permissions or ask an administrator to remove it."
LangString UninstallWarnDataPath ${LANG_SIMPCHINESE} "以下数据路径存在链接或重定向，已保留。请先确认实际指向，再自行处理。"
LangString UninstallWarnDataPath ${LANG_ENGLISH} "The following data path is linked or redirected and was kept. Verify its actual destination before removing it."
LangString UninstallWarnShortcut ${LANG_SIMPCHINESE} "以下位置的 LL Remote 快捷方式可能残留。请只检查并删除对应快捷方式，不要删除整个目录。"
LangString UninstallWarnShortcut ${LANG_ENGLISH} "LL Remote shortcuts may remain at this location. Remove only the corresponding shortcuts, not the entire directory."
LangString UninstallWarnProfile ${LANG_SIMPCHINESE} "用户配置单元的临时挂载未能释放。请管理员检查或重启 Windows；不要删除用户配置文件或整个账户注册表项。"
LangString UninstallWarnProfile ${LANG_ENGLISH} "A temporary user-profile registry mount could not be released. Ask an administrator to check it, or restart Windows. Do not delete the user profile or its entire registry hive."
LangString UninstallWarnUnownedServiceData ${LANG_SIMPCHINESE} "服务已不存在，但以下历史服务数据的安装归属无法确认，已保留。请管理员核对后处理。"
LangString UninstallWarnUnownedServiceData ${LANG_ENGLISH} "The service no longer exists, but ownership of historical service data could not be verified. It was kept for an administrator to review."
LangString UninstallWarnProtectedData ${LANG_SIMPCHINESE} "服务已不存在，以下系统目录中的残留需要管理员权限清理。请联系管理员协助。"
LangString UninstallWarnProtectedData ${LANG_ENGLISH} "The service no longer exists. Cleaning the remaining system-directory data requires administrator privileges. Contact an administrator for help."
LangString UninstallWarnRegistry ${LANG_SIMPCHINESE} "以下失效安装或登录记录未能清理，系统中可能仍显示残留条目。请由管理员协助清理。"
LangString UninstallWarnRegistry ${LANG_ENGLISH} "The following stale installation or login record remains. Windows may still show a leftover entry. Ask an administrator to clean it."
LangString UninstallWarnReport ${LANG_SIMPCHINESE} "部分清理结果未能完整记录。请检查目标账户的应用数据和保存密码是否仍有残留。"
LangString UninstallWarnReport ${LANG_ENGLISH} "Some cleanup results could not be fully recorded. Check the owner account for remaining application data and saved passwords."
LangString UninstallWarnOther ${LANG_SIMPCHINESE} "以下可选数据或安装痕迹未能清理。请确认后手动删除，需要时联系管理员。"
LangString UninstallWarnOther ${LANG_ENGLISH} "The following optional data or installation trace remains. Verify it before removing it manually, or ask an administrator for help."
LangString UninstallLaunchFailed ${LANG_SIMPCHINESE} "无法启动卸载程序（系统错误：$5）。请重新运行安装包，再选择卸载。"
LangString UninstallLaunchFailed ${LANG_ENGLISH} "Unable to start the uninstaller (system error: $5). Run the setup package and select Uninstall again."
LangString UninstallReturnedError ${LANG_SIMPCHINESE} "卸载未完成（退出码：$0）。请重新运行安装包，再选择卸载。"
LangString UninstallReturnedError ${LANG_ENGLISH} "Uninstall did not complete (exit code: $0). Run the setup package and select Uninstall again."
LangString UninstallMainStillExists ${LANG_SIMPCHINESE} "卸载程序已退出，但主程序仍然存在：$\r$\n$ExistingInstallDir\${MAINBINARYNAME}.exe"
LangString UninstallMainStillExists ${LANG_ENGLISH} "The uninstaller exited, but the main application still exists:$\r$\n$ExistingInstallDir\${MAINBINARYNAME}.exe"

LangString UpgradeDescription ${LANG_SIMPCHINESE} "已安装 $ExistingVersion，将安装 ${VERSION}。沿用原目录及安装范围，保留数据、密码、共享配置和自动启动设置。"
LangString UpgradeDescription ${LANG_ENGLISH} "Installed: $ExistingVersion. Package: ${VERSION}. Keep the installation directory, scope, data, passwords, sharing settings and startup preferences."
LangString UpgradeAction ${LANG_SIMPCHINESE} "升级并保留配置（推荐）"
LangString UpgradeAction ${LANG_ENGLISH} "Upgrade and keep settings (recommended)"
LangString RepairAction ${LANG_SIMPCHINESE} "重新安装 / 修复（保留配置）"
LangString RepairAction ${LANG_ENGLISH} "Reinstall / repair (keep settings)"
LangString CancelInstallation ${LANG_SIMPCHINESE} "取消安装"
LangString CancelInstallation ${LANG_ENGLISH} "Cancel installation"
LangString DowngradeBlocked ${LANG_SIMPCHINESE} "已安装 $ExistingVersion，高于安装包版本 ${VERSION}。为保护数据，已阻止降级。请使用相同或更新版本。"
LangString DowngradeBlocked ${LANG_ENGLISH} "Installed version $ExistingVersion is newer than ${VERSION}. Downgrade is blocked to protect your data. Use the same or a newer version."
LangString InstallationRecordInvalid ${LANG_SIMPCHINESE} "已有安装记录不完整，无法安全判断版本或原安装目录。安装已停止，请先修复安装记录或备份后手动迁移。"
LangString InstallationRecordInvalid ${LANG_ENGLISH} "The installation record is incomplete. Installation stopped because the previous version or directory cannot be determined safely. Repair the record or back up and migrate manually."
LangString InstallInterruptsSharing ${LANG_SIMPCHINESE} "安装将关闭 LL Remote，停止本机共享并断开当前远程连接。配置、密码和自动启动设置会保留；安装后需手动重新开始共享。是否继续？"
LangString InstallInterruptsSharing ${LANG_ENGLISH} "Installation will close LL Remote, stop sharing and disconnect current remote sessions. Settings, passwords and startup preferences are kept. Start sharing again manually after installation. Continue?"
LangString InstallPreparationFailed ${LANG_SIMPCHINESE} "无法安全停止共享或完成管理员授权，尚未覆盖应用文件。请关闭共享并检查权限后重试。"
LangString InstallPreparationFailed ${LANG_ENGLISH} "Unable to stop sharing safely or obtain administrator approval. Application files have not been replaced. Stop sharing, check permissions and retry."

LangString ScopeTitle ${LANG_SIMPCHINESE} "选择安装范围"
LangString ScopeTitle ${LANG_ENGLISH} "Choose who can use LL Remote"
LangString ScopeCurrent ${LANG_SIMPCHINESE} "仅为我安装（无需管理员权限）"
LangString ScopeCurrent ${LANG_ENGLISH} "Install only for me (no administrator access)"
LangString ScopeAll ${LANG_SIMPCHINESE} "为所有人安装（需要管理员授权）"
LangString ScopeAll ${LANG_ENGLISH} "Install for all users (administrator approval required)"
LangString ScopeAdminOnly ${LANG_SIMPCHINESE} "当前账户不是管理员，无法选择为所有人安装。"
LangString ScopeAdminOnly ${LANG_ENGLISH} "This account is not an administrator, so all-users installation is unavailable."
LangString ScopeConflictCurrent ${LANG_SIMPCHINESE} "已有仅为当前用户安装的版本。升级请沿用原范围。切换范围需先备份并手动迁移；普通卸载保留个人配置，但会删除共享服务配置和服务密码，迁移后须重新配置。"
LangString ScopeConflictCurrent ${LANG_ENGLISH} "A current-user installation exists. Keep its scope to upgrade. To change scope, back up and migrate manually. Normal uninstall keeps personal data but removes sharing-service settings and passwords; configure the service again after migration."
LangString ScopeConflictMachine ${LANG_SIMPCHINESE} "已有为所有人安装的版本。升级请沿用原范围。切换范围需先备份并由管理员手动迁移；普通卸载保留个人配置，但会删除共享服务配置和服务密码，迁移后须重新配置。"
LangString ScopeConflictMachine ${LANG_ENGLISH} "An all-users installation exists. Keep its scope to upgrade. To change scope, back up and have an administrator migrate manually. Normal uninstall keeps personal data but removes sharing-service settings and passwords; configure the service again after migration."
LangString ScopeElevationFailed ${LANG_SIMPCHINESE} "管理员授权未完成，安装尚未开始。"
LangString ScopeElevationFailed ${LANG_ENGLISH} "Administrator approval was not completed; installation has not started."
LangString ScopeMachineDirectory ${LANG_SIMPCHINESE} "为所有人安装时，请选择 Program Files 下的目录。"
LangString ScopeMachineDirectory ${LANG_ENGLISH} "For all-users installation, choose a directory under Program Files."

Function SelectCurrentScope
  StrCpy $MultiUser.InstallMode "CurrentUser"
  SetShellVarContext current
  StrCpy $INSTDIR "$LOCALAPPDATA\LLRemote\App"
  Call RestorePreviousInstallLocation
FunctionEnd

Function SelectMachineScope
  StrCpy $MultiUser.InstallMode "AllUsers"
  SetShellVarContext all
  ${If} ${RunningX64}
    !if "${ARCH}" == "x64"
      StrCpy $INSTDIR "$PROGRAMFILES64\LLRemote\App"
    !else if "${ARCH}" == "arm64"
      StrCpy $INSTDIR "$PROGRAMFILES64\LLRemote\App"
    !else
      StrCpy $INSTDIR "$PROGRAMFILES\LLRemote\App"
    !endif
  ${Else}
    StrCpy $INSTDIR "$PROGRAMFILES\LLRemote\App"
  ${EndIf}
  Call RestorePreviousInstallLocation
FunctionEnd

Function ScopePage
  ${If} $ScopeResumed == 1
  ${OrIf} $PassiveMode == 1
  ${OrIf} ${Silent}
    Abort
  ${EndIf}
  !insertmacro MUI_HEADER_TEXT "$(ScopeTitle)" ""
  nsDialogs::Create 1018
  Pop $0
  ${If} $0 == error
    Abort
  ${EndIf}
  ${NSD_CreateRadioButton} 0 16u 100% 16u "$(ScopeCurrent)"
  Pop $ScopeCurrentRadio
  ${NSD_CreateRadioButton} 0 45u 100% 16u "$(ScopeAll)"
  Pop $ScopeAllRadio
  ${If} $ScopeAccountType != "Admin"
    EnableWindow $ScopeAllRadio 0
    ${NSD_CreateLabel} 0 80u 100% 32u "$(ScopeAdminOnly)"
    Pop $0
  ${EndIf}
  ${If} $ScopeExistingMachine != ""
  ${AndIf} $ScopeAccountType == "Admin"
    ${NSD_Check} $ScopeAllRadio
  ${Else}
    ${NSD_Check} $ScopeCurrentRadio
  ${EndIf}
  nsDialogs::Show
FunctionEnd

Function ScopeLeave
  ${NSD_GetState} $ScopeAllRadio $0
  ${If} $0 == ${BST_CHECKED}
    ${If} $ScopeAccountType != "Admin"
      Abort
    ${EndIf}
    ${If} $ScopeExistingCurrent != ""
      MessageBox MB_ICONEXCLAMATION|MB_OK "$(ScopeConflictCurrent)"
      Abort
    ${EndIf}
    UserInfo::GetAccountType
    Pop $0
    ${If} $0 != "Admin"
      ClearErrors
      ExecShell "runas" "$EXEPATH" "/ALLUSERS"
      ${If} ${Errors}
        MessageBox MB_ICONEXCLAMATION|MB_OK "$(ScopeElevationFailed)"
        Abort
      ${EndIf}
      Quit
    ${EndIf}
    Call SelectMachineScope
  ${Else}
    ${If} $ScopeExistingMachine != ""
      MessageBox MB_ICONEXCLAMATION|MB_OK "$(ScopeConflictMachine)"
      Abort
    ${EndIf}
    Call SelectCurrentScope
  ${EndIf}
  Call DetectExistingInstallation
FunctionEnd

Function ValidateMachineDirectory
  ${If} $MultiUser.InstallMode != "AllUsers"
    Return
  ${EndIf}
  ${If} ${RunningX64}
    !if "${ARCH}" == "x64"
      StrCpy $0 "$PROGRAMFILES64\"
    !else if "${ARCH}" == "arm64"
      StrCpy $0 "$PROGRAMFILES64\"
    !else
      StrCpy $0 "$PROGRAMFILES\"
    !endif
  ${Else}
    StrCpy $0 "$PROGRAMFILES\"
  ${EndIf}
  ; A trailing separator treats the destination as a directory. Without it,
  ; NSIS tries to resolve the final component as an existing file and rejects
  ; the not-yet-created directory used by a first installation.
  ClearErrors
  GetFullPathName $1 "$INSTDIR\"
  ${If} ${Errors}
  ${OrIf} $1 == $0
    MessageBox MB_ICONEXCLAMATION|MB_OK "$(ScopeMachineDirectory)" /SD IDOK
    Abort
  ${EndIf}
  StrLen $2 $0
  StrCpy $3 $1 $2
  ${If} $3 != $0
    MessageBox MB_ICONEXCLAMATION|MB_OK "$(ScopeMachineDirectory)" /SD IDOK
    Abort
  ${EndIf}
  StrCpy $INSTDIR $1 -1
FunctionEnd

!insertmacro LLREMOTE_INSTALLER_FUNCTIONS

Function LaunchPackagedUninstaller
  ; The installed uninstaller can be missing or damaged. Use the one carried
  ; by this package and target the registered directory, before any relocation.
  Call LockInstaller
  InitPluginsDir
  StrCpy $R1 "$PLUGINSDIR\uninstall.exe"
  ClearErrors
  WriteUninstaller "$R1"
  ${If} ${Errors}
    System::Call 'kernel32::GetLastError() i.r5'
    MessageBox MB_ICONEXCLAMATION|MB_OK "$(UninstallLaunchFailed)" /SD IDOK
    System::Call 'kernel32::CloseHandle(p $InstallMutex)'
    Abort
  ${EndIf}
  HideWindow
  ClearErrors
  ExecWait '"$R1" _?=$ExistingInstallDir' $0
  ${If} ${Errors}
    System::Call 'kernel32::GetLastError() i.r5'
    System::Call 'kernel32::CloseHandle(p $InstallMutex)'
    BringToFront
    MessageBox MB_ICONEXCLAMATION|MB_OK "$(UninstallLaunchFailed)" /SD IDOK
    Abort
  ${EndIf}
  System::Call 'kernel32::CloseHandle(p $InstallMutex)'
  BringToFront
  ${If} $0 <> 0
  ${AndIf} $0 <> 3010
    ${If} $0 <> 1
      MessageBox MB_ICONEXCLAMATION|MB_OK "$(UninstallReturnedError)" /SD IDOK
    ${EndIf}
    Abort
  ${EndIf}
  ${If} ${FileExists} "$ExistingInstallDir\${MAINBINARYNAME}.exe"
  ${AndIf} $0 <> 3010
    MessageBox MB_ICONEXCLAMATION|MB_OK "$(UninstallMainStillExists)" /SD IDOK
    Abort
  ${EndIf}
  SetErrorLevel $0
FunctionEnd

Function .onInit
  Call PreflightCompatibility
  Call ProbeInstallerPayload
  ${GetOptions} $CMDLINE "/P" $PassiveMode
  ${IfNot} ${Errors}
    StrCpy $PassiveMode 1
  ${EndIf}

  ${GetOptions} $CMDLINE "/NS" $NoShortcutMode
  ${IfNot} ${Errors}
    StrCpy $NoShortcutMode 1
  ${EndIf}
  ${GetOptions} $CMDLINE "/UPDATE" $UpdateMode
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}

  !if "${DISPLAYLANGUAGESELECTOR}" == "true"
    !insertmacro MUI_LANGDLL_DISPLAY
  !endif

  !insertmacro SetContext
  UserInfo::GetOriginalAccountType
  Pop $ScopeAccountType
  ReadRegStr $ScopeExistingCurrent HKCU "${UNINSTKEY}" "UninstallString"
  ReadRegStr $ScopeExistingMachine HKLM "${UNINSTKEY}" "UninstallString"
  ${GetOptions} $CMDLINE "/ALLUSERS" $0
  ${IfNot} ${Errors}
    UserInfo::GetAccountType
    Pop $0
    ${If} $0 != "Admin"
      Abort "$(ScopeElevationFailed)"
    ${EndIf}
    ${If} $ScopeExistingCurrent != ""
      Abort "$(ScopeConflictCurrent)"
    ${EndIf}
    StrCpy $ScopeResumed 1
    Call SelectMachineScope
  ${Else}
    ; Unattended upgrades must follow the existing scope, too. Interactive
    ; installs still choose their scope before asking for administrator access.
    StrCpy $0 0
    ${If} $UpdateMode = 1
    ${OrIf} $PassiveMode = 1
    ${OrIf} ${Silent}
      ${If} $ScopeExistingMachine != ""
        StrCpy $0 1
      ${EndIf}
    ${EndIf}
    ${If} $0 = 1
      ${If} $ScopeAccountType != "Admin"
        Abort "$(ScopeAdminOnly)"
      ${EndIf}
      UserInfo::GetAccountType
      Pop $0
      ${If} $0 != "Admin"
        ${GetParameters} $0
        ClearErrors
        ExecShell "runas" "$EXEPATH" "$0 /ALLUSERS"
        ${If} ${Errors}
          Abort "$(ScopeElevationFailed)"
        ${EndIf}
        Quit
      ${EndIf}
      Call SelectMachineScope
    ${Else}
      Call SelectCurrentScope
    ${EndIf}
  ${EndIf}
FunctionEnd

Section EarlyChecks
  Call LockInstaller
  Call ValidateScopeConflict
  Call DetectExistingInstallation
  Call ValidateMachineDirectory
  Call CheckInstallerDestination
  Call ReadInstallerState

SectionEnd
Section WebView2
  ; Check if Webview2 is already installed and skip this section
  ${If} ${RunningX64}
    ReadRegStr $4 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\${WEBVIEW2APPGUID}" "pv"
  ${Else}
    ReadRegStr $4 HKLM "SOFTWARE\Microsoft\EdgeUpdate\Clients\${WEBVIEW2APPGUID}" "pv"
  ${EndIf}
  ${If} $4 == ""
    ${If} $MultiUser.InstallMode == "CurrentUser"
      ReadRegStr $4 HKCU "SOFTWARE\Microsoft\EdgeUpdate\Clients\${WEBVIEW2APPGUID}" "pv"
    ${EndIf}
  ${EndIf}
  ${If} $4 == ""
    ; Repair a missing runtime in update mode as well.
    Delete "$PLUGINSDIR\MicrosoftEdgeWebView2RuntimeInstaller.exe"
    File "/oname=$PLUGINSDIR\MicrosoftEdgeWebView2RuntimeInstaller.exe" "${WEBVIEW2INSTALLERPATH}"
    DetailPrint "$(installingWebview2)"
    ClearErrors
    ExecWait '"$PLUGINSDIR\MicrosoftEdgeWebView2RuntimeInstaller.exe" /silent /install' $1
    ${IfNot} ${Errors}
    ${AndIf} $1 = 0
      DetailPrint "$(webview2InstallSuccess)"
    ${Else}
      DetailPrint "$(webview2InstallError)"
      StrCpy $InstallStage "webview2_install"
      StrCpy $HelperExit $1
      StrCpy $HelperDetail "$(webview2AbortError)"
      Call FailInstaller
    ${EndIf}
  ${Else}
    !if "${MINIMUMWEBVIEW2VERSION}" != ""
      ${VersionCompare} "${MINIMUMWEBVIEW2VERSION}" "$4" $R0
      ${If} $R0 = 1
        update_webview:
          DetailPrint "$(installingWebview2)"
          ${If} ${RunningX64}
            ReadRegStr $R1 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate" "path"
          ${Else}
            ReadRegStr $R1 HKLM "SOFTWARE\Microsoft\EdgeUpdate" "path"
          ${EndIf}
          ${If} $R1 == ""
            ReadRegStr $R1 HKCU "SOFTWARE\Microsoft\EdgeUpdate" "path"
          ${EndIf}
          ${If} $R1 != ""
            ; Chromium updater docs: https://source.chromium.org/chromium/chromium/src/+/main:docs/updater/user_manual.md
            ; Modified from "HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Microsoft EdgeWebView\ModifyPath"
            ClearErrors
            ExecWait `"$R1" /install appguid=${WEBVIEW2APPGUID}&needsadmin=true` $1
            ${IfNot} ${Errors}
            ${AndIf} $1 = 0
              DetailPrint "$(webview2InstallSuccess)"
            ${Else}
              StrCpy $InstallStage "webview2_update"
              StrCpy $HelperExit $1
              StrCpy $HelperDetail "$(webview2InstallError)"
              Call FailInstaller
            ${EndIf}
          ${EndIf}
      ${EndIf}
    !endif
  ${EndIf}
  ; Registry entries are hints. Verify that the application architecture can
  ; actually locate the runtime before stopping any existing sharing.
  StrCpy $InstallStage "verify_webview2"
  StrCpy $HelperArgs "--installer-verify"
  StrCpy $HelperTimeout 15000
  Call RunInstallerHelper
  ${If} $HelperExit != 0
    Call FailInstaller
  ${EndIf}
SectionEnd
Section Install
  Call ValidateScopeConflict
  Call DetectExistingInstallation
  Call ValidateMachineDirectory
  ${If} $ExistingInstall = 1
    StrCpy $InstallStage "repair"
    StrCpy $HelperNativeError 0
    StrCpy $HelperExit 0
    !insertmacro LogMissingInstallerFile "${MAINBINARYNAME}.exe"
    !insertmacro LogMissingInstallerFile "uninstall.exe"
    {{#each resources}}
      !insertmacro LogMissingInstallerFile "{{this.[1]}}"
    {{/each}}
    {{#each binaries}}
      !insertmacro LogMissingInstallerFile "{{this}}"
    {{/each}}
  ${EndIf}
  ; Stage all required files before interrupting the existing application or
  ; sharing. Every resource creates its own parent and fails without retry UI.
  StrCpy $InstallStage "extract_payload"
  {{#each resources}}
    !insertmacro ExtractInstallerFile "{{no-escape @key}}" "{{this.[1]}}"
  {{/each}}
  {{#each binaries}}
    !insertmacro ExtractInstallerFile "{{no-escape @key}}" "{{this}}"
  {{/each}}
  StrCpy $HelperDetail "write_payload_uninstaller: $PLUGINSDIR\payload\uninstall.exe"
  ClearErrors
  WriteUninstaller "$PLUGINSDIR\payload\uninstall.exe"
  Call CheckPayloadExtraction

  ; Recheck after potentially long prerequisite installation.
  StrCpy $HelperPath "$PLUGINSDIR\payload\${MAINBINARYNAME}.exe"
  Call ReadInstallerState
  StrCpy $InstallHasApp 0
  ${If} $MultiUser.InstallMode == "CurrentUser"
    nsis_tauri_utils::FindProcessCurrentUser "${MAINBINARYNAME}.exe"
  ${Else}
    nsis_tauri_utils::FindProcess "${MAINBINARYNAME}.exe"
  ${EndIf}
  Pop $0
  ${If} $0 = 0
    StrCpy $InstallHasApp 1
  ${EndIf}
  ${If} $InstallHasRuntime = 1
  ${OrIf} $InstallHasApp = 1
    ${If} $PassiveMode <> 1
    ${AndIfNot} ${Silent}
      MessageBox MB_ICONEXCLAMATION|MB_OKCANCEL "$(InstallInterruptsSharing)" IDCANCEL install_cancelled
    ${EndIf}
  ${EndIf}
  ; A fresh installation has no runtime to stop. Existing services are stopped
  ; here, but replaced only after the new application files pass verification.
  ${If} $UpgradeHadService = 1
  ${OrIf} $InstallHasRuntime = 1
    StrCpy $InstallStage "stop_sharing"
    StrCpy $HelperArgs "--prepare-update"
    StrCpy $HelperTimeout 180000
    Call RunInstallerHelper
    ${If} $HelperExit != 0
    ${AndIf} $HelperExit != 10
      Goto install_prepare_failed
    ${EndIf}
  ${EndIf}

  ${If} $InstallHasApp = 1
    ; Consent was collected once above. Limit a user install to that user's GUI.
    ${If} $MultiUser.InstallMode == "CurrentUser"
      nsis_tauri_utils::KillProcessCurrentUser "${MAINBINARYNAME}.exe"
    ${Else}
      nsis_tauri_utils::KillProcess "${MAINBINARYNAME}.exe"
    ${EndIf}
    Pop $0
    ; Allow terminated processes to release image/file handles before copying.
    Sleep 500
    ${If} $0 != 0
    ${AndIf} $0 != 2
      StrCpy $InstallStage "close_application"
      StrCpy $HelperDetail "application_could_not_exit"
      Goto install_prepare_failed
    ${EndIf}
  ${EndIf}

  StrCpy $InstallStage "backup_application"
  Call BackupInstallerPayload
  ${If} ${Errors}
    StrCpy $HelperDetail "backup_failed; application_files_not_replaced"
    Goto install_prepare_failed
  ${EndIf}
  StrCpy $InstallStage "replace_application"
  Call ApplyInstallerPayload
  ${If} ${Errors}
    StrCpy $HelperDetail "file_replacement_failed"
    Goto install_copy_failed
  ${EndIf}
  StrCpy $InstallStage "verify_installed_application"
  StrCpy $HelperPath "$INSTDIR\${MAINBINARYNAME}.exe"
  StrCpy $HelperArgs "--installer-verify"
  StrCpy $HelperTimeout 15000
  Call RunInstallerHelper
  ${If} $HelperExit != 0
    Goto install_copy_failed
  ${EndIf}

  !ifmacrodef NSIS_HOOK_POSTINSTALL
    StrCpy $InstallStage "register_installation_owner"
    ${If} $MultiUser.InstallMode == "AllUsers"
      StrCpy $HelperArgs "--installer-owner-all"
    ${Else}
      StrCpy $HelperArgs "--installer-owner-user"
    ${EndIf}
    Call RunInstallerHelper
    ${If} $HelperExit != 0
      Goto install_post_failed
    ${EndIf}
    !insertmacro NSIS_HOOK_POSTINSTALL
  !endif
  ; Publish installation metadata only after required maintenance succeeds.
  StrCpy $InstallStage "register_installation"
  ClearErrors
  SetOutPath "$INSTDIR"
  ; Create file associations
  {{#each file_associations as |association| ~}}
    {{#each association.ext as |ext| ~}}
       !insertmacro APP_ASSOCIATE "{{ext}}" "{{or association.name ext}}" "{{association-description association.description ext}}" "$INSTDIR\${MAINBINARYNAME}.exe,0" "Open with ${PRODUCTNAME}" "$INSTDIR\${MAINBINARYNAME}.exe $\"%1$\""
    {{/each}}
  {{/each}}
  ; Register deep links
  {{#each deep_link_protocols as |protocol| ~}}
    !insertmacro WriteInstallerReg Str "Software\Classes\\{{protocol}}" "URL Protocol" ""
    !insertmacro WriteInstallerReg Str "Software\Classes\\{{protocol}}" "" "URL:${BUNDLEID} protocol"
    !insertmacro WriteInstallerReg Str "Software\Classes\\{{protocol}}\DefaultIcon" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\",0"
    !insertmacro WriteInstallerReg Str "Software\Classes\\{{protocol}}\shell\open\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
  {{/each}}
  ; The verified payload already includes the newly generated uninstaller.

  ; Save $INSTDIR in registry for future installations
  !insertmacro WriteInstallerReg Str "${MANUPRODUCTKEY}" "" "$INSTDIR"

  !if "${INSTALLMODE}" == "both"
    ; Save install mode to be selected by default for the next installation such as updating
    ; or when uninstalling
    !insertmacro WriteInstallerReg DWORD "${UNINSTKEY}" "$MultiUser.InstallMode" "1"
  !endif
  ; Save current MAINBINARYNAME for future updates
  !insertmacro WriteInstallerReg Str "${UNINSTKEY}" "MainBinaryName" "${MAINBINARYNAME}.exe"
  ; Registry information for add/remove programs
  !insertmacro WriteInstallerReg Str "${UNINSTKEY}" "DisplayName" "${PRODUCTNAME}"
  !insertmacro WriteInstallerReg Str "${UNINSTKEY}" "DisplayIcon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\""
  !insertmacro WriteInstallerReg Str "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
  !insertmacro WriteInstallerReg Str "${UNINSTKEY}" "Publisher" "${MANUFACTURER}"
  !insertmacro WriteInstallerReg Str "${UNINSTKEY}" "InstallLocation" "$\"$INSTDIR$\""
  !insertmacro WriteInstallerReg Str "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  !insertmacro WriteInstallerReg DWORD "${UNINSTKEY}" "NoModify" "1"
  !insertmacro WriteInstallerReg DWORD "${UNINSTKEY}" "NoRepair" "1"
  ${GetSize} "$INSTDIR" "/M=uninstall.exe /S=0K /G=0" $0 $1 $2
  IntOp $0 $0 + ${ESTIMATEDSIZE}
  IntFmt $0 "0x%08X" $0
  !insertmacro WriteInstallerReg DWORD "${UNINSTKEY}" "EstimatedSize" "$0"

  !if "${HOMEPAGE}" != ""
    !insertmacro WriteInstallerReg Str "${UNINSTKEY}" "URLInfoAbout" "${HOMEPAGE}"
    !insertmacro WriteInstallerReg Str "${UNINSTKEY}" "URLUpdateInfo" "${HOMEPAGE}"
    !insertmacro WriteInstallerReg Str "${UNINSTKEY}" "HelpLink" "${HOMEPAGE}"
  !endif
  ; Create start menu shortcut
  !insertmacro MUI_STARTMENU_WRITE_BEGIN Application
    ClearErrors
    Call CreateOrUpdateStartMenuShortcut
    ${If} ${Errors}
      StrCpy $HelperDetail "start_menu_shortcut_failed"
      Goto install_post_failed
    ${EndIf}
  !insertmacro MUI_STARTMENU_WRITE_END

  ; The finish page owns the interactive choice. Do not create a new desktop
  ; shortcut during a silent update if the user previously opted out.
  ${If} $PassiveMode = 1
  ${OrIf} ${Silent}
    ${If} $UpdateMode != 1
      ClearErrors
      Call CreateOrUpdateDesktopShortcut
      ${If} ${Errors}
        StrCpy $HelperDetail "desktop_shortcut_failed"
        Goto install_post_failed
      ${EndIf}
    ${EndIf}
  ${EndIf}

  DeleteRegValue SHCTX "${UNINSTKEY}" "InstallState"
  Delete "$INSTDIR\.llremote-install-recovery.ini"
  RMDir /r "$InstallBackup"
  StrCpy $HelperDetail "complete"
  Call LogInstallerStage
  ; Auto close this page for passive mode
  ${If} $PassiveMode = 1
    SetAutoClose true
  ${EndIf}
  Goto install_done
  install_copy_failed:
    Call LogInstallerStage
    ${If} $InstallUncertain != 1
      Call RestoreInstallerPayload
      ${IfNot} ${Errors}
        StrCpy $HelperDetail "$(InstallerRestored)"
        StrCpy $InstallRecoveryMessage "$(InstallerRestored)"
        Delete "$INSTDIR\.llremote-install-recovery.ini"
        RMDir /r "$InstallBackup"
        Goto install_prepare_failed
      ${EndIf}
    ${EndIf}
    StrCpy $HelperDetail "rollback_incomplete; $(InstallerPartial)"
    Goto install_post_failed
  install_post_failed:
    ; Retain the backup and recovery marker. Never roll back a GUI payload
    ; underneath a possibly partially updated service or a running mutator.
    WriteINIStr "$INSTDIR\.llremote-install-recovery.ini" "recovery" "stage" "$InstallStage"
    ; Keep a discoverable uninstall/repair entry even for a fresh custom path.
    WriteRegStr SHCTX "${MANUPRODUCTKEY}" "" "$INSTDIR"
    WriteRegStr SHCTX "${UNINSTKEY}" "DisplayName" "${PRODUCTNAME}"
    WriteRegStr SHCTX "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
    WriteRegStr SHCTX "${UNINSTKEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr SHCTX "${UNINSTKEY}" "UninstallString" '$\"$INSTDIR\uninstall.exe$\"'
    WriteRegStr SHCTX "${UNINSTKEY}" "InstallState" "repair_required"
    StrCpy $HelperDetail "$HelperDetail; $(InstallerPartial)"
    StrCpy $InstallRecoveryMessage "$(InstallerPartial)"
  install_prepare_failed:
    Call FailInstaller
  install_cancelled:
    StrCpy $InstallStage "cancelled"
    StrCpy $HelperDetail "user_cancelled_before_sharing_was_stopped"
    Call LogInstallerStage
    SetErrorLevel 1
    Quit
  install_done:
SectionEnd
Function CheckInstallerDestination
  StrCpy $InstallStage "destination_check"
  ; Validate before dependencies or runtime changes. The probe is the only
  ; destination write and is removed immediately; it cannot overwrite a file.
  StrCpy $0 "$INSTDIR\.llremote-rollback"
  Call ValidateInstallerPath
  ${If} ${Errors}
    StrCpy $HelperDetail "destination_contains_reparse_point_or_is_inaccessible"
    Call FailInstaller
  ${EndIf}
  ${If} ${FileExists} "$INSTDIR\.llremote-install-recovery.ini"
    ; A previous attempt may already have changed the service. Preserve the
    ; original snapshot for explicit repair, never silently roll it back.
    StrCpy $InstallUncertain 1
  ${EndIf}
  ClearErrors
  CreateDirectory "$INSTDIR"
  GetTempFileName $0 "$INSTDIR"
  ${If} ${Errors}
    StrCpy $HelperDetail "destination_not_writable_or_disk_full"
    Call FailInstaller
  ${EndIf}
  Delete "$0"
  ; Reserve room for extraction, the new files and a bounded file snapshot.
  ; Use 64-bit arithmetic even though NSIS itself is a 32-bit executable.
  System::Int64Op ${ESTIMATEDSIZE} * 3072
  Pop $6
  System::Int64Op $6 + 8388608
  Pop $6
  System::Call 'kernel32::GetDiskFreeSpaceExW(w "$INSTDIR", *l.r0, p0, p0) i.r1'
  System::Int64Op $0 > $6
  Pop $2
  ${If} $1 = 0
  ${OrIf} $2 = 0
    StrCpy $HelperDetail "insufficient_destination_space_or_space_query_failed"
    Call FailInstaller
  ${EndIf}
  System::Call 'kernel32::GetDiskFreeSpaceExW(w "$TEMP", *l.r0, p0, p0) i.r1'
  System::Int64Op $0 > $6
  Pop $2
  ${If} $1 = 0
  ${OrIf} $2 = 0
    StrCpy $HelperDetail "insufficient_temporary_space_or_space_query_failed"
    Call FailInstaller
  ${EndIf}
FunctionEnd

Function BackupInstallerPayload
  StrCpy $InstallBackup "$INSTDIR\.llremote-rollback"
  ClearErrors
  CreateDirectory "$InstallBackup"
  WriteINIStr "$INSTDIR\.llremote-install-recovery.ini" "recovery" "stage" "backup"
  WriteINIStr "$INSTDIR\.llremote-install-recovery.ini" "recovery" "version" "${VERSION}"
  ${If} ${Errors}
    Return
  ${EndIf}
  !insertmacro BackupInstallerFile "${MAINBINARYNAME}.exe"
  !insertmacro BackupInstallerFile "uninstall.exe"
  {{#each resources}}
    !insertmacro BackupInstallerFile "{{this.[1]}}"
  {{/each}}
  {{#each binaries}}
    !insertmacro BackupInstallerFile "{{this}}"
  {{/each}}
  ClearErrors
FunctionEnd

Function ApplyInstallerPayload
  !insertmacro ApplyInstallerFile "${MAINBINARYNAME}.exe"
  !insertmacro ApplyInstallerFile "uninstall.exe"
  {{#each resources}}
    !insertmacro ApplyInstallerFile "{{this.[1]}}"
  {{/each}}
  {{#each binaries}}
    !insertmacro ApplyInstallerFile "{{this}}"
  {{/each}}
  ClearErrors
FunctionEnd

Function RestoreInstallerPayload
  !insertmacro RestoreInstallerFile "${MAINBINARYNAME}.exe"
  !insertmacro RestoreInstallerFile "uninstall.exe"
  {{#each resources}}
    !insertmacro RestoreInstallerFile "{{this.[1]}}"
  {{/each}}
  {{#each binaries}}
    !insertmacro RestoreInstallerFile "{{this}}"
  {{/each}}
  ClearErrors
FunctionEnd

Function .onInstSuccess
  ; Check for `/R` flag only in silent and passive installers because
  ; GUI installer has a toggle for the user to (re)start the app
  ${If} $PassiveMode = 1
  ${OrIf} ${Silent}
    ${GetOptions} $CMDLINE "/R" $R0
    ${IfNot} ${Errors}
      ${GetOptions} $CMDLINE "/ARGS" $R0
      nsis_tauri_utils::RunAsUser "$INSTDIR\${MAINBINARYNAME}.exe" "$R0"
    ${EndIf}
  ${EndIf}
FunctionEnd

Function un.onInit
  !insertmacro SetContext
  InitPluginsDir
  ${GetOptions} $CMDLINE "/UPDATE" $UpdateMode
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}
  StrCpy $0 "$INSTDIR"
  Call un.ValidateInstallPath
  ${If} ${Errors}
    StrCpy $R0 102
    !insertmacro LLREMOTE_UNINSTALL_FAILURE
  ${EndIf}
  CreateDirectory "$TEMP\LLRemote\Uninstaller"
  ClearErrors
  GetTempFileName $UninstallLog "$TEMP\LLRemote\Uninstaller"
  ${If} ${Errors}
    StrCpy $UninstallLog ""
  ${Else}
    FileOpen $0 "$UninstallLog" w
    FileWriteWord $0 65279
    FileClose $0
  ${EndIf}
  ; The uninstaller stays executable even when all installed program files are
  ; gone. The helper validates the target registration independently of this
  ; temporary process's own location, including after an administrator restart.
  StrCpy $UninstallDirectoryArgs ' --uninstall-directory "$INSTDIR"'
  ClearErrors
  SetOutPath "$PLUGINSDIR"
  SetOverwrite try
  File /oname=uninstall-maintenance.exe "${MAINBINARYSRCPATH}"
  SetOverwrite lastused
  ${If} ${Errors}
    StrCpy $R0 122
    !insertmacro LLREMOTE_UNINSTALL_FAILURE
  ${EndIf}
  StrCpy $UninstallHelperPath "$PLUGINSDIR\uninstall-maintenance.exe"
  !insertmacro LLREMOTE_UNINSTALL_LOG "bundled_uninstaller: $INSTDIR"
  ${GetOptions} $CMDLINE "/TARGETUSER=" $UninstallTargetSid
  ${GetOptions} $CMDLINE "/RESULTFILE=" $UninstallResultFile
  ClearErrors
  ${If} $UninstallTargetSid == ""
    ExecWait '"$UninstallHelperPath" --uninstall-inspect "$PLUGINSDIR\uninstall.ini"$UninstallDirectoryArgs' $R0
  ${Else}
    ExecWait '"$UninstallHelperPath" --uninstall-inspect "$PLUGINSDIR\uninstall.ini" $UninstallTargetSid$UninstallDirectoryArgs' $R0
  ${EndIf}
  ${If} ${Errors}
    StrCpy $R0 123
    !insertmacro LLREMOTE_UNINSTALL_FAILURE
  ${ElseIf} $R0 != 0
    !insertmacro LLREMOTE_UNINSTALL_FAILURE
  ${EndIf}
  ReadINIStr $UninstallScope "$PLUGINSDIR\uninstall.ini" "uninstall" "scope"
  ReadINIStr $UninstallTargetSid "$PLUGINSDIR\uninstall.ini" "uninstall" "sid"
  ReadINIStr $UninstallUserLabel "$PLUGINSDIR\uninstall.ini" "uninstall" "user"
  ReadINIStr $0 "$PLUGINSDIR\uninstall.ini" "uninstall" "elevate"
  ReadINIStr $1 "$PLUGINSDIR\uninstall.ini" "uninstall" "service_elevate"
  ; Updates retain the service. A real uninstall must acquire all service
  ; privileges now, before closing the App or starting any cleanup.
  ${If} $UpdateMode <> 1
  ${AndIf} $1 == "1"
    StrCpy $0 1
  ${EndIf}
  ${If} $UninstallTargetSid == ""
    StrCpy $R0 101
    !insertmacro LLREMOTE_UNINSTALL_FAILURE
  ${EndIf}
  ${If} $UninstallScope == "all"
    SetShellVarContext all
  ${Else}
    SetShellVarContext current
  ${EndIf}
  ${If} $0 == "1"
      ${GetParameters} $1
      ClearErrors
      ; NSIS is already executing a temporary uninstaller copy. Preserve the
      ; original installation directory when restarting that copy with UAC.
      ; _?= must be last and its path must not be quoted, even with spaces.
      ExecShellWait "runas" "$EXEPATH" "$1 /TARGETUSER=$UninstallTargetSid /RESULTFILE=$\"$PLUGINSDIR\uninstall-result$\" _?=$INSTDIR"
      ${If} ${Errors}
        ReadINIStr $R0 "$PLUGINSDIR\uninstall.ini" "uninstall" "service_elevate"
        ${If} $R0 == "1"
          MessageBox MB_ICONSTOP|MB_OK "$(UninstallServiceAuthorizationRequired)" /SD IDOK
        ${Else}
          MessageBox MB_ICONSTOP|MB_OK "$(ScopeElevationFailed)" /SD IDOK
        ${EndIf}
        SetErrorLevel 1
        Abort
      ${EndIf}
      ; ExecShellWait does not expose the child's exit code. Require an explicit
      ; completion receipt: absence of the App alone can hide a later failure.
      ClearErrors
      FileOpen $0 "$PLUGINSDIR\uninstall-result" r
      FileRead $0 $1
      FileClose $0
      ${IfNot} ${Errors}
      ${AndIf} $1 == "1"
        SetErrorLevel 0
      ${ElseIf} $1 == "2"
      ${AndIfNot} ${Errors}
        SetErrorLevel 3010
      ${Else}
        SetErrorLevel 1
      ${EndIf}
      Quit
  ${EndIf}

  !insertmacro MUI_UNGETLANGUAGE
  ${If} $UninstallScope == "all"
    StrCpy $UninstallScopeLabel "$(UninstallScopeAll)"
  ${Else}
    StrCpy $UninstallScopeLabel "$(UninstallScopeUser)"
  ${EndIf}
  ReadINIStr $0 "$PLUGINSDIR\uninstall.ini" "uninstall" "service"
  ${If} $0 == "1"
    StrCpy $UninstallServiceLabel "$(UninstallServiceRemove)"
  ${Else}
    StrCpy $UninstallServiceLabel "$(UninstallServiceKeep)"
  ${EndIf}

  ${GetOptions} $CMDLINE "/P" $PassiveMode
  ${IfNot} ${Errors}
    StrCpy $PassiveMode 1
  ${EndIf}

FunctionEnd

Function un.ValidateInstallPath
  !insertmacro LLREMOTE_VALIDATE_INSTALL_PATH
FunctionEnd

Function un.RecordCompletion
  SetErrorLevel 0
  IfRebootFlag 0 +2
    SetErrorLevel 3010
  ${If} $UninstallResultFile == ""
    Return
  ${EndIf}
  ; CREATE_NEW prevents a supplied receipt path from overwriting a file or
  ; following a pre-existing final symlink. Write only a single success byte.
  System::Call 'kernel32::CreateFileW(w "$UninstallResultFile", i 0x40000000, i0, p0, i1, i0x80, p0) p.r0'
  ${If} $0 == -1
    SetErrorLevel 1
    Return
  ${EndIf}
  StrCpy $R9 49
  IfRebootFlag 0 +2
    StrCpy $R9 50
  System::Call '*(i R9)p.r2'
  System::Call 'kernel32::WriteFile(p r0, p r2, i1, *i.r3, p0) i.r1'
  System::Free $2
  System::Call 'kernel32::CloseHandle(p r0)'
  ${If} $1 == 0
  ${OrIf} $3 != 1
    SetErrorLevel 1
  ${EndIf}
FunctionEnd

Function un.onUninstSuccess
  !insertmacro LLREMOTE_UNINSTALL_LOG "removal_complete: warnings=$UninstallWarningCount"
  !insertmacro LLREMOTE_UNINSTALL_COMPLETION
  Call un.RecordCompletion
FunctionEnd

Section Uninstall

  ; The scoped helper closes only processes using this exact installation.
  !ifmacrodef NSIS_HOOK_PREUNINSTALL
    !insertmacro NSIS_HOOK_PREUNINSTALL
  !endif

  ; Delete resources
  {{#each resources}}
    !insertmacro LLREMOTE_DELETE_UNINSTALL_FILE "{{this.[1]}}" ""
  {{/each}}

  ; Delete external binaries
  {{#each binaries}}
    !insertmacro LLREMOTE_DELETE_UNINSTALL_FILE "{{this}}" ""
  {{/each}}
  ; Delete app associations
  {{#each file_associations as |association| ~}}
    {{#each association.ext as |ext| ~}}
      !insertmacro APP_UNASSOCIATE "{{ext}}" "{{or association.name ext}}"
    {{/each}}
  {{/each}}
  ; Delete deep links
  {{#each deep_link_protocols as |protocol| ~}}
    ReadRegStr $R7 SHCTX "Software\Classes\\{{protocol}}\shell\open\command" ""
    ${If} $R7 == "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
      DeleteRegKey SHCTX "Software\Classes\\{{protocol}}"
    ${EndIf}
  {{/each}}


  ; Keep the App until resources have been removed; preserve the recovery
  ; command and registration until every package file deletion has succeeded.
  !insertmacro LLREMOTE_DELETE_UNINSTALL_FILE "${MAINBINARYNAME}.exe" "/REBOOTOK"

  ; Remove shortcuts if not updating
  ${If} $UpdateMode <> 1
  ${AndIf} $UninstallScope == "all"
    ; Do not modify the authorizing administrator's personal jump lists/pins.
    ; Remove start menu shortcut
    !insertmacro MUI_STARTMENU_GETFOLDER Application $AppStartMenuFolder
    !insertmacro IsShortcutTarget "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      ClearErrors
      Delete "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
      ${If} ${Errors}
        !insertmacro LLREMOTE_UNINSTALL_WARN 110 "$SMPROGRAMS"
      ${EndIf}
      RMDir "$SMPROGRAMS\$AppStartMenuFolder"
    ${EndIf}
    !insertmacro IsShortcutTarget "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      ClearErrors
      Delete "$SMPROGRAMS\${PRODUCTNAME}.lnk"
      ${If} ${Errors}
        !insertmacro LLREMOTE_UNINSTALL_WARN 110 "$SMPROGRAMS"
      ${EndIf}
    ${EndIf}
    ; Remove desktop shortcuts
    !insertmacro IsShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      ClearErrors
      Delete "$DESKTOP\${PRODUCTNAME}.lnk"
      ${If} ${Errors}
        !insertmacro LLREMOTE_UNINSTALL_WARN 110 "$DESKTOP"
      ${EndIf}
    ${EndIf}
  ${EndIf}
  !ifmacrodef NSIS_HOOK_POSTUNINSTALL
    !insertmacro NSIS_HOOK_POSTUNINSTALL
  !endif
  ; A direct _?= launch can still be executing uninstall.exe. If necessary,
  ; explicitly schedule its final removal instead of silently ignoring failure.
  ClearErrors
  Delete /REBOOTOK "$INSTDIR\.llremote-uninstall-helper.exe"
  ${If} ${FileExists} "$INSTDIR\uninstall.exe"
    Delete /REBOOTOK "$INSTDIR\uninstall.exe"
  ${Else}
    !insertmacro LLREMOTE_UNINSTALL_LOG "already_absent: uninstall.exe"
  ${EndIf}
  Delete "$INSTDIR\.llremote-install-owner"
  ${If} ${Errors}
    !insertmacro LLREMOTE_UNINSTALL_WARN 134 "$INSTDIR"
  ${EndIf}
  {{#each resources_ancestors}}
  RMDir "$INSTDIR\\{{this}}"
  {{/each}}
  ${If} ${FileExists} "$INSTDIR"
    RMDir "$INSTDIR"
  ${Else}
    ClearErrors
  ${EndIf}
  ${If} ${Errors}
    StrCpy $R3 0
    IfRebootFlag 0 +2
      StrCpy $R3 1
    ${If} $R3 = 0
      !insertmacro LLREMOTE_UNINSTALL_WARN 134 "$INSTDIR"
    ${EndIf}
  ${EndIf}
  ClearErrors

  ; Auto close if passive mode or updating
  ${If} $PassiveMode = 1
  ${OrIf} $UpdateMode = 1
    SetAutoClose true
  ${EndIf}
SectionEnd

Function ValidateScopeConflict
  ${If} $MultiUser.InstallMode == "AllUsers"
    ReadRegStr $0 HKCU "${UNINSTKEY}" "UninstallString"
    ${If} $0 != ""
      Abort "$(ScopeConflictCurrent)"
    ${EndIf}
  ${Else}
    ReadRegStr $0 HKLM "${UNINSTKEY}" "UninstallString"
    ${If} $0 != ""
      Abort "$(ScopeConflictMachine)"
    ${EndIf}
  ${EndIf}
FunctionEnd

Function ValidateExistingVersion
  ${If} $ExistingComparison = -1
    MessageBox MB_ICONSTOP|MB_OK "$(DowngradeBlocked)" /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
  ${If} $ExistingComparison != 0
  ${AndIf} $ExistingComparison != 1
    MessageBox MB_ICONSTOP|MB_OK "$(InstallationRecordInvalid)" /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
FunctionEnd

Function DetectExistingInstallation
  StrCpy $ExistingInstall 0
  ReadRegStr $0 SHCTX "${UNINSTKEY}" "UninstallString"
  ${If} $0 == ""
    Return
  ${EndIf}
  StrCpy $ExistingInstall 1
  ReadRegStr $ExistingVersion SHCTX "${UNINSTKEY}" "DisplayVersion"
  ReadRegStr $ExistingInstallDir SHCTX "${MANUPRODUCTKEY}" ""
  ${If} $ExistingVersion == ""
  ${OrIf} $ExistingInstallDir == ""
    MessageBox MB_ICONSTOP|MB_OK "$(InstallationRecordInvalid)" /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
  nsis_tauri_utils::SemverCompare "${VERSION}" $ExistingVersion
  Pop $ExistingComparison
  Call ValidateExistingVersion
  StrCpy $INSTDIR $ExistingInstallDir
FunctionEnd

Function SkipDirectoryForExisting
  Call DetectExistingInstallation
  ${If} $ExistingInstall = 1
  ${OrIf} $PassiveMode = 1
    Abort
  ${EndIf}
FunctionEnd

Function RestorePreviousInstallLocation
  ReadRegStr $4 SHCTX "${MANUPRODUCTKEY}" ""
  StrCmp $4 "" +2 0
    StrCpy $INSTDIR $4
FunctionEnd
Function Skip
  Abort
FunctionEnd

Function SkipIfPassive
  ${IfThen} $PassiveMode = 1  ${|} Abort ${|}
FunctionEnd
Function SkipIfPassiveOrResume
  ${If} $PassiveMode = 1
  ${OrIf} $ScopeResumed = 1
    Abort
  ${EndIf}
FunctionEnd
Function un.SkipIfPassive
  ${IfThen} $PassiveMode = 1  ${|} Abort ${|}
FunctionEnd

Function CreateOrUpdateStartMenuShortcut
  ${If} $UpdateMode = 1
  ${OrIf} $NoShortcutMode = 1
    Return
  ${EndIf}
  !if "${STARTMENUFOLDER}" != ""
    !insertmacro IsShortcutTarget "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      Return
    ${EndIf}
    CreateDirectory "$SMPROGRAMS\$AppStartMenuFolder"
    CreateShortcut "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
  !else
    !insertmacro IsShortcutTarget "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      Return
    ${EndIf}
    CreateShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\${PRODUCTNAME}.lnk"
  !endif
FunctionEnd
Function CreateOrUpdateDesktopShortcut
  !insertmacro IsShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
  Pop $0
  ${If} $0 = 1
    Return
  ${EndIf}
  ${If} $NoShortcutMode = 1
    Return
  ${EndIf}

  CreateShortcut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
  !insertmacro SetLnkAppUserModelId "$DESKTOP\${PRODUCTNAME}.lnk"
FunctionEnd

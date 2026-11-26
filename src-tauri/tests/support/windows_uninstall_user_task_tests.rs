use super::*;

const OWNER: &str = "S-1-5-21-1-2-3-1001";

#[test]
fn critical_stop_has_a_separate_action_and_a_quoted_owner_report_path() {
    let xml = definition(
        OWNER,
        "C:/App/helper.exe",
        "C:/App",
        UserAction::Stop,
        Some("C:/A&B/Temp/cleanup report.json"),
    )
    .unwrap();
    assert!(xml.contains("--uninstall-user-stop --target-user"));
    assert!(xml.contains("--uninstall-result &quot;C:/A&amp;B/Temp/cleanup report.json&quot;"));
    assert!(!xml.contains("--delete-data"));
}

#[test]
fn delegated_cleanup_uses_the_install_owner_existing_session_at_normal_privilege() {
    let xml = definition(
        OWNER,
        r"C:\Users\admin\AppData\Local\Temp\uninstall-maintenance.exe",
        r"C:\Users\user\AppData\Local\LL Remote",
        UserAction::Clean { delete_data: true },
        None,
    )
    .unwrap();
    assert!(xml.contains("<UserId>S-1-5-21-1-2-3-1001</UserId>"));
    assert!(xml.contains("<LogonType>InteractiveToken</LogonType>"));
    assert!(xml.contains("<RunLevel>LeastPrivilege</RunLevel>"));
    assert!(xml.contains("--uninstall-user-clean --target-user S-1-5-21-1-2-3-1001 --delete-data"));
    assert!(
        xml.contains(r"--uninstall-directory &quot;C:\Users\user\AppData\Local\LL Remote&quot;")
    );
    assert!(!xml.contains("<Triggers>"));
    assert!(!xml.contains("Password"));
}

#[test]
fn check_action_cannot_delete_personal_data_and_executable_is_xml_escaped() {
    let xml = definition(
        OWNER,
        r"C:\Users\A&B\LL Remote\app.exe",
        r"C:\Users\A&B\LL Remote",
        UserAction::Check,
        None,
    )
    .unwrap();
    assert!(xml.contains("<Command>C:\\Users\\A&amp;B\\LL Remote\\app.exe</Command>"));
    assert!(xml.contains("--uninstall-user-check --target-user S-1-5-21-1-2-3-1001"));
    assert!(xml.contains(r"--uninstall-directory &quot;C:\Users\A&amp;B\LL Remote&quot;"));
    assert!(!xml.contains("--delete-data"));
    assert!(definition("SYSTEM", "app.exe", "C:/App", UserAction::Check, None).is_err());
}

use super::*;

#[derive(Default)]
struct Fixture {
    configured: bool,
    credential: bool,
    fail: Option<&'static str>,
    calls: Vec<&'static str>,
}
impl Fixture {
    fn step(&mut self, name: &'static str) -> Result<(), String> {
        self.calls.push(name);
        if self.fail == Some(name) {
            Err("windows_service_install_authorization_failed".into())
        } else {
            Ok(())
        }
    }
}
impl Operations for Fixture {
    fn inspect(&mut self) -> Result<(bool, bool), String> {
        self.step("inspect")?;
        Ok((self.configured, self.credential))
    }
    fn install(&mut self) -> Result<(), String> {
        self.step("install")
    }
    fn configure(&mut self, request: WindowsServiceConfigureRequest) -> Result<(), String> {
        assert_eq!(request.bind_address, "127.0.0.1");
        assert_eq!(request.port, 5913);
        self.step("configure")
    }
    fn authorize(&mut self) -> Result<(), String> {
        self.step("authorize")
    }
}
fn request(password: &str) -> WindowsServiceConfigureRequest {
    WindowsServiceConfigureRequest {
        bind_address: "127.0.0.1".into(),
        port: 5913,
        view_only: true,
        password: password.into(),
    }
}
#[test]
fn setup_finishes_configuration_and_delegation_without_starting_sharing() {
    let mut f = Fixture::default();
    run(&mut f, request("fixture")).unwrap();
    assert_eq!(f.calls, ["inspect", "install", "configure", "authorize"]);
}
#[test]
fn invalid_setup_never_installs_or_overwrites_an_existing_configuration() {
    for (configured, credential, password) in [
        (false, false, ""),
        (true, true, "fixture"),
        (false, false, "123456789"),
    ] {
        let mut f = Fixture {
            configured,
            credential,
            ..Default::default()
        };
        assert!(run(&mut f, request(password)).is_err());
        assert_eq!(f.calls, ["inspect"]);
    }
    let mut f = Fixture {
        credential: true,
        ..Default::default()
    };
    run(&mut f, request("")).unwrap();
}
#[test]
fn partial_setup_never_reports_success_and_stops_at_the_failed_stage() {
    for (stage, count) in [
        ("inspect", 1),
        ("install", 2),
        ("configure", 3),
        ("authorize", 4),
    ] {
        let mut f = Fixture {
            fail: Some(stage),
            ..Default::default()
        };
        assert_eq!(
            run(&mut f, request("fixture")).unwrap_err(),
            "windows_service_install_authorization_failed"
        );
        assert_eq!(f.calls.len(), count);
    }
}

#[test]
fn saved_password_reuse_reads_only_the_backend_store() {
    let password =
        super::reuse_password("", || Ok(zeroize::Zeroizing::new("fixture".to_string()))).unwrap();
    assert_eq!(password, "fixture");
}
#[test]
fn password_reuse_rejects_ambiguous_sources_before_reading_credentials() {
    assert_eq!(
        super::reuse_password("typed", || panic!("must not read credentials")).unwrap_err(),
        "windows_service_invalid_config"
    );
}
#[test]
fn unavailable_saved_password_preserves_the_error_for_the_existing_form() {
    assert_eq!(
        super::reuse_password("", || Err("background_share_credential_missing".into()))
            .unwrap_err(),
        "background_share_credential_missing"
    );
}

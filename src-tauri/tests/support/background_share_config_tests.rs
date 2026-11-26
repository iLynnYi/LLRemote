use super::*;
use std::cell::RefCell;

#[test]
fn starting_share_preserves_the_independent_login_policy() {
    for previous in [true, false] {
        let mut config = Config {
            auto_start: previous,
            ..Config::default()
        };
        config.apply_start(&BackgroundShareStartRequest {
            bind_address: "127.0.0.1".into(),
            port: 5903,
            view_only: false,
            password: String::new(),
            temporary_password: false,
        });
        assert_eq!(config.auto_start, previous);
    }
}

fn configured() -> Config {
    Config {
        bind_address: "127.0.0.1".into(),
        port: 5903,
        credential_ref: Some("system://background-sharing/user-server".into()),
        ..Config::default()
    }
}

#[test]
fn loading_saved_config_preserves_settings_and_rejects_invalid_content() {
    let directory = std::env::temp_dir().join(format!("vnc-share-config-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("server.json");
    assert_eq!(Config::load_from(&directory).unwrap().port, 0);

    let mut expected = configured();
    expected.auto_start = true;
    expected.view_only = true;
    let current = serde_json::to_value(&expected).unwrap();
    let bytes = serde_json::to_vec(&current).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    assert_eq!(
        serde_json::to_value(Config::load_from(&directory).unwrap()).unwrap(),
        current
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);

    let mut unknown_field = current;
    unknown_field["unknownPolicy"] = serde_json::json!(true);
    for bytes in [
        b"{".to_vec(),
        b"null".to_vec(),
        serde_json::to_vec(&unknown_field).unwrap(),
    ] {
        std::fs::write(&path, &bytes).unwrap();
        assert!(
            matches!(Config::load_from(&directory), Err(code) if code == "background_share_config_invalid")
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    std::fs::remove_dir_all(directory).unwrap();
}

fn synthetic_secret() -> Zeroizing<String> {
    Zeroizing::new(uuid::Uuid::new_v4().simple().to_string()[..8].to_owned())
}

fn start_request(temporary: bool, password: String) -> BackgroundShareStartRequest {
    BackgroundShareStartRequest {
        bind_address: "127.0.0.1".into(),
        port: 5903,
        view_only: false,
        password,
        temporary_password: temporary,
    }
}

#[test]
fn temporary_start_skips_all_credential_and_persistence_operations() {
    for auto_start in [true, false] {
        let mut config = Config {
            auto_start,
            ..configured()
        };
        let before = serde_json::to_value(&config).unwrap();
        let secret = synthetic_secret();
        let mut request = start_request(true, secret.to_string());
        let passwords = passwords_for_start_with(&config, &mut request, |_| {
            panic!("temporary sharing must not read active or rollback credentials")
        })
        .unwrap();
        assert_eq!(passwords.active.as_str(), secret.as_str());
        assert!(passwords.previous.is_none());
        assert!(request.password.is_empty());
        persist_start_with(
            &mut config,
            true,
            PasswordChange {
                new: Some(passwords.active.as_str()),
                previous: None,
            },
            |_, _| panic!("temporary sharing must not save credentials"),
            |_| panic!("temporary sharing must not change login registration"),
            |_| panic!("temporary sharing must not persist listener settings"),
            |_| panic!("temporary sharing must not restore or delete credentials"),
        )
        .unwrap();
        assert_eq!(serde_json::to_value(&config).unwrap(), before);
    }
}

#[test]
fn an_empty_temporary_password_cannot_fall_back_to_the_saved_secret() {
    let mut request = start_request(true, String::new());
    let result = passwords_for_start_with(&configured(), &mut request, |_| {
        panic!("an empty temporary password must never read saved credentials")
    });
    assert!(matches!(result, Err(code) if code == "background_share_config_invalid"));
}

#[test]
fn invalid_explicit_passwords_and_settings_are_rejected_before_reading_rollback_credentials() {
    let secret = synthetic_secret();
    let oversized = Zeroizing::new(format!("{}x", secret.as_str()));
    for temporary in [true, false] {
        for (port, password) in [(0, secret.as_str()), (5903, oversized.as_str())] {
            let mut request = start_request(temporary, password.into());
            request.port = port;
            let result = passwords_for_start_with(&configured(), &mut request, |_| {
                panic!("invalid explicit input must not prompt for existing credentials")
            });
            assert!(matches!(result, Err(code) if code == "background_share_config_invalid"));
        }
    }
}

#[test]
fn normal_starts_reuse_saved_credentials_or_read_the_previous_secret_for_rollback() {
    let saved = synthetic_secret();
    let replacement = synthetic_secret();
    for supplied in [String::new(), replacement.to_string()] {
        let had_new_password = !supplied.is_empty();
        let mut request = start_request(false, supplied);
        let mut reads = 0;
        let passwords = passwords_for_start_with(&configured(), &mut request, |_| {
            reads += 1;
            Ok(Zeroizing::new(saved.to_string()))
        })
        .unwrap();
        assert_eq!(reads, 1);
        assert_eq!(
            passwords.active.as_str(),
            if had_new_password {
                replacement.as_str()
            } else {
                saved.as_str()
            }
        );
        assert_eq!(
            passwords.previous.as_deref().map(String::as_str),
            had_new_password.then_some(saved.as_str())
        );
    }
}

#[test]
fn current_start_requests_omit_retired_policy_fields_and_keep_temporary_intent_explicit() {
    let normal = serde_json::to_value(start_request(false, String::new())).unwrap();
    assert!(normal.get("temporaryPassword").is_none());
    assert!(normal.get("saveCredential").is_none());
    assert!(normal.get("autoStart").is_none());
    let decoded: BackgroundShareStartRequest = serde_json::from_value(normal.clone()).unwrap();
    assert!(!decoded.temporary_password);
    let mut legacy = normal;
    legacy["saveCredential"] = serde_json::json!(true);
    assert!(serde_json::from_value::<BackgroundShareStartRequest>(legacy).is_err());
    let temporary = serde_json::to_value(start_request(true, String::new())).unwrap();
    assert_eq!(temporary["temporaryPassword"], true);
}

#[test]
fn only_login_launch_with_auto_start_restores_sharing() {
    let config = Config {
        auto_start: true,
        ..configured()
    };
    assert!(config.restores_listener(BackgroundShareLaunchMode::Login));
    assert!(!config.restores_listener(BackgroundShareLaunchMode::Idle));
    let disabled = Config {
        auto_start: false,
        ..config
    };
    assert!(!disabled.restores_listener(BackgroundShareLaunchMode::Login));
}

#[test]
fn launch_arguments_distinguish_login_from_manual_preparation() {
    assert!(matches!(
        BackgroundShareLaunchMode::from_arguments(&["--background-share".into()]),
        Some(BackgroundShareLaunchMode::Login)
    ));
    assert!(matches!(
        BackgroundShareLaunchMode::from_arguments(&["--background-share-idle".into()]),
        Some(BackgroundShareLaunchMode::Idle)
    ));
    assert!(BackgroundShareLaunchMode::from_arguments(&[]).is_none());
    assert!(BackgroundShareLaunchMode::from_arguments(&[
        "--background-share-idle".into(),
        "unexpected".into()
    ])
    .is_none());
}

#[test]
fn enabling_login_start_requires_a_readable_saved_secret_and_valid_listener() {
    let config = configured();
    assert_eq!(
        validate_auto_start_with(&config, true, |_| {
            Err("background_share_credential_missing".into())
        })
        .unwrap_err(),
        "background_share_auto_start_credential_required"
    );
    assert_eq!(
        validate_auto_start_with(&config, true, |_| {
            Err("background_share_credential_unavailable".into())
        })
        .unwrap_err(),
        "background_share_credential_unavailable"
    );
    let saved = synthetic_secret();
    let oversized = Zeroizing::new(format!("{}x", saved.as_str()));
    for (port, address, password) in [
        (0, "127.0.0.1", saved.as_str()),
        (5903, "invalid", saved.as_str()),
        (5903, "127.0.0.1", ""),
        (5903, "127.0.0.1", oversized.as_str()),
    ] {
        let invalid = Config {
            port,
            bind_address: address.into(),
            ..configured()
        };
        assert_eq!(
            validate_auto_start_with(&invalid, true, |_| Ok(Zeroizing::new(password.into())))
                .unwrap_err(),
            "background_share_config_invalid"
        );
    }
    validate_auto_start_with(&config, true, |_| Ok(saved)).unwrap();
}

#[test]
fn disabling_login_start_never_reads_credentials_or_requires_valid_configuration() {
    validate_auto_start_with(&Config::default(), false, |_| {
        panic!("disabling must remain possible when the saved credential is lost")
    })
    .unwrap();
}

#[test]
fn a_deleted_saved_credential_can_be_replaced_but_denied_storage_is_preserved() {
    assert!(previous_password_with(&configured(), |_| {
        Err("background_share_credential_missing".into())
    })
    .unwrap()
    .is_none());
    assert_eq!(
        previous_password_with(&configured(), |_| {
            Err("background_share_credential_unavailable".into())
        })
        .unwrap_err(),
        "background_share_credential_unavailable"
    );
    assert!(previous_password_with(&Config::default(), |_| {
        panic!("a first save has no prior credential to retrieve")
    })
    .unwrap()
    .is_none());
}

#[test]
fn new_password_is_persisted_with_settings_and_login_policy_is_untouched() {
    let mut next = Config {
        auto_start: true,
        ..configured()
    };
    let actions = RefCell::new(Vec::new());
    let password = synthetic_secret();
    persist_start_with(
        &mut next,
        false,
        PasswordChange {
            new: Some(password.as_str()),
            previous: None,
        },
        |config, secret| {
            assert_eq!(secret, password.as_str());
            config.credential_ref = Some("system://background-sharing/user-server".into());
            actions.borrow_mut().push("secret");
            Ok(())
        },
        |enabled| {
            assert!(enabled);
            actions.borrow_mut().push("registration");
            Ok(())
        },
        |config| {
            assert!(config.auto_start);
            let serialized = serde_json::to_value(config).unwrap();
            assert!(serialized.get("password").is_none());
            actions.borrow_mut().push("config");
            Ok(())
        },
        |_| panic!("successful save must not roll back"),
    )
    .unwrap();
    assert_eq!(*actions.borrow(), ["secret", "registration", "config"]);
}

#[test]
fn reusing_a_saved_password_does_not_rewrite_or_roll_back_the_secret() {
    persist_start_with(
        &mut configured(),
        false,
        PasswordChange {
            new: None,
            previous: None,
        },
        |_, _| panic!("reusing must not rewrite the saved credential"),
        |_| Ok(()),
        |_| Err("background_share_config_unavailable".into()),
        |_| panic!("settings failure must not delete an unchanged credential"),
    )
    .unwrap_err();
}

#[test]
fn secret_save_failure_preserves_the_old_secret_and_skips_settings() {
    let restored = RefCell::new(None);
    let password = synthetic_secret();
    let previous = synthetic_secret();
    let code = persist_start_with(
        &mut configured(),
        false,
        PasswordChange {
            new: Some(password.as_str()),
            previous: Some(previous.as_str()),
        },
        |_, _| Err("background_share_credential_unavailable".into()),
        |_| panic!("registration is unchanged when credential saving fails"),
        |_| panic!("settings must not persist without a saved password"),
        |previous| {
            *restored.borrow_mut() = previous.map(str::to_owned);
            Ok(())
        },
    )
    .unwrap_err();
    assert_eq!(code, "background_share_credential_unavailable");
    assert_eq!(restored.borrow().as_deref(), Some(previous.as_str()));
}

#[test]
fn first_secret_save_denial_with_failed_cleanup_is_distinct_from_a_failed_replacement_rollback() {
    let password = synthetic_secret();
    let restored = RefCell::new(false);
    let code = persist_start_with(
        &mut Config::default(),
        false,
        PasswordChange {
            new: Some(password.as_str()),
            previous: None,
        },
        |_, _| Err("background_share_credential_unavailable".into()),
        |_| panic!("registration must not change after a denied first save"),
        |_| panic!("settings must not persist after a denied first save"),
        |previous| {
            assert!(previous.is_none());
            *restored.borrow_mut() = true;
            Err("background_share_credential_unavailable".into())
        },
    )
    .unwrap_err();
    assert!(*restored.borrow());
    assert_eq!(code, "background_share_initial_credential_rollback_failed");
}

#[test]
fn first_secret_saved_then_failed_settings_still_reports_a_real_rollback_failure() {
    let password = synthetic_secret();
    let code = persist_start_with(
        &mut Config::default(),
        false,
        PasswordChange {
            new: Some(password.as_str()),
            previous: None,
        },
        |config, _| {
            config.credential_ref = Some("system://background-sharing/user-server".into());
            Ok(())
        },
        |_| Ok(()),
        |_| Err("background_share_config_unavailable".into()),
        |_| Err("background_share_credential_unavailable".into()),
    )
    .unwrap_err();
    assert_eq!(code, "background_share_credential_rollback_failed");
}

#[test]
fn failed_settings_restore_the_old_secret_or_remove_a_new_entry() {
    let password = synthetic_secret();
    let previous_secret = synthetic_secret();
    for previous in [Some(previous_secret.as_str()), None] {
        let restored = RefCell::new((false, None));
        let code = persist_start_with(
            &mut configured(),
            false,
            PasswordChange {
                new: Some(password.as_str()),
                previous,
            },
            |_, _| Ok(()),
            |_| Ok(()),
            |_| Err("background_share_config_unavailable".into()),
            |secret| {
                *restored.borrow_mut() = (true, secret.map(str::to_owned));
                Ok(())
            },
        )
        .unwrap_err();
        assert_eq!(code, "background_share_config_unavailable");
        assert!(restored.borrow().0);
        assert_eq!(restored.borrow().1.as_deref(), previous);
    }
}

#[test]
fn failed_secret_rollback_is_reported_as_a_distinct_error() {
    let password = synthetic_secret();
    let previous = synthetic_secret();
    let registrations = RefCell::new(Vec::new());
    let code = persist_start_with(
        &mut configured(),
        false,
        PasswordChange {
            new: Some(password.as_str()),
            previous: Some(previous.as_str()),
        },
        |_, _| Ok(()),
        |enabled| {
            registrations.borrow_mut().push(enabled);
            Ok(())
        },
        |_| Err("background_share_config_unavailable".into()),
        |_| Err("background_share_credential_unavailable".into()),
    )
    .unwrap_err();
    assert_eq!(code, "background_share_credential_rollback_failed");
    assert_eq!(*registrations.borrow(), [false, false]);
}

#[test]
fn failed_start_registration_restores_credentials_and_does_not_save_settings() {
    let password = synthetic_secret();
    let previous = synthetic_secret();
    let restored = RefCell::new(None);
    let attempts = RefCell::new(0);
    let code = persist_start_with(
        &mut configured(),
        false,
        PasswordChange {
            new: Some(password.as_str()),
            previous: Some(previous.as_str()),
        },
        |_, _| Ok(()),
        |_| {
            *attempts.borrow_mut() += 1;
            if *attempts.borrow() == 1 {
                Err("background_share_startup_unavailable".into())
            } else {
                Ok(())
            }
        },
        |_| panic!("failed registration must not persist settings"),
        |previous| {
            *restored.borrow_mut() = previous.map(str::to_owned);
            Ok(())
        },
    )
    .unwrap_err();
    assert_eq!(code, "background_share_startup_unavailable");
    assert_eq!(*attempts.borrow(), 2);
    assert_eq!(restored.borrow().as_deref(), Some(previous.as_str()));
}

#[test]
fn failed_login_policy_save_restores_the_previous_registration() {
    let previous = configured();
    let next = Config {
        auto_start: true,
        ..previous.clone()
    };
    let registrations = RefCell::new(Vec::new());
    let code = persist_auto_start_with(
        &previous,
        &next,
        |enabled| {
            registrations.borrow_mut().push(enabled);
            Ok(())
        },
        |_| Err("background_share_config_unavailable".into()),
    )
    .unwrap_err();
    assert_eq!(code, "background_share_config_unavailable");
    assert_eq!(*registrations.borrow(), [true, false]);
    assert!(!previous.auto_start);
}

#[test]
fn failed_registration_rollback_is_reported_and_settings_are_not_saved() {
    let previous = configured();
    let next = Config {
        auto_start: true,
        ..previous.clone()
    };
    let code = persist_auto_start_with(
        &previous,
        &next,
        |_| Err("background_share_startup_unavailable".into()),
        |_| panic!("failed registration must not persist the new policy"),
    )
    .unwrap_err();
    assert_eq!(code, "background_share_startup_rollback_failed");
}

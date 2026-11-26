use super::*;

#[test]
fn dropping_the_handshake_clears_material_retained_by_the_connection_owner() {
    let h = handshake();
    let owner = h.authentication.clone();
    assert!(owner.has_key().unwrap());
    drop(h);
    assert!(!owner.has_key().unwrap());
}

fn handshake() -> RfbClientHandshake {
    // Runtime-generated synthetic input; never read a stored credential.
    RfbClientHandshake::with_password(Some(uuid::Uuid::new_v4().to_string()))
}

#[test]
fn none_security_releases_unused_credentials_at_selection_for_all_versions() {
    for minor in [3, 7, 8] {
        let mut h = handshake();
        h.consume(format!("RFB 003.{minor:03}\n").as_bytes())
            .unwrap();
        let security = if minor == 3 {
            vec![0, 0, 0, 1]
        } else {
            vec![1, 1]
        };
        h.consume(&security).unwrap();
        assert!(
            !h.authentication.has_key().unwrap(),
            "None does not retain auth material"
        );
    }
}

#[test]
fn a_fragmented_vnc_challenge_releases_credentials_after_the_response() {
    for minor in [3, 7, 8] {
        let mut h = handshake();
        h.consume(format!("RFB 003.{minor:03}\n").as_bytes())
            .unwrap();
        let security = if minor == 3 {
            vec![0, 0, 0, 2]
        } else {
            vec![1, 2]
        };
        h.consume(&security).unwrap();
        assert!(h.authentication.has_key().unwrap());
        assert!(h.consume(&[0; 15]).unwrap().is_empty());
        assert!(h.authentication.has_key().unwrap());
        let writes = h.consume(&[0]).unwrap();
        assert_eq!(writes.iter().map(Vec::len).collect::<Vec<_>>(), [16]);
        assert!(!h.authentication.has_key().unwrap());
        assert!(h.consume(&1u32.to_be_bytes()).is_err());
        assert!(!h.authentication.has_key().unwrap());
    }
}

#[test]
fn handshake_errors_release_credentials_and_make_the_parser_terminal() {
    for invalid in [
        b"bad bad bad!".to_vec(),
        b"RFB 004.008\n".to_vec(),
        [b"RFB 003.008\n".as_slice(), &[0]].concat(),
        [b"RFB 003.008\n".as_slice(), &[1, 99]].concat(),
        [b"RFB 003.003\n".as_slice(), &[0, 0, 0, 99]].concat(),
    ] {
        let mut h = handshake();
        assert!(h.consume(&invalid).is_err());
        assert!(
            !h.authentication.has_key().unwrap(),
            "terminal parser releases auth material"
        );
        assert!(h.take_events().is_empty());
        assert_eq!(h.consume(&[]).unwrap_err(), "rfb_handshake_failed");
    }
}

#[test]
fn missing_credentials_remain_a_stable_retry_challenge() {
    for minor in [3, 7, 8] {
        let mut h = RfbClientHandshake::default();
        h.consume(format!("RFB 003.{minor:03}\n").as_bytes())
            .unwrap();
        let security = if minor == 3 {
            vec![0, 0, 0, 2]
        } else {
            vec![1, 2]
        };
        assert_eq!(
            h.consume(&security).unwrap_err(),
            "rfb_credentials_required"
        );
        assert_eq!(h.consume(&[]).unwrap_err(), "rfb_handshake_failed");
    }
}

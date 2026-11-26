use super::*;

fn reply_case(
    reply: impl FnOnce(TcpStream) + Send + 'static,
    timeout: Duration,
) -> Result<AgentResponse, String> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let worker = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut request)
            .unwrap();
        reply(stream);
    });
    let response = send_agent_request_with_timeout(
        "127.0.0.1",
        port,
        serde_json::json!({"type": "audit-test"}),
        timeout,
    );
    worker.join().unwrap();
    response
}

#[test]
fn agent_response_rejects_oversized_json_before_decoding_it() {
    let result = reply_case(
        |mut stream| {
            let _ = stream.write_all(b"{\"ok\":true,\"text\":\"");
            for _ in 0..144 {
                if stream.write_all(&vec![b'x'; 64 * 1024]).is_err() {
                    return;
                }
            }
            let _ = stream.write_all(b"\"}\n");
        },
        Duration::from_secs(3),
    );
    let Err(error) = result else {
        panic!("oversized response must fail without echoing its body")
    };
    assert_eq!(error, "agent_response_too_large");
}

#[test]
fn agent_response_deadline_cannot_be_renewed_by_a_drip_feed() {
    let started = Instant::now();
    let result = reply_case(
        |mut stream| {
            for _ in 0..8 {
                if stream.write_all(b" ").is_err() {
                    return;
                }
                thread::sleep(Duration::from_millis(40));
            }
            let _ = stream.write_all(b"{\"ok\":true}\n");
        },
        Duration::from_millis(150),
    );
    let Err(error) = result else {
        panic!("slow response must time out without echoing its body")
    };
    assert!(error.starts_with("agent_response_timeout"));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn agent_response_requires_complete_line_framing() {
    let result = reply_case(
        |mut stream| {
            stream.write_all(b"{\"ok\":true}").unwrap();
        },
        Duration::from_secs(2),
    );
    assert_eq!(result.unwrap_err(), "agent_response_incomplete");
}

#[test]
fn agent_response_still_accepts_utf8_and_empty_optional_fields() {
    let result = reply_case(
        |mut stream| {
            stream
                .write_all("{\"ok\":true,\"text\":\"中文剪贴板\"}\n".as_bytes())
                .unwrap();
        },
        Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(result.text.as_deref(), Some("中文剪贴板"));
}

#[test]
fn agent_response_budget_includes_the_line_terminator_and_stops_network_reads() {
    let mut response = b"{\"ok\":true}".to_vec();
    response.resize(MAX_AGENT_RESPONSE_BYTES - 1, b' ');
    response.push(b'\n');
    assert!(read_bounded_agent_response(response.as_slice()).unwrap().ok);
    response.insert(0, b' ');
    response.extend_from_slice(b"unread trailer");
    let mut cursor = std::io::Cursor::new(response);
    assert_eq!(
        read_bounded_agent_response(&mut cursor).err().unwrap(),
        "agent_response_too_large"
    );
    assert_eq!(cursor.position(), MAX_AGENT_RESPONSE_BYTES as u64 + 1);
}

#[test]
fn agent_response_decode_errors_do_not_echo_untrusted_data() {
    let invalid = b"{\"ok\":\"untrusted-synthetic-marker\"}\n";
    assert_eq!(
        read_bounded_agent_response(invalid.as_slice())
            .err()
            .unwrap(),
        "agent_response_decode_failed"
    );
}

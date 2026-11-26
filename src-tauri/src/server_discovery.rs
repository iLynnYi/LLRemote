//! LL Remote discovery framing carried by the standard RFB Fence extension.
//! Discovery carries declarations only. Agent authentication happens on its
//! independently established transport and never uses the RFB/Fence payload.

use crate::remote_desktop_core::rfb_fence::{fence_message, FENCE_FLAG_REQUEST, MAX_FENCE_PAYLOAD};
use std::io::{self, Write};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

pub const FENCE_PSEUDO_ENCODING: i32 = -312;
const FRAME_MAGIC: &[u8; 4] = b"VWSD";
const HELLO_TYPE: u8 = 1;
const DISCOVER_TYPE: u8 = 2;
const MANIFEST_TYPE: u8 = 3;
const ERROR_TYPE: u8 = 4;

#[derive(Debug, Clone)]
pub struct ServerDiscoveryContext {
    server_id: String,
    view_only: bool,
    agent_port: Option<u16>,
    agent_ticket_store: Option<crate::local_agent_service::AgentTicketStore>,
    revision: Arc<AtomicU64>,
    #[cfg(target_os = "windows")]
    service_agent: bool,
}

impl ServerDiscoveryContext {
    pub fn new(
        server_id: String,
        view_only: bool,
        agent_port: Option<u16>,
        agent_ticket_store: Option<crate::local_agent_service::AgentTicketStore>,
    ) -> Self {
        Self {
            // This is opaque, per server lifetime, and deliberately contains no address.
            server_id,
            view_only,
            agent_port,
            agent_ticket_store,
            revision: Arc::new(AtomicU64::new(0)),
            #[cfg(target_os = "windows")]
            service_agent: false,
        }
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    #[cfg(any(target_os = "windows", test))]
    pub(crate) fn revision_handle(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.revision)
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn with_service_agent(mut self, enabled: bool) -> Self {
        self.service_agent = enabled;
        self
    }
}

pub fn hello_frame() -> Vec<u8> {
    let body = br#"{"minVersion":"1.0","maxVersion":"1.0"}"#;
    let mut frame = Vec::with_capacity(10 + body.len());
    frame.extend_from_slice(FRAME_MAGIC);
    frame.extend_from_slice(&[1, HELLO_TYPE, 0, 0, 0, 1]);
    frame.extend_from_slice(body);
    frame
}

pub fn write_hello<W: Write>(stream: &mut W) -> io::Result<()> {
    write_fence_payload(stream, &hello_frame())
}

pub fn handle_discover_request<W: Write>(
    stream: &mut W,
    context: &ServerDiscoveryContext,
    payload: &[u8],
) -> io::Result<()> {
    let Some((request_id, frame_type, body)) = parse_single_frame(payload) else {
        return Ok(());
    };
    if frame_type != DISCOVER_TYPE || serde_json::from_slice::<serde_json::Value>(body).is_err() {
        return write_json_response(
            stream,
            ERROR_TYPE,
            request_id,
            serde_json::json!({
                "version": "1.0", "code": "malformed_request", "retryable": false
            }),
        );
    }
    // This phase deliberately contains no credential or authentication result.
    let mut manifest = serde_json::json!({
        "version": "1.0",
        "server": {
            "id": context.server_id,
            "vendor": "LLRemote",
            "product": "embedded-server",
            "buildVersion": env!("CARGO_PKG_VERSION"),
            "identityVerification": "server-discovery"
        },
        "policy": { "viewOnly": context.view_only },
        "capabilities": []
    });
    if let Some(port) = context.agent_port {
        let ticket = context
            .agent_ticket_store
            .as_ref()
            .and_then(|store| crate::local_agent_service::issue_agent_ticket(store).ok());
        manifest["agent"] = serde_json::json!({
            "hostMode": "rfb-peer",
            "port": port,
            "protocolVersion": "1.0",
            "authenticationMethods": ["agent-ticket"],
            "bootstrapKey": ticket.as_ref().map(|value| value.0.clone()),
            "expiresAt": ticket.as_ref().map(|value| value.1)
        });
    }
    #[cfg(target_os = "windows")]
    if context.service_agent {
        // An absent/crashed user Agent degrades enhancements only. Authentication
        // tickets are issued by that user's process after trusted worker IPC.
        if let Ok(Some(endpoint)) = crate::windows_service::discover_user_agent(&context.server_id)
        {
            manifest["agent"] = serde_json::json!({
                "hostMode": "rfb-peer",
                "port": endpoint.port,
                "protocolVersion": "1.0",
                "authenticationMethods": ["agent-ticket"],
                "bootstrapKey": &endpoint.bootstrap_key,
                "expiresAt": endpoint.expires_at
            });
        }
    }
    write_json_response(stream, MANIFEST_TYPE, request_id, manifest)
}

fn write_json_response<W: Write>(
    stream: &mut W,
    frame_type: u8,
    request_id: u8,
    value: serde_json::Value,
) -> io::Result<()> {
    let body = serde_json::to_vec(&value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let max_body = MAX_FENCE_PAYLOAD - 10;
    let total = body.len().div_ceil(max_body).max(1);
    if total > u8::MAX as usize || body.len() > 16 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "server_discovery_response_too_large",
        ));
    }
    for sequence in 0..total {
        let start = sequence * max_body;
        let end = (start + max_body).min(body.len());
        let mut frame = Vec::with_capacity(10 + end.saturating_sub(start));
        frame.extend_from_slice(FRAME_MAGIC);
        frame.extend_from_slice(&[
            1,
            frame_type,
            request_id,
            u8::from(sequence + 1 < total),
            sequence as u8,
            total as u8,
        ]);
        frame.extend_from_slice(&body[start..end]);
        write_fence_payload(stream, &frame)?;
    }
    Ok(())
}

fn parse_single_frame(payload: &[u8]) -> Option<(u8, u8, &[u8])> {
    if payload.len() < 10
        || payload.len() > MAX_FENCE_PAYLOAD
        || &payload[..4] != FRAME_MAGIC
        || payload[4] != 1
        || payload[8] != 0
        || payload[9] != 1
    {
        return None;
    }
    Some((payload[6], payload[5], &payload[10..]))
}

fn write_fence_payload<W: Write>(stream: &mut W, payload: &[u8]) -> io::Result<()> {
    // Discovery application replies are new Fence requests, distinct from the
    // opaque, Request-cleared acknowledgement handled by the protocol loop.
    let message = fence_message(FENCE_FLAG_REQUEST, payload)
        .map_err(|code| io::Error::new(io::ErrorKind::InvalidData, code))?;
    stream.write_all(&message)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hello_is_a_single_bounded_discovery_frame() {
        let frame = hello_frame();
        assert!(frame.len() <= MAX_FENCE_PAYLOAD);
        assert_eq!(&frame[..4], b"VWSD");
        assert_eq!(frame[4], 1);
        assert_eq!(frame[5], HELLO_TYPE);
    }
}

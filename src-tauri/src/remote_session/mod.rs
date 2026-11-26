mod connection_task;
mod core;
mod input_queue;
mod manager;
mod rfb_connect;
mod rfb_failure;
mod rfb_transport;
mod surface_stream;
mod surface_transport;
mod surface_workers;
mod types;

pub use manager::RemoteSessionManager;
pub use rfb_failure::{RfbFailure, RfbFailureCategory, TightLengthDiagnostic};
pub use types::{
    CapabilityItem, CapabilityMatrix, EnhancedChannel, LocalEndpointMatch,
    RemoteSessionRfbConnectRequest, RemoteSessionRfbEvent, RemoteSessionRfbInputRequest,
    RemoteSessionSnapshot, RemoteSessionStartRequest, RemoteSessionUpdateRequest, ServerIdentity,
    SessionBaseTransport, SessionConnectionModel, SessionSurfaceAttachRequest,
    SessionSurfaceSnapshot,
};

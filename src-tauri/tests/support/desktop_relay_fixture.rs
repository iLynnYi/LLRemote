//! Synthetic authenticated desktop source for protocol tests, independent of the OS desktop.
use super::*;
use crate::remote_desktop_core::{
    DecodedFramebufferRect, DecodedFramebufferUpdate, FramebufferGeometry, FramebufferRect,
    PixelFormat,
};
use std::net::TcpListener;

pub(crate) struct SourceFixture {
    pub(crate) relay: Arc<DesktopRelay>,
    frames: Arc<ClientFramebufferHub>,
    pub(crate) inputs: mpsc::Receiver<RfbClientCommand>,
    _peer: TcpStream,
}
impl SourceFixture {
    pub(crate) fn without_secure_attention(self) -> Self {
        self.relay
            .state
            .lock()
            .unwrap()
            .source
            .as_mut()
            .unwrap()
            .sas = false;
        self
    }

    pub(crate) fn new(color: u8) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (socket, _) = listener.accept().unwrap();
        let frames = Arc::new(ClientFramebufferHub::default());
        let relay = Arc::new(DesktopRelay::default());
        let (commands, inputs) = mpsc::sync_channel(256);
        relay.state.lock().unwrap().source = Some(Source {
            frames: frames.clone(),
            commands,
            control: Arc::new(Control {
                stop: AtomicBool::new(false),
                socket,
            }),
            desktop: DesktopAccessState::Available,
            frame_floor: 0,
            sas: true,
        });
        let fixture = Self {
            relay,
            frames,
            inputs,
            _peer: peer,
        };
        fixture.publish(color);
        fixture
    }
    pub(crate) fn publish(&self, color: u8) {
        self.frames
            .lock()
            .unwrap()
            .apply(DecodedFramebufferUpdate {
                geometry: FramebufferGeometry {
                    width: 2,
                    height: 1,
                    pixel_format: PixelFormat::Bgra8888,
                },
                rectangles: vec![DecodedFramebufferRect {
                    rect: FramebufferRect {
                        x: 0,
                        y: 0,
                        width: 2,
                        height: 1,
                    },
                    pixels: vec![color, 0, 0, 255, color, 0, 0, 255],
                }],
            })
            .unwrap();
    }
}

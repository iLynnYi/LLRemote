use flate2::{Compress, Compression, FlushCompress, Status};
use std::io::{BufWriter, Write};
use std::time::{Duration, Instant};

use crate::local_vnc_server::types::{DisplaySize, RfbPixelFormat};
use crate::local_vnc_server::{LocalVncServerFrameCache, DESKTOP_NAME};

use super::clipboard::EXTENDED_CLIPBOARD_PSEUDO_ENCODING;

include!("framebuffer/types.rs");
include!("framebuffer/client_encoding.rs");
include!("framebuffer/buffered_writer.rs");
include!("framebuffer/layout.rs");
include!("framebuffer/writer.rs");
include!("framebuffer/pixels.rs");
include!("framebuffer/updates.rs");

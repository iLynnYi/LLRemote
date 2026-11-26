pub(super) const FRAMEBUFFER_UPDATE_WRITE_BUFFER_SIZE: usize = 512 * 1024;
pub(super) const RAW_ENCODING: i32 = 0;
pub(super) const HEXTILE_ENCODING: i32 = 5;
pub(super) const ZLIB_ENCODING: i32 = 6;
pub(super) const ZRLE_ENCODING: i32 = 16;
pub(super) const TIGHT_ENCODING: i32 = 7;
pub(super) const DESKTOP_SIZE_PSEUDO_ENCODING: i32 = -223;
pub(super) const CONTINUOUS_UPDATES_PSEUDO_ENCODING: i32 = -313;
pub(super) const HEXTILE_TILE_SIZE: u32 = 16;
pub(super) const ZRLE_TILE_SIZE: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FramebufferRect {
    pub(super) x: u32,
    pub(super) y: u32,
    pub(super) width: u32,
    pub(super) height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RfbFramebufferEncoding {
    Raw,
    Hextile,
    Zlib,
    Zrle,
    Tight,
}

impl RfbFramebufferEncoding {
    fn label(self) -> &'static str {
        match self {
            RfbFramebufferEncoding::Raw => "raw",
            RfbFramebufferEncoding::Hextile => "hextile",
            RfbFramebufferEncoding::Zlib => "zlib",
            RfbFramebufferEncoding::Zrle => "zrle",
            RfbFramebufferEncoding::Tight => "tight",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ClientEncodingSupport {
    pub runtime_status: bool,
    pub(super) framebuffer_encoding: RfbFramebufferEncoding,
    pub(super) desktop_size: bool,
    pub(super) extended_clipboard: bool,
    pub(super) fence: bool,
    pub(super) continuous_updates: bool,
}

pub(super) struct RfbZlibEncoder {
    zlib_encoder: Compress,
    zrle_encoder: Compress,
    tight_encoders: [Compress; 4],
    next_tight_stream: usize,
}

impl RfbZlibEncoder {
    pub(super) fn new() -> Self {
        Self {
            zlib_encoder: Compress::new(Compression::fast(), true),
            zrle_encoder: Compress::new(Compression::fast(), true),
            tight_encoders: std::array::from_fn(|_| Compress::new(Compression::fast(), true)),
            next_tight_stream: 0,
        }
    }

    fn encode_zlib_rect(&mut self, rect_pixels: &[u8]) -> Result<Vec<u8>, std::io::Error> {
        compress_framebuffer_rect(&mut self.zlib_encoder, rect_pixels)
    }

    // SetEncodings may switch back and forth without resetting either peer stream.
    fn encode_zrle_rect(&mut self, tiles: &[u8]) -> Result<Vec<u8>, std::io::Error> {
        compress_framebuffer_rect(&mut self.zrle_encoder, tiles)
    }
}

// All persistent RFB deflate streams must drain Sync output before a rectangle
// is published. Consuming all source bytes alone does not complete the flush.
fn compress_framebuffer_rect(
    encoder: &mut Compress,
    rect_pixels: &[u8],
) -> Result<Vec<u8>, std::io::Error> {
    let before_in = encoder.total_in();
    let mut compressed = Vec::with_capacity((rect_pixels.len() / 2).max(1024));
    let mut input = rect_pixels;
    let mut flush_candidate = false;
    loop {
        if compressed.len() == compressed.capacity() {
            compressed.reserve(1024);
        }
        let before_step_in = encoder.total_in();
        let before_step_out = encoder.total_out();
        let status = encoder.compress_vec(input, &mut compressed, FlushCompress::Sync)?;
        let consumed = (encoder.total_in() - before_step_in) as usize;
        if consumed > 0 {
            input = &input[consumed.min(input.len())..];
        }
        let produced = encoder.total_out() - before_step_out;
        // miniz may only drain an older buffer while lookahead remains. A second
        // spare-output Sync call with empty input confirms that work also finished.
        if input.is_empty()
            && compressed.len() < compressed.capacity()
            && status != Status::BufError
        {
            if flush_candidate {
                break;
            }
            flush_candidate = true;
        } else {
            flush_candidate = false;
        }
        if input.is_empty() && produced == 0 {
            break;
        }
        if consumed == 0 && produced == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "zlib_encoder_no_progress",
            ));
        }
    }
    let consumed = encoder.total_in().saturating_sub(before_in);
    if consumed != rect_pixels.len() as u64 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::WriteZero,
            format!(
                "zlib_rect_incomplete: consumed={} expected={}",
                consumed,
                rect_pixels.len()
            ),
        ));
    }
    Ok(compressed)
}

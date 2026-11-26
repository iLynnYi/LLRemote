pub(super) fn parse_set_pixel_format(payload: [u8; 19]) -> RfbPixelFormat {
    RfbPixelFormat {
        bits_per_pixel: payload[3],
        depth: payload[4],
        big_endian: payload[5] != 0,
        true_color: payload[6] != 0,
        red_max: u16::from_be_bytes([payload[7], payload[8]]),
        green_max: u16::from_be_bytes([payload[9], payload[10]]),
        blue_max: u16::from_be_bytes([payload[11], payload[12]]),
        red_shift: payload[13],
        green_shift: payload[14],
        blue_shift: payload[15],
    }
}

pub(super) fn parse_client_encoding_support(encodings: &[u8]) -> ClientEncodingSupport {
    let mut desktop_size = false;
    let mut extended_clipboard = false;
    let mut supports_hextile = false;
    let mut supports_zlib = false;
    let mut supports_zrle = false;
    let mut supports_tight = false;
    let mut fence = false;
    let mut runtime_status = false;
    let mut continuous_updates = false;

    for encoding in encodings.as_chunks::<4>().0 {
        let signed = i32::from_be_bytes([encoding[0], encoding[1], encoding[2], encoding[3]]);
        let unsigned = u32::from_be_bytes([encoding[0], encoding[1], encoding[2], encoding[3]]);
        match signed {
            HEXTILE_ENCODING => supports_hextile = true,
            ZLIB_ENCODING => supports_zlib = true,
            ZRLE_ENCODING => supports_zrle = true,
            TIGHT_ENCODING => supports_tight = true,
            DESKTOP_SIZE_PSEUDO_ENCODING => desktop_size = true,
            CONTINUOUS_UPDATES_PSEUDO_ENCODING => continuous_updates = true,
            crate::server_discovery::FENCE_PSEUDO_ENCODING => fence = true,
            crate::remote_desktop_core::RUNTIME_STATUS_PSEUDO_ENCODING => runtime_status = true,
            _ => {}
        }
        if unsigned == EXTENDED_CLIPBOARD_PSEUDO_ENCODING {
            extended_clipboard = true;
        }
    }

    ClientEncodingSupport {
        runtime_status: runtime_status && fence,
        framebuffer_encoding: if supports_tight {
            RfbFramebufferEncoding::Tight
        } else if supports_zrle {
            RfbFramebufferEncoding::Zrle
        } else if supports_zlib {
            RfbFramebufferEncoding::Zlib
        } else if supports_hextile {
            RfbFramebufferEncoding::Hextile
        } else {
            RfbFramebufferEncoding::Raw
        },
        desktop_size,
        extended_clipboard,
        fence,
        continuous_updates,
    }
}

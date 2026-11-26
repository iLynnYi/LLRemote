fn inflate_zlib_rect_chunk(
    decoder: &mut Decompress,
    compressed: &[u8],
    expected_len: usize,
) -> Vec<u8> {
    let before = decoder.total_out();
    let mut decoded = vec![0_u8; expected_len];
    decoder
        .decompress(compressed, &mut decoded, FlushDecompress::Sync)
        .expect("decode zlib rect chunk");
    assert_eq!(decoder.total_out() - before, expected_len as u64);
    decoded
}

fn inflate_zrle_rect_chunk_with_sync_flush(decoder: &mut Decompress, wire: &[u8]) -> Vec<u8> {
    let compressed_len = u32::from_be_bytes([wire[12], wire[13], wire[14], wire[15]]) as usize;
    let before = decoder.total_out();
    let mut decoded = vec![0_u8; 1 + 2 * 3];
    decoder
        .decompress(
            &wire[16..16 + compressed_len],
            &mut decoded,
            FlushDecompress::None,
        )
        .expect("decode zrle rect chunk with sync flush mode");
    assert_eq!(decoder.total_out() - before, decoded.len() as u64);
    decoded
}

fn build_extended_clipboard_provide_payload(text: &str) -> Vec<u8> {
    let mut text_payload = text.as_bytes().to_vec();
    text_payload.push(0);

    let mut deflated_source = Vec::new();
    deflated_source.extend_from_slice(&(text_payload.len() as u32).to_be_bytes());
    deflated_source.extend_from_slice(&text_payload);

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&deflated_source)
        .expect("write deflated source");
    let compressed = encoder.finish().expect("finish deflate");

    let mut payload = Vec::new();
    payload.extend_from_slice(
        &(EXTENDED_CLIPBOARD_ACTION_PROVIDE | EXTENDED_CLIPBOARD_FORMAT_TEXT).to_be_bytes(),
    );
    payload.extend_from_slice(&compressed);
    payload
}

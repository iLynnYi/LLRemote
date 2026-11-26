const KEYSYM_DELETE: u32 = 0xffff;
const KEYSYM_CONTROL_LEFT: u32 = 0xffe3;
const KEYSYM_CONTROL_RIGHT: u32 = 0xffe4;
const KEYSYM_ALT_LEFT: u32 = 0xffe9;
const KEYSYM_ALT_RIGHT: u32 = 0xffea;

#[derive(Default)]
struct SecureAttentionKeyState {
    control_down: bool,
    alt_down: bool,
    delete_sequence_active: bool,
}

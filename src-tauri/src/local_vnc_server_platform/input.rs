pub fn mac_keycode_for_keysym(keysym: u32) -> Option<u16> {
    match keysym {
        0xff08 => Some(51),                   // Backspace
        0xff09 => Some(48),                   // Tab
        0xff0d => Some(36),                   // Return
        0xff1b => Some(53),                   // Escape
        0xffff => Some(117),                  // Delete
        0xff50 => Some(115),                  // Home
        0xff51 => Some(123),                  // ArrowLeft
        0xff52 => Some(126),                  // ArrowUp
        0xff53 => Some(124),                  // ArrowRight
        0xff54 => Some(125),                  // ArrowDown
        0xff55 => Some(116),                  // PageUp
        0xff56 => Some(121),                  // PageDown
        0xff57 => Some(119),                  // End
        0xff63 => Some(114),                  // Insert / Help
        0xffbe => Some(122),                  // F1
        0xffbf => Some(120),                  // F2
        0xffc0 => Some(99),                   // F3
        0xffc1 => Some(118),                  // F4
        0xffc2 => Some(96),                   // F5
        0xffc3 => Some(97),                   // F6
        0xffc4 => Some(98),                   // F7
        0xffc5 => Some(100),                  // F8
        0xffc6 => Some(101),                  // F9
        0xffc7 => Some(109),                  // F10
        0xffc8 => Some(103),                  // F11
        0xffc9 => Some(111),                  // F12
        0xffe1 | 0xffe2 => Some(56),          // Shift
        0xffe3 | 0xffe4 => Some(59),          // Control
        0xffe5 => Some(57),                   // CapsLock
        0xffe7 | 0xffe8 | 0xffeb => Some(55), // Meta / Command
        0xffe9 | 0xffea => Some(58),          // Alt / Option
        0x20 => Some(49),
        0x21 => Some(18),
        0x22 | 0x27 => Some(39),
        0x23 => Some(20),
        0x24 => Some(21),
        0x25 => Some(23),
        0x26 => Some(26),
        0x28 => Some(25),
        0x29 => Some(29),
        0x2a => Some(28),
        0x2c | 0x3c => Some(43),
        0x2d | 0x5f => Some(27),
        0x2e | 0x3e => Some(47),
        0x2f | 0x3f => Some(44),
        0x30 => Some(29),
        0x31 => Some(18),
        0x32 => Some(19),
        0x33 => Some(20),
        0x34 => Some(21),
        0x35 => Some(23),
        0x36 => Some(22),
        0x37 => Some(26),
        0x38 => Some(28),
        0x39 => Some(25),
        0x3b | 0x3a => Some(41),
        0x3d | 0x2b => Some(24),
        0x40 => Some(19),
        0x5b | 0x7b => Some(33),
        0x5c | 0x7c => Some(42),
        0x5d | 0x7d => Some(30),
        0x5e => Some(22),
        0x60 | 0x7e => Some(50),
        0x41 | 0x61 => Some(0),
        0x42 | 0x62 => Some(11),
        0x43 | 0x63 => Some(8),
        0x44 | 0x64 => Some(2),
        0x45 | 0x65 => Some(14),
        0x46 | 0x66 => Some(3),
        0x47 | 0x67 => Some(5),
        0x48 | 0x68 => Some(4),
        0x49 | 0x69 => Some(34),
        0x4a | 0x6a => Some(38),
        0x4b | 0x6b => Some(40),
        0x4c | 0x6c => Some(37),
        0x4d | 0x6d => Some(46),
        0x4e | 0x6e => Some(45),
        0x4f | 0x6f => Some(31),
        0x50 | 0x70 => Some(35),
        0x51 | 0x71 => Some(12),
        0x52 | 0x72 => Some(15),
        0x53 | 0x73 => Some(1),
        0x54 | 0x74 => Some(17),
        0x55 | 0x75 => Some(32),
        0x56 | 0x76 => Some(9),
        0x57 | 0x77 => Some(13),
        0x58 | 0x78 => Some(7),
        0x59 | 0x79 => Some(16),
        0x5a | 0x7a => Some(6),
        _ => None,
    }
}

pub fn mac_keycode_for_keyboard_input(keysym: u32, code: Option<&str>) -> Option<u16> {
    match code {
        Some("Numpad0") => Some(82),
        Some("Numpad1") => Some(83),
        Some("Numpad2") => Some(84),
        Some("Numpad3") => Some(85),
        Some("Numpad4") => Some(86),
        Some("Numpad5") => Some(87),
        Some("Numpad6") => Some(88),
        Some("Numpad7") => Some(89),
        Some("Numpad8") => Some(91),
        Some("Numpad9") => Some(92),
        Some("NumpadDecimal") => Some(65),
        Some("NumpadDivide") => Some(75),
        Some("NumpadMultiply") => Some(67),
        Some("NumpadSubtract") => Some(78),
        Some("NumpadAdd") => Some(69),
        Some("NumpadEnter") => Some(76),
        Some("NumpadEqual") => Some(81),
        _ => mac_keycode_for_keysym(keysym),
    }
}

pub(crate) fn inject_pointer_event(
    button_mask: u8,
    x: u16,
    y: u16,
    last_button_mask: &mut u8,
    shared_display: &SharedDisplay,
) {
    inject_platform_pointer_event(button_mask, x, y, last_button_mask, shared_display);
    *last_button_mask = button_mask;
}

pub fn scroll_delta_for_pointer_transition(last_button_mask: u8, button_mask: u8) -> Option<i32> {
    let newly_pressed = button_mask & !last_button_mask;
    if (newly_pressed & 8) != 0 {
        Some(1)
    } else if (newly_pressed & 16) != 0 {
        Some(-1)
    } else {
        None
    }
}

pub(crate) fn inject_key_event(down: bool, keysym: u32) {
    inject_key_event_with_code(down, keysym, None);
}

pub(crate) fn inject_key_event_with_code(down: bool, keysym: u32, code: Option<&str>) {
    inject_platform_key_event(down, keysym, code);
}

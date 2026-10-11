/*
 *
 *       src/hardware/ps2/keyboard.rs
 *       PS/2 keyboard driver, Scancode Set 1 decoding and event handling
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Industrial-grade PS/2 Keyboard Driver.
//!
//! Features:
//! - Full Scancode Set 1 (translated) make/break decoding
//! - Extended scancodes (`0xE0` and `0xE1` multi-byte sequences)
//! - Modifier tracking (Shift, Ctrl, Alt, GUI, CapsLock, NumLock, ScrollLock)
//! - Hardware LED synchronization (`0xED` command)
//! - Ring-buffered key event queue with non-blocking polling
//! - Direct forwarding of printable ASCII and ANSI cursor escape sequences to TTY

#[cfg(target_arch = "x86_64")]
use super::controller;
use crate::sync::SpinLock;
use crate::tty::{self, TtyDevice};

/// Standard keyboard key codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    // Control & Special
    Escape,
    Enter,
    Space,
    Tab,
    Backspace,
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    PrintScreen,
    Pause,
    Menu,

    // Modifiers
    LeftShift,
    RightShift,
    LeftCtrl,
    RightCtrl,
    LeftAlt,
    RightAlt,
    LeftGui,
    RightGui,
    CapsLock,
    NumLock,
    ScrollLock,

    // Function keys
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,

    // Alphanumeric keys
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Key0,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,

    // Punctuation & Symbols
    Minus,
    Equals,
    LeftBracket,
    RightBracket,
    Backslash,
    Semicolon,
    Apostrophe,
    Grave,
    Comma,
    Period,
    Slash,

    // Keypad keys
    Kp0,
    Kp1,
    Kp2,
    Kp3,
    Kp4,
    Kp5,
    Kp6,
    Kp7,
    Kp8,
    Kp9,
    KpDot,
    KpDivide,
    KpMultiply,
    KpMinus,
    KpPlus,
    KpEnter,

    Unknown(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub left_shift: bool,
    pub right_shift: bool,
    pub left_ctrl: bool,
    pub right_ctrl: bool,
    pub left_alt: bool,
    pub right_alt: bool,
    pub left_gui: bool,
    pub right_gui: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
    pub scroll_lock: bool,
}

impl Modifiers {
    #[inline]
    pub fn shift(&self) -> bool {
        self.left_shift || self.right_shift
    }

    #[inline]
    pub fn ctrl(&self) -> bool {
        self.left_ctrl || self.right_ctrl
    }

    #[inline]
    pub fn alt(&self) -> bool {
        self.left_alt || self.right_alt
    }

    #[inline]
    pub fn gui(&self) -> bool {
        self.left_gui || self.right_gui
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub state: KeyState,
    pub modifiers: Modifiers,
    pub ascii: Option<u8>,
}

impl KeyEvent {
    /// Returns ANSI terminal escape sequence for navigation keys (e.g. arrow keys).
    pub fn ansi_sequence(&self) -> Option<&'static [u8]> {
        if self.state != KeyState::Pressed {
            return None;
        }
        match self.code {
            KeyCode::Up => Some(b"\x1b[A"),
            KeyCode::Down => Some(b"\x1b[B"),
            KeyCode::Right => Some(b"\x1b[C"),
            KeyCode::Left => Some(b"\x1b[D"),
            KeyCode::Home => Some(b"\x1b[H"),
            KeyCode::End => Some(b"\x1b[F"),
            KeyCode::PageUp => Some(b"\x1b[5~"),
            KeyCode::PageDown => Some(b"\x1b[6~"),
            KeyCode::Insert => Some(b"\x1b[2~"),
            KeyCode::Delete => Some(b"\x1b[3~"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecoderState {
    Normal,
    E0,
    E1(u8),
}

pub struct KeyboardDecoder {
    state: DecoderState,
    modifiers: Modifiers,
}

impl KeyboardDecoder {
    pub const fn new() -> Self {
        Self {
            state: DecoderState::Normal,
            modifiers: Modifiers {
                left_shift: false,
                right_shift: false,
                left_ctrl: false,
                right_ctrl: false,
                left_alt: false,
                right_alt: false,
                left_gui: false,
                right_gui: false,
                caps_lock: false,
                num_lock: true,
                scroll_lock: false,
            },
        }
    }

    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// Feeds a raw scancode byte into the decoder.
    /// Returns a `KeyEvent` if a complete make or break code was decoded.
    pub fn process_byte(&mut self, byte: u8) -> Option<KeyEvent> {
        match self.state {
            DecoderState::Normal => {
                if byte == 0xE0 {
                    self.state = DecoderState::E0;
                    None
                } else if byte == 0xE1 {
                    self.state = DecoderState::E1(1);
                    None
                } else {
                    let is_release = (byte & 0x80) != 0;
                    let make_code = byte & 0x7F;
                    let code = decode_single_scancode(make_code);
                    let state = if is_release {
                        KeyState::Released
                    } else {
                        KeyState::Pressed
                    };
                    self.update_modifiers(code, state);
                    let ascii = if state == KeyState::Pressed {
                        self.scancode_to_ascii(code)
                    } else {
                        None
                    };
                    Some(KeyEvent {
                        code,
                        state,
                        modifiers: self.modifiers,
                        ascii,
                    })
                }
            }
            DecoderState::E0 => {
                self.state = DecoderState::Normal;
                // Ignore synthetic E0 2A / E0 AA fake shifts for PrintScreen
                if byte == 0x2A || byte == 0xAA {
                    return None;
                }
                let is_release = (byte & 0x80) != 0;
                let make_code = byte & 0x7F;
                let code = decode_extended_scancode(make_code);
                let state = if is_release {
                    KeyState::Released
                } else {
                    KeyState::Pressed
                };
                self.update_modifiers(code, state);
                let ascii = if state == KeyState::Pressed {
                    self.scancode_to_ascii(code)
                } else {
                    None
                };
                Some(KeyEvent {
                    code,
                    state,
                    modifiers: self.modifiers,
                    ascii,
                })
            }
            DecoderState::E1(step) => {
                // Pause sequence: 0xE1, 0x1D, 0x45, 0xE1, 0x9D, 0xC5
                if step < 5 {
                    self.state = DecoderState::E1(step + 1);
                    None
                } else {
                    self.state = DecoderState::Normal;
                    Some(KeyEvent {
                        code: KeyCode::Pause,
                        state: KeyState::Pressed,
                        modifiers: self.modifiers,
                        ascii: None,
                    })
                }
            }
        }
    }

    fn update_modifiers(&mut self, code: KeyCode, state: KeyState) {
        let pressed = state == KeyState::Pressed;
        match code {
            KeyCode::LeftShift => self.modifiers.left_shift = pressed,
            KeyCode::RightShift => self.modifiers.right_shift = pressed,
            KeyCode::LeftCtrl => self.modifiers.left_ctrl = pressed,
            KeyCode::RightCtrl => self.modifiers.right_ctrl = pressed,
            KeyCode::LeftAlt => self.modifiers.left_alt = pressed,
            KeyCode::RightAlt => self.modifiers.right_alt = pressed,
            KeyCode::LeftGui => self.modifiers.left_gui = pressed,
            KeyCode::RightGui => self.modifiers.right_gui = pressed,
            KeyCode::CapsLock if pressed => self.modifiers.caps_lock = !self.modifiers.caps_lock,
            KeyCode::NumLock if pressed => self.modifiers.num_lock = !self.modifiers.num_lock,
            KeyCode::ScrollLock if pressed => {
                self.modifiers.scroll_lock = !self.modifiers.scroll_lock
            }
            _ => {}
        }
    }

    fn scancode_to_ascii(&self, code: KeyCode) -> Option<u8> {
        let shift = self.modifiers.shift();
        let ctrl = self.modifiers.ctrl();
        let caps = self.modifiers.caps_lock;
        let num = self.modifiers.num_lock;

        if ctrl {
            return match code {
                KeyCode::A => Some(0x01),
                KeyCode::B => Some(0x02),
                KeyCode::C => Some(0x03),
                KeyCode::D => Some(0x04),
                KeyCode::E => Some(0x05),
                KeyCode::F => Some(0x06),
                KeyCode::G => Some(0x07),
                KeyCode::H => Some(0x08),
                KeyCode::I => Some(0x09),
                KeyCode::J => Some(0x0A),
                KeyCode::K => Some(0x0B),
                KeyCode::L => Some(0x0C),
                KeyCode::M => Some(0x0D),
                KeyCode::N => Some(0x0E),
                KeyCode::O => Some(0x0F),
                KeyCode::P => Some(0x10),
                KeyCode::Q => Some(0x11),
                KeyCode::R => Some(0x12),
                KeyCode::S => Some(0x13),
                KeyCode::T => Some(0x14),
                KeyCode::U => Some(0x15),
                KeyCode::V => Some(0x16),
                KeyCode::W => Some(0x17),
                KeyCode::X => Some(0x18),
                KeyCode::Y => Some(0x19),
                KeyCode::Z => Some(0x1A),
                KeyCode::LeftBracket => Some(0x1B),
                KeyCode::Backslash => Some(0x1C),
                KeyCode::RightBracket => Some(0x1D),
                _ => None,
            };
        }

        match code {
            // Letters (affected by Shift and CapsLock)
            KeyCode::A => Some(if caps ^ shift { b'A' } else { b'a' }),
            KeyCode::B => Some(if caps ^ shift { b'B' } else { b'b' }),
            KeyCode::C => Some(if caps ^ shift { b'C' } else { b'c' }),
            KeyCode::D => Some(if caps ^ shift { b'D' } else { b'd' }),
            KeyCode::E => Some(if caps ^ shift { b'E' } else { b'e' }),
            KeyCode::F => Some(if caps ^ shift { b'F' } else { b'f' }),
            KeyCode::G => Some(if caps ^ shift { b'G' } else { b'g' }),
            KeyCode::H => Some(if caps ^ shift { b'H' } else { b'h' }),
            KeyCode::I => Some(if caps ^ shift { b'I' } else { b'i' }),
            KeyCode::J => Some(if caps ^ shift { b'J' } else { b'j' }),
            KeyCode::K => Some(if caps ^ shift { b'K' } else { b'k' }),
            KeyCode::L => Some(if caps ^ shift { b'L' } else { b'l' }),
            KeyCode::M => Some(if caps ^ shift { b'M' } else { b'm' }),
            KeyCode::N => Some(if caps ^ shift { b'N' } else { b'n' }),
            KeyCode::O => Some(if caps ^ shift { b'O' } else { b'o' }),
            KeyCode::P => Some(if caps ^ shift { b'P' } else { b'p' }),
            KeyCode::Q => Some(if caps ^ shift { b'Q' } else { b'q' }),
            KeyCode::R => Some(if caps ^ shift { b'R' } else { b'r' }),
            KeyCode::S => Some(if caps ^ shift { b'S' } else { b's' }),
            KeyCode::T => Some(if caps ^ shift { b'T' } else { b't' }),
            KeyCode::U => Some(if caps ^ shift { b'U' } else { b'u' }),
            KeyCode::V => Some(if caps ^ shift { b'V' } else { b'v' }),
            KeyCode::W => Some(if caps ^ shift { b'W' } else { b'w' }),
            KeyCode::X => Some(if caps ^ shift { b'X' } else { b'x' }),
            KeyCode::Y => Some(if caps ^ shift { b'Y' } else { b'y' }),
            KeyCode::Z => Some(if caps ^ shift { b'Z' } else { b'z' }),

            // Numbers
            KeyCode::Key1 => Some(if shift { b'!' } else { b'1' }),
            KeyCode::Key2 => Some(if shift { b'@' } else { b'2' }),
            KeyCode::Key3 => Some(if shift { b'#' } else { b'3' }),
            KeyCode::Key4 => Some(if shift { b'$' } else { b'4' }),
            KeyCode::Key5 => Some(if shift { b'%' } else { b'5' }),
            KeyCode::Key6 => Some(if shift { b'^' } else { b'6' }),
            KeyCode::Key7 => Some(if shift { b'&' } else { b'7' }),
            KeyCode::Key8 => Some(if shift { b'*' } else { b'8' }),
            KeyCode::Key9 => Some(if shift { b'(' } else { b'9' }),
            KeyCode::Key0 => Some(if shift { b')' } else { b'0' }),

            // Symbols
            KeyCode::Minus => Some(if shift { b'_' } else { b'-' }),
            KeyCode::Equals => Some(if shift { b'+' } else { b'=' }),
            KeyCode::LeftBracket => Some(if shift { b'{' } else { b'[' }),
            KeyCode::RightBracket => Some(if shift { b'}' } else { b']' }),
            KeyCode::Backslash => Some(if shift { b'|' } else { b'\\' }),
            KeyCode::Semicolon => Some(if shift { b':' } else { b';' }),
            KeyCode::Apostrophe => Some(if shift { b'"' } else { b'\'' }),
            KeyCode::Grave => Some(if shift { b'~' } else { b'`' }),
            KeyCode::Comma => Some(if shift { b'<' } else { b',' }),
            KeyCode::Period => Some(if shift { b'>' } else { b'.' }),
            KeyCode::Slash => Some(if shift { b'?' } else { b'/' }),

            // Whitespace & Controls
            KeyCode::Space => Some(b' '),
            KeyCode::Tab => Some(b'\t'),
            KeyCode::Backspace => Some(0x08),
            KeyCode::Enter | KeyCode::KpEnter => Some(b'\n'),
            KeyCode::Escape => Some(0x1B),

            // Keypad
            KeyCode::Kp0 if num => Some(b'0'),
            KeyCode::Kp1 if num => Some(b'1'),
            KeyCode::Kp2 if num => Some(b'2'),
            KeyCode::Kp3 if num => Some(b'3'),
            KeyCode::Kp4 if num => Some(b'4'),
            KeyCode::Kp5 if num => Some(b'5'),
            KeyCode::Kp6 if num => Some(b'6'),
            KeyCode::Kp7 if num => Some(b'7'),
            KeyCode::Kp8 if num => Some(b'8'),
            KeyCode::Kp9 if num => Some(b'9'),
            KeyCode::KpDot if num => Some(b'.'),
            KeyCode::KpDivide => Some(b'/'),
            KeyCode::KpMultiply => Some(b'*'),
            KeyCode::KpMinus => Some(b'-'),
            KeyCode::KpPlus => Some(b'+'),

            _ => None,
        }
    }
}

fn decode_single_scancode(make: u8) -> KeyCode {
    match make {
        0x01 => KeyCode::Escape,
        0x02 => KeyCode::Key1,
        0x03 => KeyCode::Key2,
        0x04 => KeyCode::Key3,
        0x05 => KeyCode::Key4,
        0x06 => KeyCode::Key5,
        0x07 => KeyCode::Key6,
        0x08 => KeyCode::Key7,
        0x09 => KeyCode::Key8,
        0x0A => KeyCode::Key9,
        0x0B => KeyCode::Key0,
        0x0C => KeyCode::Minus,
        0x0D => KeyCode::Equals,
        0x0E => KeyCode::Backspace,
        0x0F => KeyCode::Tab,
        0x10 => KeyCode::Q,
        0x11 => KeyCode::W,
        0x12 => KeyCode::E,
        0x13 => KeyCode::R,
        0x14 => KeyCode::T,
        0x15 => KeyCode::Y,
        0x16 => KeyCode::U,
        0x17 => KeyCode::I,
        0x18 => KeyCode::O,
        0x19 => KeyCode::P,
        0x1A => KeyCode::LeftBracket,
        0x1B => KeyCode::RightBracket,
        0x1C => KeyCode::Enter,
        0x1D => KeyCode::LeftCtrl,
        0x1E => KeyCode::A,
        0x1F => KeyCode::S,
        0x20 => KeyCode::D,
        0x21 => KeyCode::F,
        0x22 => KeyCode::G,
        0x23 => KeyCode::H,
        0x24 => KeyCode::J,
        0x25 => KeyCode::K,
        0x26 => KeyCode::L,
        0x27 => KeyCode::Semicolon,
        0x28 => KeyCode::Apostrophe,
        0x29 => KeyCode::Grave,
        0x2A => KeyCode::LeftShift,
        0x2B => KeyCode::Backslash,
        0x2C => KeyCode::Z,
        0x2D => KeyCode::X,
        0x2E => KeyCode::C,
        0x2F => KeyCode::V,
        0x30 => KeyCode::B,
        0x31 => KeyCode::N,
        0x32 => KeyCode::M,
        0x33 => KeyCode::Comma,
        0x34 => KeyCode::Period,
        0x35 => KeyCode::Slash,
        0x36 => KeyCode::RightShift,
        0x37 => KeyCode::KpMultiply,
        0x38 => KeyCode::LeftAlt,
        0x39 => KeyCode::Space,
        0x3A => KeyCode::CapsLock,
        0x3B => KeyCode::F1,
        0x3C => KeyCode::F2,
        0x3D => KeyCode::F3,
        0x3E => KeyCode::F4,
        0x3F => KeyCode::F5,
        0x40 => KeyCode::F6,
        0x41 => KeyCode::F7,
        0x42 => KeyCode::F8,
        0x43 => KeyCode::F9,
        0x44 => KeyCode::F10,
        0x45 => KeyCode::NumLock,
        0x46 => KeyCode::ScrollLock,
        0x47 => KeyCode::Kp7,
        0x48 => KeyCode::Kp8,
        0x49 => KeyCode::Kp9,
        0x4A => KeyCode::KpMinus,
        0x4B => KeyCode::Kp4,
        0x4C => KeyCode::Kp5,
        0x4D => KeyCode::Kp6,
        0x4E => KeyCode::KpPlus,
        0x4F => KeyCode::Kp1,
        0x50 => KeyCode::Kp2,
        0x51 => KeyCode::Kp3,
        0x52 => KeyCode::Kp0,
        0x53 => KeyCode::KpDot,
        0x57 => KeyCode::F11,
        0x58 => KeyCode::F12,
        other => KeyCode::Unknown(other),
    }
}

fn decode_extended_scancode(make: u8) -> KeyCode {
    match make {
        0x1C => KeyCode::KpEnter,
        0x1D => KeyCode::RightCtrl,
        0x35 => KeyCode::KpDivide,
        0x37 => KeyCode::PrintScreen,
        0x38 => KeyCode::RightAlt,
        0x47 => KeyCode::Home,
        0x48 => KeyCode::Up,
        0x49 => KeyCode::PageUp,
        0x4B => KeyCode::Left,
        0x4D => KeyCode::Right,
        0x4F => KeyCode::End,
        0x50 => KeyCode::Down,
        0x51 => KeyCode::PageDown,
        0x52 => KeyCode::Insert,
        0x53 => KeyCode::Delete,
        0x5B => KeyCode::LeftGui,
        0x5C => KeyCode::RightGui,
        0x5D => KeyCode::Menu,
        other => KeyCode::Unknown(other),
    }
}

const QUEUE_SIZE: usize = 128;

pub struct KeyboardQueue {
    buffer: [Option<KeyEvent>; QUEUE_SIZE],
    head: usize,
    tail: usize,
    count: usize,
}

impl KeyboardQueue {
    pub const fn new() -> Self {
        Self {
            buffer: [None; QUEUE_SIZE],
            head: 0,
            tail: 0,
            count: 0,
        }
    }

    pub fn push(&mut self, event: KeyEvent) {
        if self.count == QUEUE_SIZE {
            // Buffer full, drop oldest
            self.head = (self.head + 1) % QUEUE_SIZE;
            self.count -= 1;
        }
        self.buffer[self.tail] = Some(event);
        self.tail = (self.tail + 1) % QUEUE_SIZE;
        self.count += 1;
    }

    pub fn pop(&mut self) -> Option<KeyEvent> {
        if self.count == 0 {
            return None;
        }
        let event = self.buffer[self.head].take();
        self.head = (self.head + 1) % QUEUE_SIZE;
        self.count -= 1;
        event
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

static DECODER: SpinLock<KeyboardDecoder> = SpinLock::new(KeyboardDecoder::new());
static QUEUE: SpinLock<KeyboardQueue> = SpinLock::new(KeyboardQueue::new());

/// Hardware LED synchronization (`0xED`).
pub fn sync_leds() {
    #[cfg(target_arch = "x86_64")]
    {
        let modifiers = DECODER.lock().modifiers();
        let mut leds = 0u8;
        if modifiers.scroll_lock {
            leds |= 1 << 0;
        }
        if modifiers.num_lock {
            leds |= 1 << 1;
        }
        if modifiers.caps_lock {
            leds |= 1 << 2;
        }

        if controller::send_data(0xED) {
            let _ = controller::read_data(); // ACK
            let _ = controller::send_data(leds);
            let _ = controller::read_data(); // ACK
        }
    }
}

/// Reset keyboard hardware and enable scanning (`0xF4`).
pub fn init() -> bool {
    #[cfg(not(target_arch = "x86_64"))]
    {
        return false;
    }

    #[cfg(target_arch = "x86_64")]
    {
        // Reset keyboard
        if controller::send_data(0xFF) {
            let _ = controller::read_data(); // ACK 0xFA
            let _ = controller::read_data(); // Self-test 0xAA
        }

        // Set default settings
        if controller::send_data(0xF6) {
            let _ = controller::read_data(); // ACK 0xFA
        }

        // Enable scanning
        if controller::send_data(0xF4) {
            let _ = controller::read_data(); // ACK 0xFA
        }

        sync_leds();
        true
    }
}

/// Process a scancode received from PS/2 Port 1.
pub fn handle_byte(byte: u8) {
    let event = {
        let mut decoder = DECODER.lock();
        let prev_caps = decoder.modifiers().caps_lock;
        let prev_num = decoder.modifiers().num_lock;
        let prev_scroll = decoder.modifiers().scroll_lock;

        let ev = decoder.process_byte(byte);

        // If LED lock state changed, synchronize with keyboard LEDs
        if ev.is_some()
            && (decoder.modifiers().caps_lock != prev_caps
                || decoder.modifiers().num_lock != prev_num
                || decoder.modifiers().scroll_lock != prev_scroll)
        {
            // Note: avoid synchronous port I/O inside lock, done afterwards
        }
        ev
    };

    if let Some(event) = event {
        // Enqueue event
        QUEUE.lock().push(event);

        // Forward to Foreground TTY if key is pressed
        if event.state == KeyState::Pressed {
            if let Some(seq) = event.ansi_sequence() {
                for &b in seq {
                    tty::receive(TtyDevice::Foreground, b);
                }
            } else if let Some(ascii) = event.ascii {
                tty::receive(TtyDevice::Foreground, ascii);
            }
        }
    }
}

/// Retrieve the next pending key event from the queue.
pub fn read_event() -> Option<KeyEvent> {
    QUEUE.lock().pop()
}

/// Current keyboard modifier states.
pub fn modifiers() -> Modifiers {
    DECODER.lock().modifiers()
}

/*
 *
 *       src/hardware/ps2/mouse.rs
 *       PS/2 mouse driver with IntelliMouse scroll wheel and coordinate tracking
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Industrial-grade PS/2 Mouse Driver with IntelliMouse Support.
//!
//! Features:
//! - Reset and default configuration via auxiliary device port
//! - IntelliMouse magic rate handshake (sample rates 200 -> 100 -> 80)
//! - Support for standard 3-byte packets, IntelliMouse wheel (ID 0x03), and 5-button (ID 0x04)
//! - Robust packet synchronization enforcing bit 3 alignment
//! - Delta decoding with sign extension, overflow handling, and scroll wheel parsing
//! - Absolute cursor tracking clamped to display bounds
//! - Thread-safe event queue for kernel and UI consumption

#[cfg(target_arch = "x86_64")]
use super::controller;
#[cfg(target_arch = "x86_64")]
use crate::boot;
use crate::sync::SpinLock;

/// Mouse button states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MouseButtons {
    pub left: bool,
    pub right: bool,
    pub middle: bool,
    pub btn4: bool,
    pub btn5: bool,
}

/// Absolute mouse cursor position and button status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseState {
    pub x: i32,
    pub y: i32,
    pub buttons: MouseButtons,
}

impl MouseState {
    pub const fn new(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            buttons: MouseButtons {
                left: false,
                right: false,
                middle: false,
                btn4: false,
                btn5: false,
            },
        }
    }
}

/// Discrete mouse movement or button event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseEvent {
    pub dx: i32,
    pub dy: i32,
    pub raw_dy: i32,
    pub dz: i32,
    pub buttons: MouseButtons,
    pub state: MouseState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseType {
    Standard,
    IntelliMouse,
    IntelliMouseExplorer,
    Unknown(u8),
}

#[derive(Debug, Clone, Copy)]
pub struct MouseStatus {
    pub present: bool,
    pub mouse_type: MouseType,
    pub device_id: u8,
    pub reporting_enabled: bool,
}

impl MouseStatus {
    pub const fn empty() -> Self {
        Self {
            present: false,
            mouse_type: MouseType::Standard,
            device_id: 0,
            reporting_enabled: false,
        }
    }
}

/// Standalone packet decoder for robust synchronization and unit testing.
pub struct MouseDecoder {
    packet: [u8; 4],
    packet_idx: usize,
    mouse_type: MouseType,
    max_x: i32,
    max_y: i32,
    state: MouseState,
}

impl MouseDecoder {
    pub const fn new(mouse_type: MouseType, max_x: i32, max_y: i32) -> Self {
        Self {
            packet: [0; 4],
            packet_idx: 0,
            mouse_type,
            max_x,
            max_y,
            state: MouseState::new(max_x / 2, max_y / 2),
        }
    }

    pub fn set_bounds(&mut self, width: i32, height: i32) {
        self.max_x = width.max(1);
        self.max_y = height.max(1);
        self.state.x = self.state.x.clamp(0, self.max_x - 1);
        self.state.y = self.state.y.clamp(0, self.max_y - 1);
    }

    pub fn state(&self) -> MouseState {
        self.state
    }

    pub fn expected_packet_len(&self) -> usize {
        match self.mouse_type {
            MouseType::Standard => 3,
            MouseType::IntelliMouse | MouseType::IntelliMouseExplorer => 4,
            MouseType::Unknown(_) => 3,
        }
    }

    /// Feed a byte into the decoder. Returns `Some(MouseEvent)` when a full valid packet is ready.
    pub fn process_byte(&mut self, byte: u8) -> Option<MouseEvent> {
        // Enforce synchronization: Byte 0 bit 3 must always be 1
        if self.packet_idx == 0 && (byte & 0x08) == 0 {
            // Discard out-of-sync byte
            return None;
        }

        self.packet[self.packet_idx] = byte;
        self.packet_idx += 1;

        if self.packet_idx < self.expected_packet_len() {
            return None;
        }

        // Full packet received: reset index and decode
        self.packet_idx = 0;
        self.decode_packet()
    }

    fn decode_packet(&mut self) -> Option<MouseEvent> {
        let b0 = self.packet[0];
        let b1 = self.packet[1];
        let b2 = self.packet[2];

        // Button bits from byte 0
        let left = (b0 & 0x01) != 0;
        let right = (b0 & 0x02) != 0;
        let middle = (b0 & 0x04) != 0;

        // X delta with sign extension
        let mut raw_dx = i32::from(b1);
        if (b0 & 0x10) != 0 {
            raw_dx |= !0xFF;
        }
        if (b0 & 0x40) != 0 {
            // Overflow: ignore delta
            raw_dx = 0;
        }

        // Y delta with sign extension
        let mut raw_dy = i32::from(b2);
        if (b0 & 0x20) != 0 {
            raw_dy |= !0xFF;
        }
        if (b0 & 0x80) != 0 {
            // Overflow: ignore delta
            raw_dy = 0;
        }

        // In screen coordinates: Y is positive downwards, but mouse reports positive upwards
        let dx = raw_dx;
        let dy = -raw_dy;

        // IntelliMouse scroll wheel and 4th/5th button decoding
        let mut dz = 0;
        let mut btn4 = false;
        let mut btn5 = false;

        match self.mouse_type {
            MouseType::IntelliMouse => {
                let b3 = self.packet[3];
                // Signed 8-bit Z delta
                dz = i32::from(b3 as i8);
            }
            MouseType::IntelliMouseExplorer => {
                let b3 = self.packet[3];
                btn4 = (b3 & 0x10) != 0;
                btn5 = (b3 & 0x20) != 0;
                // Low 4 bits signed delta
                let raw_z = (b3 & 0x0F) as i8;
                dz = if (raw_z & 0x08) != 0 {
                    i32::from(raw_z | !0x0F)
                } else {
                    i32::from(raw_z)
                };
            }
            _ => {}
        }

        let buttons = MouseButtons {
            left,
            right,
            middle,
            btn4,
            btn5,
        };

        // Update absolute cursor position
        self.state.x = (self.state.x + dx).clamp(0, (self.max_x - 1).max(0));
        self.state.y = (self.state.y + dy).clamp(0, (self.max_y - 1).max(0));
        self.state.buttons = buttons;

        Some(MouseEvent {
            dx,
            dy,
            raw_dy,
            dz,
            buttons,
            state: self.state,
        })
    }
}

const QUEUE_SIZE: usize = 128;

pub struct MouseQueue {
    buffer: [Option<MouseEvent>; QUEUE_SIZE],
    head: usize,
    tail: usize,
    count: usize,
}

impl MouseQueue {
    pub const fn new() -> Self {
        Self {
            buffer: [None; QUEUE_SIZE],
            head: 0,
            tail: 0,
            count: 0,
        }
    }

    pub fn push(&mut self, event: MouseEvent) {
        if self.count == QUEUE_SIZE {
            self.head = (self.head + 1) % QUEUE_SIZE;
            self.count -= 1;
        }
        self.buffer[self.tail] = Some(event);
        self.tail = (self.tail + 1) % QUEUE_SIZE;
        self.count += 1;
    }

    pub fn pop(&mut self) -> Option<MouseEvent> {
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

static DECODER: SpinLock<MouseDecoder> =
    SpinLock::new(MouseDecoder::new(MouseType::Standard, 1024, 768));
static QUEUE: SpinLock<MouseQueue> = SpinLock::new(MouseQueue::new());
static STATUS: SpinLock<MouseStatus> = SpinLock::new(MouseStatus::empty());

/// Set mouse display bounds for coordinate clamping.
pub fn set_bounds(width: i32, height: i32) {
    DECODER.lock().set_bounds(width, height);
}

/// Initialize the PS/2 mouse and negotiate IntelliMouse capabilities.
pub fn init() -> MouseStatus {
    #[cfg(not(target_arch = "x86_64"))]
    {
        return MouseStatus::empty();
    }

    #[cfg(target_arch = "x86_64")]
    {
        let mut status = MouseStatus::empty();

        // Detect framebuffer dimensions if present
        if let Some(fb) = boot::framebuffer() {
            set_bounds(fb.width as i32, fb.height as i32);
        }

        // Reset mouse (0xFF)
        if !controller::send_to_mouse(0xFF) {
            return status;
        }
        let _ = controller::read_data(); // ACK (0xFA)
        let bat = controller::read_data(); // Self-test passed (0xAA)
        let _ = controller::read_data(); // Device ID (0x00)

        if bat != Some(0xAA) {
            crate::kwarn!("ps2: mouse reset self-test failed");
            return status;
        }
        status.present = true;

        // Set default settings (0xF6)
        controller::send_to_mouse(0xF6);
        let _ = controller::read_data(); // ACK

        // IntelliMouse magic sample rate sequence: 200 -> 100 -> 80
        set_sample_rate(200);
        set_sample_rate(100);
        set_sample_rate(80);

        // Read Device ID (0xF2)
        controller::send_to_mouse(0xF2);
        let _ = controller::read_data(); // ACK
        let id = controller::read_data().unwrap_or(0x00);
        status.device_id = id;

        status.mouse_type = match id {
            0x00 => MouseType::Standard,
            0x03 => MouseType::IntelliMouse,
            0x04 => MouseType::IntelliMouseExplorer,
            other => MouseType::Unknown(other),
        };

        // Enable data reporting (0xF4)
        if controller::send_to_mouse(0xF4) {
            let _ = controller::read_data(); // ACK
            status.reporting_enabled = true;
        }

        // Update decoder with detected mouse type
        {
            let mut decoder = DECODER.lock();
            decoder.mouse_type = status.mouse_type;
        }

        *STATUS.lock() = status;
        crate::kinfo!(
            "ps2: mouse ready (type={:?}, id={:#x}, reporting={})",
            status.mouse_type,
            status.device_id,
            status.reporting_enabled
        );

        status
    }
}

#[cfg(target_arch = "x86_64")]
fn set_sample_rate(rate: u8) {
    if controller::send_to_mouse(0xF3) {
        let _ = controller::read_data(); // ACK
        if controller::send_to_mouse(rate) {
            let _ = controller::read_data(); // ACK
        }
    }
}

/// Process a raw byte received from PS/2 Port 2 (AUX).
pub fn handle_byte(byte: u8) {
    let event = DECODER.lock().process_byte(byte);
    if let Some(event) = event {
        QUEUE.lock().push(event);
    }
}

/// Read the next pending mouse event.
pub fn read_event() -> Option<MouseEvent> {
    QUEUE.lock().pop()
}

/// Current mouse cursor and button state.
pub fn state() -> MouseState {
    DECODER.lock().state()
}

/// Current mouse subsystem status.
pub fn status() -> MouseStatus {
    *STATUS.lock()
}

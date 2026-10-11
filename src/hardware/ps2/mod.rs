/*
 *
 *       src/hardware/ps2/mod.rs
 *       PS/2 input subsystem public interfaces and interrupt dispatch
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Industrial-grade PS/2 Subsystem.
//!
//! Provides unified management for:
//! - i8042 dual-channel keyboard/mouse controller
//! - PS/2 keyboard with Scancode Set 1 decoding and TTY forwarding
//! - PS/2 IntelliMouse with scroll wheel and coordinate clamping
//! - Automated boot self-tests and diagnostic status queries

pub mod controller;
pub mod keyboard;
pub mod mouse;
pub mod self_test;

use crate::sync::SpinLock;
#[allow(unused_imports)]
pub use controller::ControllerStatus;
#[allow(unused_imports)]
pub use keyboard::{
    KeyCode, KeyEvent, KeyState, Modifiers, modifiers, read_event as read_key, sync_leds,
};
#[allow(unused_imports)]
pub use mouse::{
    MouseButtons, MouseEvent, MouseState, MouseStatus, MouseType, read_event as read_mouse,
    set_bounds as set_mouse_bounds, state as mouse_state,
};

#[derive(Debug, Clone, Copy)]
pub struct Ps2Status {
    pub controller: ControllerStatus,
    pub mouse: MouseStatus,
    pub keyboard_ok: bool,
}

impl Ps2Status {
    pub const fn empty() -> Self {
        Self {
            controller: ControllerStatus::empty(),
            mouse: MouseStatus::empty(),
            keyboard_ok: false,
        }
    }
}

static SUBSYSTEM_STATUS: SpinLock<Ps2Status> = SpinLock::new(Ps2Status::empty());

/// Common interrupt handler for PS/2 keyboard and mouse data.
pub fn handle_irq(_vector: u32) {
    for _ in 0..64 {
        let status = controller::read_status();
        if status & controller::STATUS_OUTPUT_FULL == 0 {
            break;
        }
        let byte = controller::read_data_immediate();
        if status & controller::STATUS_AUX_DATA != 0 {
            mouse::handle_byte(byte);
        } else {
            keyboard::handle_byte(byte);
        }
    }
}

/// Initialize the complete PS/2 subsystem.
pub fn init() -> Ps2Status {
    let ctrl_status = controller::init();
    if !ctrl_status.present {
        return Ps2Status::empty();
    }

    let kb_ok = if ctrl_status.port1_ok {
        keyboard::init()
    } else {
        false
    };

    let mouse_status = if ctrl_status.port2_ok {
        mouse::init()
    } else {
        MouseStatus::empty()
    };

    #[cfg(target_arch = "x86_64")]
    {
        if ctrl_status.port1_ok {
            if let Ok(route) = crate::arch::apic::request_isa(1, handle_irq) {
                let _ = crate::arch::apic::set_mask(route, false);
            }
        }
        if ctrl_status.port2_ok {
            if let Ok(route) = crate::arch::apic::request_isa(12, handle_irq) {
                let _ = crate::arch::apic::set_mask(route, false);
            }
        }
    }

    let status = Ps2Status {
        controller: ctrl_status,
        mouse: mouse_status,
        keyboard_ok: kb_ok,
    };
    *SUBSYSTEM_STATUS.lock() = status;
    status
}

/// Query current PS/2 subsystem status.
pub fn status() -> Ps2Status {
    *SUBSYSTEM_STATUS.lock()
}

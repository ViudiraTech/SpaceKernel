/*
 *
 *       src/input/ps2.rs
 *       evdev devices for the i8042 keyboard and mouse
 *
 *       2026/10/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Bridges the existing PS/2 decoders to Linux key and pointer events. Codes
//! are from `include/uapi/linux/input-event-codes.h`.

use super::{BUS_I8042, Capabilities, EV_KEY, EV_LED, EV_MSC, EV_REL, EV_REP, InputDevice};
use crate::{
    hardware::ps2::{KeyCode, KeyState, MouseEvent},
    sync::SpinLock,
};
use alloc::sync::Arc;

const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const BTN_MIDDLE: u16 = 0x112;
const REL_X: u16 = 0;
const REL_Y: u16 = 1;
const REL_WHEEL: u16 = 8;
const REL_WHEEL_HI_RES: u16 = 11;
const MSC_SCAN: u16 = 4;

static KEYBOARD: SpinLock<Option<Arc<InputDevice>>> = SpinLock::new(None);
static MOUSE: SpinLock<Option<Arc<InputDevice>>> = SpinLock::new(None);
static LAST_BUTTONS: SpinLock<[bool; 3]> = SpinLock::new([false; 3]);

/// Linux `KEY_*` for a decoded key, or `None` for keys with no mapping.
pub fn linux_key(code: KeyCode) -> Option<u16> {
    use KeyCode::*;
    Some(match code {
        Escape => 1,
        Key1 => 2,
        Key2 => 3,
        Key3 => 4,
        Key4 => 5,
        Key5 => 6,
        Key6 => 7,
        Key7 => 8,
        Key8 => 9,
        Key9 => 10,
        Key0 => 11,
        Minus => 12,
        Equals => 13,
        Backspace => 14,
        Tab => 15,
        Q => 16,
        W => 17,
        E => 18,
        R => 19,
        T => 20,
        Y => 21,
        U => 22,
        I => 23,
        O => 24,
        P => 25,
        LeftBracket => 26,
        RightBracket => 27,
        Enter => 28,
        LeftCtrl => 29,
        A => 30,
        S => 31,
        D => 32,
        F => 33,
        G => 34,
        H => 35,
        J => 36,
        K => 37,
        L => 38,
        Semicolon => 39,
        Apostrophe => 40,
        Grave => 41,
        LeftShift => 42,
        Backslash => 43,
        Z => 44,
        X => 45,
        C => 46,
        V => 47,
        B => 48,
        N => 49,
        M => 50,
        Comma => 51,
        Period => 52,
        Slash => 53,
        RightShift => 54,
        KpMultiply => 55,
        LeftAlt => 56,
        Space => 57,
        CapsLock => 58,
        F1 => 59,
        F2 => 60,
        F3 => 61,
        F4 => 62,
        F5 => 63,
        F6 => 64,
        F7 => 65,
        F8 => 66,
        F9 => 67,
        F10 => 68,
        NumLock => 69,
        ScrollLock => 70,
        Kp7 => 71,
        Kp8 => 72,
        Kp9 => 73,
        KpMinus => 74,
        Kp4 => 75,
        Kp5 => 76,
        Kp6 => 77,
        KpPlus => 78,
        Kp1 => 79,
        Kp2 => 80,
        Kp3 => 81,
        Kp0 => 82,
        KpDot => 83,
        F11 => 87,
        F12 => 88,
        KpEnter => 96,
        RightCtrl => 97,
        KpDivide => 98,
        PrintScreen => 99,
        RightAlt => 100,
        Home => 102,
        Up => 103,
        PageUp => 104,
        Left => 105,
        Right => 106,
        End => 107,
        Down => 108,
        PageDown => 109,
        Insert => 110,
        Delete => 111,
        Pause => 119,
        LeftGui => 125,
        RightGui => 126,
        Menu => 127,
        Unknown(_) => return None,
    })
}

fn all_keys() -> impl Iterator<Item = u16> {
    (1u16..=88).chain([
        96, 97, 98, 99, 100, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 119, 125, 126, 127,
    ])
}

/// Register whichever i8042 ports probed successfully.
pub fn register() {
    let status = crate::hardware::ps2::status();
    if status.keyboard_ok {
        let mut caps = Capabilities::new(
            "AT Translated Set 2 keyboard",
            "isa0060/serio0/input0",
            BUS_I8042,
            0x0001,
            0x0001,
        );
        caps.ev.set(EV_KEY as usize);
        caps.ev.set(EV_REP as usize);
        caps.ev.set(EV_LED as usize);
        caps.ev.set(EV_MSC as usize);
        caps.msc.set(MSC_SCAN as usize);
        for led in 0..3 {
            caps.led.set(led);
        }
        for key in all_keys() {
            caps.key.set(key as usize);
        }
        match super::register(caps) {
            Ok(device) => *KEYBOARD.lock() = Some(device),
            Err(error) => crate::kwarn!("input: PS/2 keyboard: {:?}", error),
        }
    }
    if status.mouse.present {
        let mut caps = Capabilities::new(
            "PS/2 Generic Mouse",
            "isa0060/serio1/input0",
            BUS_I8042,
            0x0002,
            0x0001,
        );
        caps.ev.set(EV_KEY as usize);
        caps.ev.set(EV_REL as usize);
        for button in [BTN_LEFT, BTN_RIGHT, BTN_MIDDLE] {
            caps.key.set(button as usize);
        }
        for axis in [REL_X, REL_Y, REL_WHEEL, REL_WHEEL_HI_RES] {
            caps.rel.set(axis as usize);
        }
        caps.properties.set(0); // INPUT_PROP_POINTER
        match super::register(caps) {
            Ok(device) => *MOUSE.lock() = Some(device),
            Err(error) => crate::kwarn!("input: PS/2 mouse: {:?}", error),
        }
    }
}

/// Called from the keyboard IRQ path for every decoded transition.
pub fn key_event(code: KeyCode, state: KeyState) {
    let Some(key) = linux_key(code) else {
        return;
    };
    let device = KEYBOARD.lock().clone();
    if let Some(device) = device {
        let value = match state {
            KeyState::Pressed => 1,
            KeyState::Released => 0,
        };
        device.report(EV_KEY, key, value);
        device.sync();
    }
}

/// Called from the mouse IRQ path for every decoded packet.
pub fn mouse_event(event: &MouseEvent) {
    let device = MOUSE.lock().clone();
    let Some(device) = device else {
        return;
    };
    device.report(EV_REL, REL_X, event.dx);
    device.report(EV_REL, REL_Y, event.dy);
    if event.dz != 0 {
        device.report(EV_REL, REL_WHEEL, -event.dz);
        device.report(EV_REL, REL_WHEEL_HI_RES, -event.dz * 120);
    }
    let now = [
        event.buttons.left,
        event.buttons.right,
        event.buttons.middle,
    ];
    let mut last = LAST_BUTTONS.lock();
    for (index, button) in [BTN_LEFT, BTN_RIGHT, BTN_MIDDLE].into_iter().enumerate() {
        if now[index] != last[index] {
            device.report(EV_KEY, button, i32::from(now[index]));
        }
    }
    *last = now;
    drop(last);
    device.sync();
}

/*
 *
 *       src/hardware/ps2/self_test.rs
 *       Automated validation for PS/2 keyboard and IntelliMouse decoders
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Industrial-grade verification suite for PS/2 input subsystem.
//!
//! Validates:
//! - Scancode Set 1 make and break codes
//! - Shift, Ctrl, and CapsLock modifier semantics
//! - Extended E0 cursor keys and ANSI escape generation
//! - Multi-byte Pause key (0xE1) sequence
//! - Mouse standard 3-byte packets and button states
//! - IntelliMouse 4-byte packets and scroll wheel decoding
//! - Packet stream re-synchronization on corrupted or dropped bytes
//! - Display bounds coordinate clamping

use super::keyboard::{KeyCode, KeyState, KeyboardDecoder};
use super::mouse::{MouseDecoder, MouseType};

pub fn run() -> bool {
    test_keyboard_basic();
    test_keyboard_modifiers();
    test_keyboard_extended();
    test_keyboard_pause();
    test_mouse_standard();
    test_mouse_intellimouse();
    test_mouse_sync_recovery();
    test_mouse_bounds_clamping();

    crate::kinfo!("ps2: all subsystem self-tests PASSED");
    true
}

fn test_keyboard_basic() {
    let mut dec = KeyboardDecoder::new();

    // Make 'A' (0x1E)
    let ev1 = dec.process_byte(0x1E).expect("make 'A'");
    assert_eq!(ev1.code, KeyCode::A);
    assert_eq!(ev1.state, KeyState::Pressed);
    assert_eq!(ev1.ascii, Some(b'a'));

    // Break 'A' (0x9E)
    let ev2 = dec.process_byte(0x9E).expect("break 'A'");
    assert_eq!(ev2.code, KeyCode::A);
    assert_eq!(ev2.state, KeyState::Released);
    assert_eq!(ev2.ascii, None);

    // Make '1' (0x02)
    let ev3 = dec.process_byte(0x02).expect("make '1'");
    assert_eq!(ev3.code, KeyCode::Key1);
    assert_eq!(ev3.ascii, Some(b'1'));

    // Enter (0x1C)
    let ev4 = dec.process_byte(0x1C).expect("make Enter");
    assert_eq!(ev4.code, KeyCode::Enter);
    assert_eq!(ev4.ascii, Some(b'\n'));
}

fn test_keyboard_modifiers() {
    let mut dec = KeyboardDecoder::new();

    // Left Shift press (0x2A)
    let ev_shift = dec.process_byte(0x2A).expect("Left Shift press");
    assert_eq!(ev_shift.code, KeyCode::LeftShift);
    assert!(dec.modifiers().shift());

    // 'A' press under shift
    let ev_a = dec.process_byte(0x1E).expect("Shift + A");
    assert_eq!(ev_a.code, KeyCode::A);
    assert_eq!(ev_a.ascii, Some(b'A'));

    // Left Shift release (0xAA)
    dec.process_byte(0xAA).expect("Left Shift release");
    assert!(!dec.modifiers().shift());

    // Left Ctrl press (0x1D)
    dec.process_byte(0x1D).expect("Left Ctrl press");
    assert!(dec.modifiers().ctrl());

    // Ctrl + C (0x2E is C) -> ASCII 0x03
    let ev_c = dec.process_byte(0x2E).expect("Ctrl + C");
    assert_eq!(ev_c.code, KeyCode::C);
    assert_eq!(ev_c.ascii, Some(0x03));

    // Left Ctrl release (0x9D)
    dec.process_byte(0x9D).expect("Left Ctrl release");
    assert!(!dec.modifiers().ctrl());

    // CapsLock toggle (0x3A)
    dec.process_byte(0x3A).expect("CapsLock toggle on");
    assert!(dec.modifiers().caps_lock);
    let ev_cap_a = dec.process_byte(0x1E).expect("CapsLock + A");
    assert_eq!(ev_cap_a.ascii, Some(b'A'));

    dec.process_byte(0x3A).expect("CapsLock toggle off");
    assert!(!dec.modifiers().caps_lock);
    let ev_lower_a = dec.process_byte(0x1E).expect("normal A");
    assert_eq!(ev_lower_a.ascii, Some(b'a'));
}

fn test_keyboard_extended() {
    let mut dec = KeyboardDecoder::new();

    // Up Arrow (0xE0, 0x48)
    assert!(dec.process_byte(0xE0).is_none());
    let ev_up = dec.process_byte(0x48).expect("Up arrow");
    assert_eq!(ev_up.code, KeyCode::Up);
    assert_eq!(ev_up.state, KeyState::Pressed);
    assert_eq!(ev_up.ansi_sequence(), Some(&b"\x1b[A"[..]));

    // Up Arrow release (0xE0, 0xC8)
    assert!(dec.process_byte(0xE0).is_none());
    let ev_up_rel = dec.process_byte(0xC8).expect("Up arrow release");
    assert_eq!(ev_up_rel.code, KeyCode::Up);
    assert_eq!(ev_up_rel.state, KeyState::Released);

    // Right Ctrl (0xE0, 0x1D)
    assert!(dec.process_byte(0xE0).is_none());
    let ev_rctrl = dec.process_byte(0x1D).expect("Right Ctrl");
    assert_eq!(ev_rctrl.code, KeyCode::RightCtrl);
    assert!(dec.modifiers().right_ctrl);
}

fn test_keyboard_pause() {
    let mut dec = KeyboardDecoder::new();
    let pause_seq = [0xE1, 0x1D, 0x45, 0xE1, 0x9D, 0xC5];

    for (i, &b) in pause_seq.iter().enumerate() {
        let ev = dec.process_byte(b);
        if i < 5 {
            assert!(ev.is_none());
        } else {
            let ev_pause = ev.expect("Pause completed");
            assert_eq!(ev_pause.code, KeyCode::Pause);
            assert_eq!(ev_pause.state, KeyState::Pressed);
        }
    }
}

fn test_mouse_standard() {
    let mut dec = MouseDecoder::new(MouseType::Standard, 1024, 768);

    // Initial position should be center (512, 384)
    assert_eq!(dec.state().x, 512);
    assert_eq!(dec.state().y, 384);

    // Feed packet: [0x08, 10, 20] (dx = +10, raw_dy = +20 -> screen dy = -20)
    assert!(dec.process_byte(0x08).is_none());
    assert!(dec.process_byte(10).is_none());
    let ev = dec.process_byte(20).expect("standard 3-byte packet");

    assert_eq!(ev.dx, 10);
    assert_eq!(ev.dy, -20);
    assert_eq!(ev.raw_dy, 20);
    assert_eq!(ev.dz, 0);
    assert!(!ev.buttons.left);
    assert_eq!(ev.state.x, 522);
    assert_eq!(ev.state.y, 364);

    // Left button pressed with negative X:
    // Byte 0: 0x01 (Left) | 0x08 (Sync) | 0x10 (X sign) = 0x19
    // Byte 1: 0xF6 (-10)
    // Byte 2: 0x00
    assert!(dec.process_byte(0x19).is_none());
    assert!(dec.process_byte(0xF6).is_none());
    let ev_btn = dec.process_byte(0x00).expect("packet with left button");

    assert_eq!(ev_btn.dx, -10);
    assert_eq!(ev_btn.dy, 0);
    assert!(ev_btn.buttons.left);
    assert_eq!(ev_btn.state.x, 512);
}

fn test_mouse_intellimouse() {
    let mut dec = MouseDecoder::new(MouseType::IntelliMouse, 1024, 768);

    // 4-byte packet with scroll wheel up (dz = +1)
    assert!(dec.process_byte(0x08).is_none());
    assert!(dec.process_byte(0x05).is_none());
    assert!(dec.process_byte(0x00).is_none());
    let ev = dec.process_byte(0x01).expect("intellimouse packet");

    assert_eq!(ev.dx, 5);
    assert_eq!(ev.dz, 1);

    // Scroll wheel down (dz = -1 -> 0xFF)
    assert!(dec.process_byte(0x08).is_none());
    assert!(dec.process_byte(0x00).is_none());
    assert!(dec.process_byte(0x00).is_none());
    let ev_down = dec.process_byte(0xFF).expect("intellimouse wheel down");

    assert_eq!(ev_down.dz, -1);
}

fn test_mouse_sync_recovery() {
    let mut dec = MouseDecoder::new(MouseType::Standard, 1024, 768);

    // Feed corrupted / noise bytes where bit 3 is NOT 1 (e.g. 0x00, 0x02, 0x05, 0xF7)
    assert!(dec.process_byte(0x00).is_none());
    assert!(dec.process_byte(0x02).is_none());
    assert!(dec.process_byte(0x05).is_none());

    // Now send legitimate packet: [0x08, 2, 4]
    assert!(dec.process_byte(0x08).is_none());
    assert!(dec.process_byte(2).is_none());
    let ev = dec.process_byte(4).expect("synced packet after noise");

    assert_eq!(ev.dx, 2);
    assert_eq!(ev.dy, -4);
}

fn test_mouse_bounds_clamping() {
    let mut dec = MouseDecoder::new(MouseType::Standard, 100, 100);

    // Move mouse far left and up (dx = -200, dy = -200)
    // Byte 0: 0x08 | 0x10 | 0x20 = 0x38
    dec.process_byte(0x38);
    dec.process_byte(0x38); // dx = -200 (approx)
    let ev = dec.process_byte(0x38).expect("clamp test 1");

    // Coordinate must be clamped to >= 0
    assert!(ev.state.x >= 0);
    assert!(ev.state.y >= 0);
    assert!(ev.state.x < 100);
    assert!(ev.state.y < 100);
}

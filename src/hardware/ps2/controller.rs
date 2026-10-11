/*
 *
 *       src/hardware/ps2/controller.rs
 *       i8042 PS/2 controller driver, dual-channel detection and port I/O
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Industrial-grade i8042 PS/2 Controller Driver.
//!
//! Handles controller self-test, dual-channel detection, bounded timeout polling,
//! configuration byte programming, and command dispatch to keyboard and mouse ports.

#[cfg(target_arch = "x86_64")]
use crate::{kinfo, kwarn};

pub const DATA_PORT: u16 = 0x60;
pub const STATUS_PORT: u16 = 0x64;
pub const COMMAND_PORT: u16 = 0x64;

// Status register bits
pub const STATUS_OUTPUT_FULL: u8 = 1 << 0; // OBF: 1 = data ready to read from 0x60
pub const STATUS_INPUT_FULL: u8 = 1 << 1; // IBF: 1 = controller busy, cannot write to 0x60/0x64
pub const STATUS_SYSTEM: u8 = 1 << 2; // System flag
pub const STATUS_COMMAND_DATA: u8 = 1 << 3; // 0 = data for device, 1 = data for controller
pub const STATUS_KEYBOARD_LOCK: u8 = 1 << 4; // 1 = keyboard not locked
pub const STATUS_AUX_DATA: u8 = 1 << 5; // AUX: 1 = data from mouse (port 2), 0 = keyboard (port 1)
pub const STATUS_TIMEOUT_ERR: u8 = 1 << 6; // 1 = timeout error
pub const STATUS_PARITY_ERR: u8 = 1 << 7; // 1 = parity error

// Controller commands
pub const CMD_READ_CONFIG: u8 = 0x20;
pub const CMD_WRITE_CONFIG: u8 = 0x60;
pub const CMD_DISABLE_PORT2: u8 = 0xA7;
pub const CMD_ENABLE_PORT2: u8 = 0xA8;
pub const CMD_TEST_PORT2: u8 = 0xA9;
pub const CMD_SELF_TEST: u8 = 0xAA;
pub const CMD_TEST_PORT1: u8 = 0xAB;
pub const CMD_DISABLE_PORT1: u8 = 0xAD;
pub const CMD_ENABLE_PORT1: u8 = 0xAE;
pub const CMD_WRITE_PORT2: u8 = 0xD4;

// Configuration byte (CCB) bits
pub const CONFIG_PORT1_INT: u8 = 1 << 0; // First port interrupt enabled (IRQ 1)
pub const CONFIG_PORT2_INT: u8 = 1 << 1; // Second port interrupt enabled (IRQ 12)
pub const CONFIG_SYSTEM_FLAG: u8 = 1 << 2; // System passed POST
pub const CONFIG_PORT1_CLOCK_OFF: u8 = 1 << 4; // First port clock disabled
pub const CONFIG_PORT2_CLOCK_OFF: u8 = 1 << 5; // Second port clock disabled
pub const CONFIG_PORT1_TRANSLATE: u8 = 1 << 6; // First port translation (Set 2 -> Set 1)

const TIMEOUT_SPINS: usize = 100_000;

#[derive(Debug, Clone, Copy)]
pub struct ControllerStatus {
    pub present: bool,
    pub self_test_ok: bool,
    pub dual_channel: bool,
    pub port1_ok: bool,
    pub port2_ok: bool,
    pub config_byte: u8,
}

impl ControllerStatus {
    pub const fn empty() -> Self {
        Self {
            present: false,
            self_test_ok: false,
            dual_channel: false,
            port1_ok: false,
            port2_ok: false,
            config_byte: 0,
        }
    }
}

#[inline]
fn in_byte(port: u16) -> u8 {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let byte: u8;
        core::arch::asm!("in al, dx", in("dx") port, out("al") byte, options(nomem, nostack, preserves_flags));
        byte
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = port;
        0
    }
}

#[inline]
fn out_byte(port: u16, val: u8) {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (port, val);
    }
}

/// Read the current i8042 status register.
#[inline]
pub fn read_status() -> u8 {
    in_byte(STATUS_PORT)
}

/// Wait until output buffer is full (data ready to read).
pub fn wait_read() -> bool {
    for _ in 0..TIMEOUT_SPINS {
        if read_status() & STATUS_OUTPUT_FULL != 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

/// Wait until input buffer is empty (safe to write command or data).
pub fn wait_write() -> bool {
    for _ in 0..TIMEOUT_SPINS {
        if read_status() & STATUS_INPUT_FULL == 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

/// Flush any pending data from the controller output buffer.
pub fn flush_output() {
    let mut count = 0;
    while read_status() & STATUS_OUTPUT_FULL != 0 && count < 64 {
        let _ = in_byte(DATA_PORT);
        count += 1;
    }
}

/// Read a data byte from port 0x60 if ready.
pub fn read_data() -> Option<u8> {
    if wait_read() {
        Some(in_byte(DATA_PORT))
    } else {
        None
    }
}

/// Read a data byte immediately from port 0x60 without waiting.
#[inline]
pub fn read_data_immediate() -> u8 {
    in_byte(DATA_PORT)
}

/// Send a command to the controller command port 0x64.
pub fn send_command(cmd: u8) -> bool {
    if wait_write() {
        out_byte(COMMAND_PORT, cmd);
        true
    } else {
        false
    }
}

/// Send a data byte to the controller data port 0x60 (addressed to first port / keyboard).
pub fn send_data(data: u8) -> bool {
    if wait_write() {
        out_byte(DATA_PORT, data);
        true
    } else {
        false
    }
}

/// Send a command/data byte to the second port (mouse) via command 0xD4.
pub fn send_to_mouse(data: u8) -> bool {
    if !send_command(CMD_WRITE_PORT2) {
        return false;
    }
    send_data(data)
}

/// Initialize the i8042 PS/2 controller following OSDev / industrial standards.
pub fn init() -> ControllerStatus {
    #[cfg(not(target_arch = "x86_64"))]
    {
        return ControllerStatus::empty();
    }

    #[cfg(target_arch = "x86_64")]
    {
        let mut status = ControllerStatus::empty();

        // Step 1: Flush stale data
        flush_output();

        // Step 2: Disable both ports during setup
        send_command(CMD_DISABLE_PORT1);
        send_command(CMD_DISABLE_PORT2);
        flush_output();

        // Step 3: Read configuration byte
        if !send_command(CMD_READ_CONFIG) {
            kwarn!("i8042: controller command write timeout");
            return status;
        }
        let mut ccb = match read_data() {
            Some(val) => val,
            None => {
                kwarn!("i8042: failed to read configuration byte");
                return status;
            }
        };

        // Disable interrupts (bits 0, 1) and translation (bit 6) for testing
        ccb &= !(CONFIG_PORT1_INT | CONFIG_PORT2_INT | CONFIG_PORT1_TRANSLATE);
        send_command(CMD_WRITE_CONFIG);
        send_data(ccb);

        // Step 4: Controller Self-Test
        if send_command(CMD_SELF_TEST) {
            match read_data() {
                Some(0x55) => {
                    status.self_test_ok = true;
                    status.present = true;
                }
                Some(val) => {
                    kwarn!("i8042: self-test failed with code {:#x}", val);
                }
                None => {
                    kwarn!("i8042: self-test timed out");
                }
            }
        }

        // Restore CCB after self-test
        send_command(CMD_WRITE_CONFIG);
        send_data(ccb);

        // Step 5: Determine if dual channel exists
        send_command(CMD_ENABLE_PORT2);
        send_command(CMD_READ_CONFIG);
        let check_ccb = read_data().unwrap_or(ccb);
        let dual_channel = (check_ccb & CONFIG_PORT2_CLOCK_OFF) == 0;
        status.dual_channel = dual_channel;
        if dual_channel {
            // Re-disable port 2 for now
            send_command(CMD_DISABLE_PORT2);
        }

        // Step 6: Test Port 1 (keyboard)
        if send_command(CMD_TEST_PORT1) {
            match read_data() {
                Some(0x00) => status.port1_ok = true,
                Some(code) => kwarn!("i8042: port 1 test returned {:#x}", code),
                None => kwarn!("i8042: port 1 test timed out"),
            }
        }

        // Step 7: Test Port 2 (mouse) if dual channel
        if dual_channel && send_command(CMD_TEST_PORT2) {
            match read_data() {
                Some(0x00) => status.port2_ok = true,
                Some(code) => kwarn!("i8042: port 2 test returned {:#x}", code),
                None => kwarn!("i8042: port 2 test timed out"),
            }
        }

        // Step 8: Enable interrupts and translation in CCB
        let mut final_ccb = ccb | CONFIG_PORT1_INT | CONFIG_PORT1_TRANSLATE;
        if dual_channel && status.port2_ok {
            final_ccb |= CONFIG_PORT2_INT;
        }
        send_command(CMD_WRITE_CONFIG);
        send_data(final_ccb);
        status.config_byte = final_ccb;

        // Step 9: Re-enable functional ports
        if status.port1_ok {
            send_command(CMD_ENABLE_PORT1);
        }
        if dual_channel && status.port2_ok {
            send_command(CMD_ENABLE_PORT2);
        }

        kinfo!(
            "i8042: controller ready (dual_channel={}, port1_ok={}, port2_ok={}, CCB={:#x})",
            status.dual_channel,
            status.port1_ok,
            status.port2_ok,
            status.config_byte
        );

        status
    }
}

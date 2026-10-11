/*
 *
 *       src/shell/mod.rs
 *       Interactive kernel shell and diagnostic command processor
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Interactive kernel shell running as a preemptive kernel task.

use crate::{
    arch, boot, cpuid, mm, pci, printk, sched,
    tty::{self, ReadError, TtyDevice},
};
use alloc::{format, vec::Vec};

const PROMPT: &str = "spacekernel> ";

pub fn start() {
    let _ = sched::spawn(
        || {
            shell_task();
        },
        0,
    );
}

fn print_tty(s: &str) {
    tty::write(TtyDevice::Serial0, s.as_bytes());
    tty::write(TtyDevice::Foreground, s.as_bytes());
}

fn shell_task() {
    print_tty("\n=== SpaceKernel Interactive Shell ===\n");
    print_tty("Type 'help' for available commands.\n\n");
    print_tty(PROMPT);

    let mut line_buf = [0u8; 256];
    let mut line_pos = 0;

    loop {
        // Read input from Serial0
        let mut byte = [0u8; 1];
        match tty::read(TtyDevice::Serial0, &mut byte) {
            Ok(1) => {
                let b = byte[0];
                if b == b'\n' || b == b'\r' {
                    print_tty("\n");
                    if line_pos > 0 {
                        if let Ok(cmd_str) = core::str::from_utf8(&line_buf[..line_pos]) {
                            execute_command(cmd_str.trim());
                        }
                        line_pos = 0;
                    }
                    print_tty(PROMPT);
                } else if b == 0x08 || b == 0x7f {
                    if line_pos > 0 {
                        line_pos -= 1;
                    }
                } else if line_pos < line_buf.len() {
                    line_buf[line_pos] = b;
                    line_pos += 1;
                }
            }
            Ok(_) => {
                sched::sleep_micros(10_000);
            }
            Err(ReadError::WouldBlock) => {
                sched::sleep_micros(20_000);
            }
            Err(_) => {
                sched::sleep_micros(50_000);
            }
        }
    }
}

fn execute_command(line: &str) {
    if line.is_empty() {
        return;
    }
    let mut parts = line.split_ascii_whitespace();
    let Some(cmd) = parts.next() else {
        return;
    };

    match cmd {
        "help" => cmd_help(),
        "ps" => cmd_ps(),
        "meminfo" => cmd_meminfo(),
        "cpuinfo" => cmd_cpuinfo(),
        "lspci" => cmd_lspci(),
        "dmesg" => cmd_dmesg(),
        "uname" => cmd_uname(),
        "echo" => {
            let rest: Vec<&str> = parts.collect();
            print_tty(&format!("{}\n", rest.join(" ")));
        }
        "reboot" | "poweroff" => {
            print_tty("System halting...\n");
            loop {
                arch::halt();
            }
        }
        _ => {
            print_tty(&format!(
                "Unknown command: '{}'. Type 'help' for commands.\n",
                cmd
            ));
        }
    }
}

fn cmd_help() {
    print_tty("Available commands:\n");
    print_tty("  help      - Show this help message\n");
    print_tty("  ps        - Display active kernel tasks and scheduler statistics\n");
    print_tty("  meminfo   - Display physical memory and allocator statistics\n");
    print_tty("  cpuinfo   - Display processor capability and topology information\n");
    print_tty("  lspci     - Display enumerated PCI configuration devices\n");
    print_tty("  dmesg     - Dump kernel log buffer\n");
    print_tty("  uname     - Display kernel architecture and system information\n");
    print_tty("  echo      - Print arguments\n");
    print_tty("  reboot    - Halt the machine\n");
}

fn cmd_ps() {
    print_tty("  TID | STATE    | CPU | RUNTIME (ms) | SWITCHES | INVOLUNTARY\n");
    print_tty("------+----------+-----+--------------+----------+------------\n");
    let tasks = sched::task_snapshots();
    for t in tasks {
        let state_str = match t.state {
            sched::TaskState::Ready => "Ready   ",
            sched::TaskState::Running => "Running ",
            sched::TaskState::Blocked => "Blocked ",
            sched::TaskState::Dead => "Dead    ",
        };
        let runtime_ms = t.runtime_ns / 1_000_000;
        print_tty(&format!(
            "{:5} | {} | {:3} | {:12} | {:8} | {:10}\n",
            t.tid, state_str, t.cpu, runtime_ms, t.switches, t.involuntary_switches
        ));
    }
}

fn cmd_meminfo() {
    let pmm_stats = mm::pmm::stats();
    print_tty(&format!("PMM total frames: {}\n", pmm_stats.frames));
    print_tty(&format!("PMM free frames:  {}\n", pmm_stats.free));
    print_tty(&format!(
        "PMM used memory:  {} KiB / {} KiB\n",
        (pmm_stats.frames - pmm_stats.free) * 4,
        pmm_stats.frames * 4
    ));
    if let Some(root) = mm::vmm::root() {
        print_tty(&format!("VMM kernel root:  {:#x}\n", root));
    }
}

fn cmd_cpuinfo() {
    let cpu = cpuid::info();
    let online = boot::cpu_count();
    print_tty(&format!("CPUs configured:  {}\n", online));
    if let Some(c) = cpu {
        print_tty(&format!("Architecture:     {:?}\n", c.architecture));
        print_tty(&format!("Vendor:           {}\n", c.vendor.as_str()));
        print_tty(&format!("Model:            {}\n", c.model.as_str()));
        print_tty(&format!("Hardware ID:      {:#x}\n", c.hardware_id));
        print_tty(&format!("Physical Bits:    {:?}\n", c.physical_bits));
        print_tty(&format!("Virtual Bits:     {:?}\n", c.virtual_bits));
    }
}

fn cmd_lspci() {
    let devs = pci::devices();
    print_tty(&format!("Discovered {} PCI function(s):\n", devs.len()));
    for d in devs {
        print_tty(&format!(
            "  {:04x}:{:04x} class {:02x}:{:02x}:{:02x} @ {}\n",
            d.vendor_id, d.device_id, d.class, d.subclass, d.programming_interface, d.address
        ));
    }
}

fn cmd_dmesg() {
    let next = printk::next_sequence();
    let start = next.saturating_sub(128);
    for seq in start..next {
        if let Some(rec) = printk::read(seq) {
            let msg = core::str::from_utf8(&rec.bytes[..rec.length]).unwrap_or("<utf8 err>");
            let sec = rec.timestamp_us / 1_000_000;
            let usec = rec.timestamp_us % 1_000_000;
            print_tty(&format!("[{:5}.{:06}] {}\n", sec, usec, msg.trim_end()));
        }
    }
}

fn cmd_uname() {
    let arch_name = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "riscv64") {
        "riscv64"
    } else {
        "unknown"
    };
    print_tty(&format!(
        "SpaceKernel 0.1.0 ({arch_name}) SMP EEVDF Linux-ABI #1 2026\n"
    ));
}

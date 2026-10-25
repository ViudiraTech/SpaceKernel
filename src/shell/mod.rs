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
    arch, block, boot, cpuid, mm, pci, printk, sched,
    tty::{self, ReadError, TtyDevice},
    vfs::{self, InodeKind, abi},
};
use alloc::{format, string::String, vec::Vec};

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

    loop {
        // Read input from Serial0 or Foreground VT
        let read_res = match tty::read(TtyDevice::Serial0, &mut line_buf) {
            Ok(n) if n > 0 => Ok(n),
            _ => tty::read(TtyDevice::Foreground, &mut line_buf),
        };
        match read_res {
            Ok(n) if n > 0 => {
                let mut len = n;
                while len > 0 && (line_buf[len - 1] == b'\n' || line_buf[len - 1] == b'\r') {
                    len -= 1;
                }
                if let Ok(cmd_str) = core::str::from_utf8(&line_buf[..len]) {
                    let cmd_trimmed = cmd_str.trim();
                    if !cmd_trimmed.is_empty() {
                        execute_command(cmd_trimmed);
                    }
                }
                print_tty(PROMPT);
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

fn current_cwd() -> String {
    let tbl = vfs::current_fd_table();
    let cwd = String::from(tbl.lock().cwd());
    cwd
}

fn resolve_arg_path(arg: &str) -> String {
    vfs::normalize_path(&current_cwd(), arg)
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
        "ps2" => cmd_ps2(),
        "meminfo" => cmd_meminfo(),
        "cpuinfo" => cmd_cpuinfo(),
        "lspci" => cmd_lspci(),
        "lsblk" => cmd_lsblk(),
        "blkbench" => cmd_blkbench(),
        "dmesg" => cmd_dmesg(),
        "uname" => cmd_uname(),
        "pwd" => print_tty(&format!("{}\n", current_cwd())),
        "cd" => cmd_cd(parts.next().unwrap_or("/")),
        "ls" => cmd_ls(parts.next()),
        "cat" => cmd_cat(parts.next()),
        "touch" => cmd_touch(parts.next()),
        "write" => {
            let file = parts.next();
            let rest: Vec<&str> = parts.collect();
            cmd_write(file, &rest.join(" "));
        }
        "mkdir" => cmd_mkdir(parts.next()),
        "rm" => cmd_rm(parts.next(), false),
        "rmdir" => cmd_rm(parts.next(), true),
        "ln" => cmd_ln(parts.next(), parts.next()),
        "stat" => cmd_stat(parts.next()),
        "mount" => cmd_mount(parts.next(), parts.next(), parts.next()),
        "umount" => cmd_umount(parts.next()),
        "df" => cmd_df(),
        "sync" => {
            vfs::sync_all();
            print_tty("Filesystems and block caches synchronized.\n");
        }
        "echo" => {
            let rest: Vec<&str> = parts.collect();
            print_tty(&format!("{}\n", rest.join(" ")));
        }
        "reboot" | "poweroff" => {
            vfs::sync_all();
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
    print_tty("  help                  - Show this help message\n");
    print_tty("  pwd / cd [dir]        - Print or change current working directory\n");
    print_tty("  ls [path]             - List directory contents via VFS\n");
    print_tty("  cat <file>            - Display file contents\n");
    print_tty("  touch <file>          - Create an empty file\n");
    print_tty("  write <file> <text>   - Write text to a file\n");
    print_tty("  mkdir <dir>           - Create a directory\n");
    print_tty("  rm <file> / rmdir <d> - Remove a file or empty directory\n");
    print_tty("  ln <target> <link>    - Create a symbolic link\n");
    print_tty("  stat <path>           - Display VFS inode metadata\n");
    print_tty("  mount / umount        - List, mount or unmount filesystems\n");
    print_tty("  df                    - Display mounted filesystem usage\n");
    print_tty("  lsblk                 - Display IDE/AHCI/NVMe/RAM block devices & cache stats\n");
    print_tty("  blkbench              - Run IDE / AHCI / NVMe / RAMDisk storage benchmark\n");
    print_tty("  sync                  - Flush all filesystem and block caches\n");
    print_tty("  ps                    - Display active kernel tasks and scheduler statistics\n");
    print_tty(
        "  ps2                   - Display PS/2 controller, keyboard and mouse diagnostics\n",
    );
    print_tty("  meminfo               - Display physical memory and allocator statistics\n");
    print_tty("  cpuinfo               - Display processor capability and topology information\n");
    print_tty("  lspci                 - Display enumerated PCI configuration devices\n");
    print_tty("  dmesg                 - Dump kernel log buffer\n");
    print_tty("  uname                 - Display kernel architecture and system information\n");
    print_tty("  echo <args...>        - Print arguments\n");
    print_tty("  reboot                - Sync filesystems and halt the machine\n");
}

fn cmd_cd(target: &str) {
    let abs = resolve_arg_path(target);
    match vfs::lookup_abs(&abs, true) {
        Ok((inode, final_path)) => match inode.metadata() {
            Ok(meta) if meta.kind == InodeKind::Directory => {
                vfs::current_fd_table().lock().set_cwd(final_path);
            }
            Ok(_) => print_tty(&format!("cd: {}: Not a directory\n", target)),
            Err(e) => print_tty(&format!("cd: {}: {:?}\n", target, e)),
        },
        Err(e) => print_tty(&format!("cd: {}: {:?}\n", target, e)),
    }
}

fn cmd_ls(target: Option<&str>) {
    let abs = resolve_arg_path(target.unwrap_or("."));
    let (inode, final_path) = match vfs::lookup_abs(&abs, true) {
        Ok(r) => r,
        Err(e) => {
            print_tty(&format!("ls: cannot access '{}': {:?}\n", abs, e));
            return;
        }
    };
    let Ok(meta) = inode.metadata() else {
        print_tty(&format!("ls: metadata error on '{}'\n", abs));
        return;
    };
    if meta.kind != InodeKind::Directory {
        print_tty(&format!("{:8}  {}\n", meta.size, final_path));
        return;
    }
    match vfs::readdir_with_mounts(&final_path, &inode) {
        Ok(entries) => {
            print_tty("TYPE   INO       SIZE  NAME\n");
            print_tty("----+-------+--------+------------------------\n");
            for ent in entries {
                let child_path = if final_path == "/" {
                    format!("/{}", ent.name)
                } else {
                    format!("{}/{}", final_path, ent.name)
                };
                let size = if ent.name == "." || ent.name == ".." {
                    0
                } else {
                    vfs::lookup_abs(&child_path, false)
                        .and_then(|(c, _)| c.metadata())
                        .map(|m| m.size)
                        .unwrap_or(0)
                };
                let kind_ch = match ent.kind {
                    InodeKind::Directory => "dir ",
                    InodeKind::Regular => "file",
                    InodeKind::CharDevice => "chr ",
                    InodeKind::BlockDevice => "blk ",
                    InodeKind::Symlink => "link",
                    InodeKind::Fifo => "fifo",
                    InodeKind::Socket => "sock",
                };
                print_tty(&format!(
                    "{} {:7} {:8}  {}\n",
                    kind_ch, ent.ino, size, ent.name
                ));
            }
        }
        Err(e) => print_tty(&format!("ls: {}: {:?}\n", final_path, e)),
    }
}

fn cmd_cat(target: Option<&str>) {
    let Some(arg) = target else {
        print_tty("Usage: cat <file>\n");
        return;
    };
    let abs = resolve_arg_path(arg);
    match vfs::read_file(&abs) {
        Ok(bytes) => {
            if let Ok(text) = core::str::from_utf8(&bytes) {
                print_tty(text);
                if !text.ends_with('\n') {
                    print_tty("\n");
                }
            } else {
                print_tty(&format!("<binary data: {} bytes>\n", bytes.len()));
            }
        }
        Err(e) => print_tty(&format!("cat: {}: {:?}\n", abs, e)),
    }
}

fn cmd_touch(target: Option<&str>) {
    let Some(arg) = target else {
        print_tty("Usage: touch <file>\n");
        return;
    };
    let abs = resolve_arg_path(arg);
    match vfs::open_abs(&abs, abi::O_CREAT | abi::O_WRONLY, 0o644) {
        Ok(f) => {
            let _ = f.inode.sync();
        }
        Err(e) => print_tty(&format!("touch: {}: {:?}\n", abs, e)),
    }
}

fn cmd_write(target: Option<&str>, content: &str) {
    let Some(arg) = target else {
        print_tty("Usage: write <file> <text...>\n");
        return;
    };
    let abs = resolve_arg_path(arg);
    let data = format!("{}\n", content);
    match vfs::write_file(&abs, data.as_bytes()) {
        Ok(()) => print_tty(&format!("Wrote {} bytes to {}\n", data.len(), abs)),
        Err(e) => print_tty(&format!("write: {}: {:?}\n", abs, e)),
    }
}

fn cmd_mkdir(target: Option<&str>) {
    let Some(arg) = target else {
        print_tty("Usage: mkdir <dir>\n");
        return;
    };
    let abs = resolve_arg_path(arg);
    if let Err(e) = vfs::mkdir_abs(&abs, 0o755) {
        print_tty(&format!("mkdir: {}: {:?}\n", abs, e));
    }
}

fn cmd_rm(target: Option<&str>, remove_dir: bool) {
    let Some(arg) = target else {
        print_tty(if remove_dir {
            "Usage: rmdir <dir>\n"
        } else {
            "Usage: rm <file>\n"
        });
        return;
    };
    let abs = resolve_arg_path(arg);
    if let Err(e) = vfs::unlink_abs(&abs, remove_dir) {
        print_tty(&format!(
            "{}: {}: {:?}\n",
            if remove_dir { "rmdir" } else { "rm" },
            abs,
            e
        ));
    }
}

fn cmd_ln(target: Option<&str>, link: Option<&str>) {
    let (Some(t), Some(l)) = (target, link) else {
        print_tty("Usage: ln <target> <link>\n");
        return;
    };
    let abs_link = resolve_arg_path(l);
    if let Err(e) = vfs::symlink_abs(t, &abs_link) {
        print_tty(&format!("ln: {}: {:?}\n", abs_link, e));
    }
}

fn cmd_stat(target: Option<&str>) {
    let Some(arg) = target else {
        print_tty("Usage: stat <path>\n");
        return;
    };
    let abs = resolve_arg_path(arg);
    match vfs::lookup_abs(&abs, false) {
        Ok((inode, final_path)) => match inode.metadata() {
            Ok(m) => {
                print_tty(&format!("  File: {}\n", final_path));
                print_tty(&format!(
                    "  Size: {:<12} Blocks: {:<10} IO Block: {}\n",
                    m.size, m.blocks, m.blksize
                ));
                print_tty(&format!(
                    "  Type: {:<12?} Inode:  {:<10} Links:    {}\n",
                    m.kind, m.ino, m.nlink
                ));
                print_tty(&format!(
                    "Access: ({:#04o})       Uid: ({})          Gid: ({})\n",
                    m.mode, m.uid, m.gid
                ));
            }
            Err(e) => print_tty(&format!("stat: {}: {:?}\n", abs, e)),
        },
        Err(e) => print_tty(&format!("stat: {}: {:?}\n", abs, e)),
    }
}

fn cmd_mount(source: Option<&str>, target: Option<&str>, fstype: Option<&str>) {
    match (source, target, fstype) {
        (Some(src), Some(tgt), Some(fst)) => {
            let abs_tgt = resolve_arg_path(tgt);
            match vfs::mount_by_type(src, &abs_tgt, fst) {
                Ok(()) => print_tty(&format!("Mounted {} ({}) on {}\n", src, fst, abs_tgt)),
                Err(e) => print_tty(&format!("mount failed: {:?}\n", e)),
            }
        }
        _ => {
            for m in vfs::list_mounts() {
                print_tty(&format!(
                    "{} on {} type {} (rw)\n",
                    m.device, m.mount_point, m.fs_type
                ));
            }
        }
    }
}

fn cmd_umount(target: Option<&str>) {
    let Some(tgt) = target else {
        print_tty("Usage: umount <mount_point>\n");
        return;
    };
    let abs = resolve_arg_path(tgt);
    match vfs::umount(&abs) {
        Ok(()) => print_tty(&format!("Unmounted {}\n", abs)),
        Err(e) => print_tty(&format!("umount: {}: {:?}\n", abs, e)),
    }
}

fn cmd_df() {
    print_tty("FILESYSTEM      TYPE      1K-BLOCKS      USED AVAILABLE MOUNTED ON\n");
    print_tty("---------------+--------+----------+---------+---------+----------------\n");
    for m in vfs::list_mounts() {
        if let Ok(sfs) = m.fs.statfs() {
            let total_kb = (sfs.total_blocks * sfs.block_size) / 1024;
            let free_kb = (sfs.free_blocks * sfs.block_size) / 1024;
            let used_kb = total_kb.saturating_sub(free_kb);
            print_tty(&format!(
                "{:<15} {:<8} {:10} {:9} {:9} {}\n",
                m.device, m.fs_type, total_kb, used_kb, free_kb, m.mount_point
            ));
        }
    }
}

fn cmd_lsblk() {
    print_tty("NAME        TYPE   SECTORS   SIZE(KiB)  RO  MODEL / SERIAL\n");
    print_tty("-----------+------+---------+----------+----+--------------------------------\n");
    for dev in block::devices() {
        let info = dev.info();
        let size_kb = info.capacity_bytes() / 1024;
        print_tty(&format!(
            "{:<11} {:<6} {:9} {:10}  {}   {} {}\n",
            info.name,
            info.kind.as_str(),
            info.total_blocks,
            size_kb,
            if info.read_only { "1" } else { "0" },
            info.model,
            if info.serial.is_empty() {
                String::new()
            } else {
                format!("[{}]", info.serial)
            }
        ));
    }
    let cs = block::cache::stats();
    print_tty(&format!(
        "\nBlock Cache: hits={} misses={} writes={} flushes={} evictions={}\n",
        cs.hits, cs.misses, cs.writes, cs.flushes, cs.evictions
    ));
}

fn cmd_blkbench() {
    print_tty(
        "Running storage driver benchmark (512 KiB 4K-DMA + 64 KiB 512B-Random + Cache + Flush)...\n",
    );
    print_tty(
        "DEVICE    DRV   4K-READ(KiB/s) 4K-WRITE(KiB/s) 512B-R(IOPS) 512B-W(IOPS) CACHE-R(KiB/s) FLUSH(us)\n",
    );
    print_tty(
        "---------+-----+--------------+---------------+------------+------------+--------------+---------\n",
    );
    for r in block::bench::run_all() {
        print_tty(&format!(
            "{:<9} {:<5} {:14} {:15} {:12} {:12} {:14} {:9}\n",
            r.name,
            r.kind,
            r.seq_read_kib_s,
            r.seq_write_kib_s,
            r.rand_read_iops,
            r.rand_write_iops,
            r.cache_read_kib_s,
            r.flush_lat_us
        ));
    }
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
            sched::TaskState::Zombie => "Zombie  ",
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
        "SpaceKernel 0.2.0 ({arch_name}) SMP EEVDF VFS Linux-ABI #1 2026\n"
    ));
}

fn cmd_ps2() {
    let status = crate::hardware::ps2::status();
    print_tty("=== PS/2 Subsystem Status ===\n");
    print_tty(&format!(
        "Controller : present={} self_test={} dual_channel={} CCB={:#04x}\n",
        status.controller.present,
        status.controller.self_test_ok,
        status.controller.dual_channel,
        status.controller.config_byte
    ));
    print_tty(&format!(
        "Port 1 (KB): functional={}\n",
        status.controller.port1_ok
    ));
    let mods = crate::hardware::ps2::modifiers();
    print_tty(&format!(
        "Keyboard   : ready={} Shift={} Ctrl={} Alt={} CapsLock={} NumLock={} ScrollLock={}\n",
        status.keyboard_ok,
        mods.shift(),
        mods.ctrl(),
        mods.alt(),
        mods.caps_lock,
        mods.num_lock,
        mods.scroll_lock
    ));
    print_tty(&format!(
        "Port 2 (MS): functional={}\n",
        status.controller.port2_ok
    ));
    let mouse_st = crate::hardware::ps2::mouse_state();
    print_tty(&format!(
        "Mouse      : ready={} type={:?} ID={:#04x} reporting={}\n",
        status.mouse.present,
        status.mouse.mouse_type,
        status.mouse.device_id,
        status.mouse.reporting_enabled
    ));
    print_tty(&format!(
        "Cursor     : X={} Y={} Buttons(L={} R={} M={} 4={} 5={})\n",
        mouse_st.x,
        mouse_st.y,
        mouse_st.buttons.left,
        mouse_st.buttons.right,
        mouse_st.buttons.middle,
        mouse_st.buttons.btn4,
        mouse_st.buttons.btn5
    ));
}

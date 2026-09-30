/*
 *
 *       src/arch/x86_64/interrupts/handlers.rs
 *       Fault reporting is lock independent and does not allocate
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Fault reporting is lock independent and does not allocate.
use crate::{arch, printk};
use x86_64::{
    registers::control::Cr2,
    structures::idt::{InterruptStackFrame, PageFaultErrorCode},
};

fn fatal(name: &str, frame: InterruptStackFrame, code: Option<u64>) -> ! {
    arch::disable_interrupts();
    printk::emergency(format_args!("{name} code={code:?} frame={frame:#?}"));
    loop {
        arch::halt();
    }
}

macro_rules! fatal_no_code {
    ($name:ident, $label:literal) => {
        pub(super) extern "x86-interrupt" fn $name(frame: InterruptStackFrame) {
            fatal($label, frame, None)
        }
    };
}

macro_rules! fatal_with_code {
    ($name:ident, $label:literal) => {
        pub(super) extern "x86-interrupt" fn $name(frame: InterruptStackFrame, code: u64) {
            fatal($label, frame, Some(code))
        }
    };
}

fatal_no_code!(divide_error, "#DE divide error");
fatal_no_code!(bound_range, "#BR bound range");
fatal_no_code!(invalid_opcode, "#UD invalid opcode");
fatal_no_code!(device_not_available, "#NM device unavailable");
fatal_no_code!(x87_floating_point, "#MF floating point");
fatal_no_code!(simd_floating_point, "#XM SIMD floating point");
fatal_no_code!(virtualization, "#HV virtualization");
fatal_no_code!(unhandled_interrupt, "unhandled interrupt");

pub(super) extern "x86-interrupt" fn external<const VECTOR: u8>(_frame: InterruptStackFrame) {
    crate::arch::apic::dispatch(VECTOR);
    crate::sched::irq_exit();
}
fatal_with_code!(invalid_tss, "#TS invalid TSS");
fatal_with_code!(segment_not_present, "#NP segment absent");
fatal_with_code!(stack_segment_fault, "#SS stack segment");
fatal_with_code!(general_protection, "#GP general protection");
fatal_with_code!(alignment_check, "#AC alignment");
fatal_with_code!(cp_protection, "#CP control protection");

pub(super) extern "x86-interrupt" fn debug_trap(frame: InterruptStackFrame) {
    printk::emergency(format_args!("#DB debug trap frame={frame:#?}"));
}

pub(super) extern "x86-interrupt" fn breakpoint(frame: InterruptStackFrame) {
    printk::emergency(format_args!("#BP breakpoint frame={frame:#?}"));
}

pub(super) extern "x86-interrupt" fn overflow(frame: InterruptStackFrame) {
    printk::emergency(format_args!("#OF overflow frame={frame:#?}"));
}

pub(super) extern "x86-interrupt" fn nmi(frame: InterruptStackFrame) {
    printk::emergency(format_args!("NMI frame={frame:#?}"));
}

pub(super) extern "x86-interrupt" fn double_fault(frame: InterruptStackFrame, code: u64) -> ! {
    fatal("#DF double fault", frame, Some(code))
}

pub(super) extern "x86-interrupt" fn machine_check(frame: InterruptStackFrame) -> ! {
    fatal("#MC machine check", frame, None)
}

pub(super) extern "x86-interrupt" fn page_fault(
    frame: InterruptStackFrame,
    code: PageFaultErrorCode,
) {
    arch::disable_interrupts();
    printk::emergency(format_args!(
        "#PF address={:?} code={code:?} frame={frame:#?}",
        Cr2::read()
    ));
    loop {
        arch::halt();
    }
}

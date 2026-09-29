//! IDT policy and exception stack selection.
use super::{
    gdt::{DOUBLE_FAULT_IST, MACHINE_CHECK_IST, NMI_IST},
    handlers::*,
};
use x86_64::structures::idt::InterruptDescriptorTable;

pub(super) fn install(idt: &mut InterruptDescriptorTable) {
    idt.divide_error.set_handler_fn(divide_error);
    idt.debug.set_handler_fn(debug_trap);
    idt.non_maskable_interrupt.set_handler_fn(nmi);
    idt.breakpoint.set_handler_fn(breakpoint);
    idt.overflow.set_handler_fn(overflow);
    idt.bound_range_exceeded.set_handler_fn(bound_range);
    idt.invalid_opcode.set_handler_fn(invalid_opcode);
    idt.device_not_available
        .set_handler_fn(device_not_available);
    idt.double_fault.set_handler_fn(double_fault);
    idt.invalid_tss.set_handler_fn(invalid_tss);
    idt.segment_not_present.set_handler_fn(segment_not_present);
    idt.stack_segment_fault.set_handler_fn(stack_segment_fault);
    idt.general_protection_fault
        .set_handler_fn(general_protection);
    idt.page_fault.set_handler_fn(page_fault);
    idt.x87_floating_point.set_handler_fn(x87_floating_point);
    idt.alignment_check.set_handler_fn(alignment_check);
    idt.machine_check.set_handler_fn(machine_check);
    idt.simd_floating_point.set_handler_fn(simd_floating_point);
    idt.virtualization.set_handler_fn(virtualization);
    idt.cp_protection_exception.set_handler_fn(cp_protection);
    for vector in 32..=255 {
        idt[vector].set_handler_fn(unhandled_interrupt);
    }
    // SAFETY: each stack is 16-byte aligned, mapped, CPU-private and
    // owned by the corresponding table for its entire active lifetime.
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault)
            .set_stack_index(DOUBLE_FAULT_IST);
        idt.non_maskable_interrupt
            .set_handler_fn(nmi)
            .set_stack_index(NMI_IST);
        idt.machine_check
            .set_handler_fn(machine_check)
            .set_stack_index(MACHINE_CHECK_IST);
    }
}

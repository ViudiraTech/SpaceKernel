/*
 *
 *       src/arch/x86_64/interrupts/idt.rs
 *       IDT policy and exception stack selection
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! IDT policy and exception stack selection.
use super::{
    gdt::{DOUBLE_FAULT_IST, MACHINE_CHECK_IST, NMI_IST},
    handlers::*,
};
use x86_64::structures::{gdt::SegmentSelector, idt::InterruptDescriptorTable};

pub(super) fn install(idt: &mut InterruptDescriptorTable, code: SegmentSelector) {
    // SAFETY: code names the kernel code descriptor in this CPU's final GDT.
    // The bootloader's current CS may instead name our TSS after GDT reload.
    unsafe {
        idt.divide_error
            .set_handler_fn(divide_error)
            .set_code_selector(code);
        idt.debug.set_handler_fn(debug_trap).set_code_selector(code);
        idt.non_maskable_interrupt
            .set_handler_fn(nmi)
            .set_code_selector(code);
        idt.breakpoint
            .set_handler_fn(breakpoint)
            .set_code_selector(code);
        idt.overflow
            .set_handler_fn(overflow)
            .set_code_selector(code);
        idt.bound_range_exceeded
            .set_handler_fn(bound_range)
            .set_code_selector(code);
        idt.invalid_opcode
            .set_handler_fn(invalid_opcode)
            .set_code_selector(code);
        idt.device_not_available
            .set_handler_fn(device_not_available)
            .set_code_selector(code);
        idt.double_fault
            .set_handler_fn(double_fault)
            .set_code_selector(code);
        idt.invalid_tss
            .set_handler_fn(invalid_tss)
            .set_code_selector(code);
        idt.segment_not_present
            .set_handler_fn(segment_not_present)
            .set_code_selector(code);
        idt.stack_segment_fault
            .set_handler_fn(stack_segment_fault)
            .set_code_selector(code);
        idt.general_protection_fault
            .set_handler_fn(general_protection)
            .set_code_selector(code);
        idt.page_fault
            .set_handler_fn(page_fault)
            .set_code_selector(code);
        idt.x87_floating_point
            .set_handler_fn(x87_floating_point)
            .set_code_selector(code);
        idt.alignment_check
            .set_handler_fn(alignment_check)
            .set_code_selector(code);
        idt.machine_check
            .set_handler_fn(machine_check)
            .set_code_selector(code);
        idt.simd_floating_point
            .set_handler_fn(simd_floating_point)
            .set_code_selector(code);
        idt.virtualization
            .set_handler_fn(virtualization)
            .set_code_selector(code);
        idt.cp_protection_exception
            .set_handler_fn(cp_protection)
            .set_code_selector(code);
        macro_rules! install_external {
            ($($vector:literal),* $(,)?) => {
                $(idt[$vector].set_handler_fn(external::<$vector>).set_code_selector(code);)*
            };
        }
        install_external!(
            32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53,
            54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75,
            76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97,
            98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115,
            116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129, 130, 131, 132,
            133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148, 149,
            150, 151, 152, 153, 154, 155, 156, 157, 158, 159, 160, 161, 162, 163, 164, 165, 166,
            167, 168, 169, 170, 171, 172, 173, 174, 175, 176, 177, 178, 179, 180, 181, 182, 183,
            184, 185, 186, 187, 188, 189, 190, 191, 192, 193, 194, 195, 196, 197, 198, 199, 200,
            201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211, 212, 213, 214, 215, 216, 217,
            218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234,
            235, 236, 237, 238, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251,
            252, 253, 254, 255
        );
        // SAFETY: each stack is 16-byte aligned, mapped, CPU-private and
        // owned by the corresponding table for its entire active lifetime.
        {
            idt.double_fault
                .set_handler_fn(double_fault)
                .set_code_selector(code)
                .set_stack_index(DOUBLE_FAULT_IST);
            idt.non_maskable_interrupt
                .set_handler_fn(nmi)
                .set_code_selector(code)
                .set_stack_index(NMI_IST);
            idt.machine_check
                .set_handler_fn(machine_check)
                .set_code_selector(code)
                .set_stack_index(MACHINE_CHECK_IST);
        }
    }
}

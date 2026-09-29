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
    macro_rules! install_external {
        ($($vector:literal),* $(,)?) => {
            $(idt[$vector].set_handler_fn(external::<$vector>);)*
        };
    }
    install_external!(
        32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54,
        55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77,
        78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99,
        100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117,
        118, 119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135,
        136, 137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148, 149, 150, 151, 152, 153,
        154, 155, 156, 157, 158, 159, 160, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171,
        172, 173, 174, 175, 176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189,
        190, 191, 192, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207,
        208, 209, 210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223, 224, 225,
        226, 227, 228, 229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 242, 243,
        244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 255
    );
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

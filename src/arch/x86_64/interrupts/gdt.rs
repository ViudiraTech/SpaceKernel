//! CPU-local GDT, TSS and emergency stack ownership.
use alloc::boxed::Box;
use x86_64::{
    VirtAddr,
    instructions::{
        segmentation::{CS, DS, ES, SS, Segment},
        tables::load_tss,
    },
    structures::{
        gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
        idt::InterruptDescriptorTable,
        tss::TaskStateSegment,
    },
};

const STACK_SIZE: usize = 16 * 1024;
pub(super) const DOUBLE_FAULT_IST: u16 = 0;
pub(super) const NMI_IST: u16 = 1;
pub(super) const MACHINE_CHECK_IST: u16 = 2;

#[repr(align(16))]
struct Stack([u8; STACK_SIZE]);

impl Stack {
    fn new() -> Box<Self> {
        Box::new(Self([0; STACK_SIZE]))
    }
    fn top(&self) -> VirtAddr {
        VirtAddr::from_ptr(self.0.as_ptr().wrapping_add(STACK_SIZE))
    }
}

pub(super) struct CpuTables {
    gdt: GlobalDescriptorTable,
    idt: InterruptDescriptorTable,
    code: SegmentSelector,
    data: SegmentSelector,
    tss_selector: SegmentSelector,
    _kernel_stack: Box<Stack>,
    _double_fault_stack: Box<Stack>,
    _nmi_stack: Box<Stack>,
    _machine_check_stack: Box<Stack>,
}

impl CpuTables {
    pub(super) fn new() -> Box<Self> {
        let kernel_stack = Stack::new();
        let double_fault_stack = Stack::new();
        let nmi_stack = Stack::new();
        let machine_check_stack = Stack::new();
        let mut tss = Box::new(TaskStateSegment::new());
        tss.privilege_stack_table[0] = kernel_stack.top();
        tss.interrupt_stack_table[DOUBLE_FAULT_IST as usize] = double_fault_stack.top();
        tss.interrupt_stack_table[NMI_IST as usize] = nmi_stack.top();
        tss.interrupt_stack_table[MACHINE_CHECK_IST as usize] = machine_check_stack.top();
        // The TSS descriptor embeds its address, so it must remain pinned.
        let tss: &'static TaskStateSegment = Box::leak(tss);
        let mut gdt = GlobalDescriptorTable::new();
        let code = gdt.append(Descriptor::kernel_code_segment());
        let data = gdt.append(Descriptor::kernel_data_segment());
        gdt.append(Descriptor::user_code_segment());
        gdt.append(Descriptor::user_data_segment());
        let tss_selector = gdt.append(Descriptor::tss_segment(tss));
        let mut cpu = Box::new(Self {
            gdt,
            idt: InterruptDescriptorTable::new(),
            code,
            data,
            tss_selector,
            _kernel_stack: kernel_stack,
            _double_fault_stack: double_fault_stack,
            _nmi_stack: nmi_stack,
            _machine_check_stack: machine_check_stack,
        });
        super::idt::install(&mut cpu.idt);
        cpu
    }

    pub(super) fn load(&self) {
        // SAFETY: the descriptors and stacks were finalized before publication
        // and remain at stable addresses in permanent Box allocations.
        unsafe {
            self.gdt.load_unsafe();
            CS::set_reg(self.code);
            DS::set_reg(self.data);
            ES::set_reg(self.data);
            SS::set_reg(self.data);
            load_tss(self.tss_selector);
            self.idt.load_unsafe();
        }
    }
}

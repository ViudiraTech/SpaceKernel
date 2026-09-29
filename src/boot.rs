use limine::{
    BaseRevision, RequestsEndMarker, RequestsStartMarker,
    paging::PagingMode,
    request::{
        DtbRequest, ExecutableAddressRequest, ExecutableCmdlineRequest, FramebufferRequest,
        HhdmRequest, MemmapRequest, MpRequest, PagingModeRequest, RsdpRequest, StackSizeRequest,
        TscFrequencyRequest,
    },
};

#[cfg(target_arch = "x86_64")]
const MODE: PagingMode = PagingMode::X86_64_4LVL;
#[cfg(target_arch = "aarch64")]
const MODE: PagingMode = PagingMode::AARCH64_4LVL;
#[cfg(target_arch = "riscv64")]
const MODE: PagingMode = PagingMode::RISCV_SV48;

#[used]
#[unsafe(link_section = ".limine_requests_start")]
static START: RequestsStartMarker = RequestsStartMarker::new();
#[used]
#[unsafe(link_section = ".limine_requests")]
static REVISION: BaseRevision = BaseRevision::new();
#[used]
#[unsafe(link_section = ".limine_requests")]
static HHDM: HhdmRequest = HhdmRequest::new();
#[used]
#[unsafe(link_section = ".limine_requests")]
static MEMMAP: MemmapRequest = MemmapRequest::new();
#[used]
#[unsafe(link_section = ".limine_requests")]
static MP: MpRequest = MpRequest::new(0);
#[used]
#[unsafe(link_section = ".limine_requests")]
static ADDRESS: ExecutableAddressRequest = ExecutableAddressRequest::new();
#[used]
#[unsafe(link_section = ".limine_requests")]
static PAGING: PagingModeRequest = PagingModeRequest::new_exact(MODE);
#[used]
#[unsafe(link_section = ".limine_requests")]
static RSDP: RsdpRequest = RsdpRequest::new();
#[used]
#[unsafe(link_section = ".limine_requests")]
static DTB: DtbRequest = DtbRequest::new();
#[used]
#[unsafe(link_section = ".limine_requests")]
static CMDLINE: ExecutableCmdlineRequest = ExecutableCmdlineRequest::new();
#[used]
#[unsafe(link_section = ".limine_requests")]
static FRAMEBUFFER: FramebufferRequest = FramebufferRequest::new();
#[used]
#[unsafe(link_section = ".limine_requests")]
static COUNTER_FREQUENCY: TscFrequencyRequest = TscFrequencyRequest::new();
#[used]
#[unsafe(link_section = ".limine_requests")]
static STACK: StackSizeRequest = StackSizeRequest::new(1024 * 1024);
#[used]
#[unsafe(link_section = ".limine_requests_end")]
static END: RequestsEndMarker = RequestsEndMarker::new();

pub struct BootInfo {
    pub hhdm: u64,
    pub kernel_virtual: u64,
    pub kernel_physical: u64,
}

pub fn read() -> BootInfo {
    assert!(REVISION.is_supported(), "unsupported Limine base revision");
    assert!(
        PAGING.response().is_some(),
        "requested paging mode required"
    );
    let hhdm = HHDM.response().expect("HHDM required");
    let address = ADDRESS.response().expect("executable address required");
    assert!(MEMMAP.response().is_some(), "memory map required");
    assert!(MP.response().is_some(), "MP required");
    let _ = RSDP.response();
    let _ = DTB.response();
    let _ = STACK.response();
    BootInfo {
        hhdm: hhdm.offset,
        kernel_virtual: address.virtual_base,
        kernel_physical: address.physical_base,
    }
}

pub fn acpi_disabled() -> bool {
    cmdline()
        .split_ascii_whitespace()
        .any(|arg| arg == "acpi=none" || arg == "acpi=off")
}

pub fn rsdp_address() -> Option<usize> {
    if acpi_disabled() {
        return None;
    }
    RSDP.response().map(|response| response.address as usize)
}

pub fn dtb_address() -> Option<usize> {
    DTB.response().map(|response| response.dtb_ptr as usize)
}

pub fn memory_map() -> &'static [&'static limine::memmap::Entry] {
    MEMMAP.response().expect("memory map required").entries()
}

pub fn cmdline() -> &'static str {
    CMDLINE.response().map_or("", |response| response.cmdline())
}

pub fn framebuffer() -> Option<&'static limine::framebuffer::Framebuffer> {
    FRAMEBUFFER.response()?.framebuffers().first().copied()
}

pub fn counter_frequency() -> Option<u64> {
    COUNTER_FREQUENCY
        .response()
        .map(|response| response.frequency)
}

pub fn cpu_count() -> usize {
    MP.response().expect("MP required").cpus().len()
}

pub fn bsp_cpu_index() -> usize {
    let response = MP.response().expect("MP required");
    #[cfg(target_arch = "x86_64")]
    let index = response
        .cpus()
        .iter()
        .position(|cpu| cpu.lapic_id == response.bsp_lapic_id);
    #[cfg(target_arch = "aarch64")]
    let index = response
        .cpus()
        .iter()
        .position(|cpu| cpu.mpidr == response.bsp_mpidr);
    #[cfg(target_arch = "riscv64")]
    let index = response
        .cpus()
        .iter()
        .position(|cpu| cpu.hartid == response.bsp_hartid);
    index.expect("BSP absent from Limine CPU list")
}

use limine::request::{ExecutableAddressRequest, HhdmRequest, MemoryMapRequest};
use limine::request::{FramebufferRequest, StackSizeRequest};
use limine::BaseRevision;

#[used]
#[link_section = ".requests"]
pub static BASE_REVISION: BaseRevision = BaseRevision::with_revision(3);

#[used]
#[link_section = ".requests"]
pub static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[link_section = ".requests"]
pub static STACK_SIZE_REQUEST: StackSizeRequest = StackSizeRequest::new().with_size(512 * 1024);

#[used]
#[link_section = ".requests_start_marker"]
static _START_MARKER: [u64; 4] = [0xf9562b2d5c95a6c8, 0x6a7b384944536bdc, 0, 0];

#[used]
#[link_section = ".requests_end_marker"]
static _END_MARKER: [u64; 2] = [0xadc0e0531bb10d03, 0x9572709f31764c62];

#[used]
#[link_section = ".requests"]
pub static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[link_section = ".requests"]
pub static KERNEL_ADDR_REQUEST: ExecutableAddressRequest = ExecutableAddressRequest::new();

#[used]
#[link_section = ".requests"]
pub static MEMMAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

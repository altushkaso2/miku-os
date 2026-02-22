use super::pci::PciDevice;
use super::NetworkDriver;
use alloc::boxed::Box;

const TX_DESC_COUNT: usize = 4;
const RX_DESC_COUNT: usize = 4;
const BUF_SIZE: usize = 1536;

const DESC_OWN: u32 = 1 << 31;
const DESC_EOR: u32 = 1 << 30;
const DESC_FS: u32 = 1 << 29;
const DESC_LS: u32 = 1 << 28;

#[repr(C, align(16))]
#[derive(Copy, Clone)]
struct Desc {
    flags: u32,
    vlan: u32,
    buf_lo: u32,
    buf_hi: u32,
}

impl Desc {
    const fn zero() -> Self {
        Self { flags: 0, vlan: 0, buf_lo: 0, buf_hi: 0 }
    }
}

#[repr(align(256))]
struct TxRing([Desc; TX_DESC_COUNT]);
#[repr(align(256))]
struct RxRing([Desc; RX_DESC_COUNT]);

#[repr(align(16))]
struct TxBufs([[u8; BUF_SIZE]; TX_DESC_COUNT]);
#[repr(align(16))]
struct RxBufs([[u8; BUF_SIZE]; RX_DESC_COUNT]);

pub struct Rtl8168 {
    mmio_base: u64,
    pub mac: [u8; 6],
    tx_ring: Box<TxRing>,
    rx_ring: Box<RxRing>,
    tx_bufs: Box<TxBufs>,
    rx_bufs: Box<RxBufs>,
    tx_idx: usize,
    rx_idx: usize,
}

impl Rtl8168 {
    pub fn new(pci: &PciDevice) -> Option<Self> {
        pci.enable_bus_mastering();

        let mem_phys = pci.mem_bar(1).or_else(|| pci.mem_bar(0))?;
        if mem_phys == 0 {
            return None;
        }
        super::map_mmio(mem_phys, 0x1000);

        let hhdm = super::HHDM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let mmio_base = mem_phys + hhdm;

        let mut drv = Self {
            mmio_base,
            mac: [0; 6],
            tx_ring: Box::new(TxRing([Desc::zero(); TX_DESC_COUNT])),
            rx_ring: Box::new(RxRing([Desc::zero(); RX_DESC_COUNT])),
            tx_bufs: Box::new(TxBufs([[0u8; BUF_SIZE]; TX_DESC_COUNT])),
            rx_bufs: Box::new(RxBufs([[0u8; BUF_SIZE]; RX_DESC_COUNT])),
            tx_idx: 0,
            rx_idx: 0,
        };
        drv.init();
        Some(drv)
    }

    fn read8(&self, off: usize) -> u8 {
        unsafe { core::ptr::read_volatile((self.mmio_base + off as u64) as *const u8) }
    }
    fn read32(&self, off: usize) -> u32 {
        unsafe { core::ptr::read_volatile((self.mmio_base + off as u64) as *const u32) }
    }
    fn write8(&self, off: usize, v: u8) {
        unsafe { core::ptr::write_volatile((self.mmio_base + off as u64) as *mut u8, v) }
    }
    fn write16(&self, off: usize, v: u16) {
        unsafe { core::ptr::write_volatile((self.mmio_base + off as u64) as *mut u16, v) }
    }
    fn write32(&self, off: usize, v: u32) {
        unsafe { core::ptr::write_volatile((self.mmio_base + off as u64) as *mut u32, v) }
    }

    fn init(&mut self) {
        self.write8(0x52, 0x00);
        self.write8(0x37, 0x10);
        for _ in 0..1_000_000 {
            if self.read8(0x37) & 0x10 == 0 {
                break;
            }
            core::hint::spin_loop();
        }

        for i in 0..6 {
            self.mac[i] = self.read8(i);
        }

        let tx_phys = super::virt_to_phys(self.tx_ring.0.as_ptr() as u64);
        let rx_phys = super::virt_to_phys(self.rx_ring.0.as_ptr() as u64);

        for i in 0..TX_DESC_COUNT {
            let buf_phys = super::virt_to_phys(self.tx_bufs.0[i].as_ptr() as u64);
            let eor = if i == TX_DESC_COUNT - 1 { DESC_EOR } else { 0 };
            self.tx_ring.0[i] = Desc {
                flags: eor,
                vlan: 0,
                buf_lo: buf_phys as u32,
                buf_hi: (buf_phys >> 32) as u32,
            };
        }

        for i in 0..RX_DESC_COUNT {
            let buf_phys = super::virt_to_phys(self.rx_bufs.0[i].as_ptr() as u64);
            let eor = if i == RX_DESC_COUNT - 1 { DESC_EOR } else { 0 };
            self.rx_ring.0[i] = Desc {
                flags: DESC_OWN | eor | (BUF_SIZE as u32),
                vlan: 0,
                buf_lo: buf_phys as u32,
                buf_hi: (buf_phys >> 32) as u32,
            };
        }

        self.write32(0x20, tx_phys as u32);
        self.write32(0x24, (tx_phys >> 32) as u32);
        self.write32(0xE4, rx_phys as u32);
        self.write32(0xE8, (rx_phys >> 32) as u32);

        self.write32(0x40, 0x03000700);
        self.write32(0x44, 0x0000E70F);

        self.write16(0xDA, BUF_SIZE as u16);

        self.write16(0x3C, 0x0000);
        self.write16(0x3E, 0xFFFF);

        self.write8(0x37, 0x04 | 0x08);
    }

    pub fn driver_name() -> &'static str {
        "RTL8168 (Realtek Gigabit)"
    }
}

impl NetworkDriver for Rtl8168 {
    fn send(&mut self, data: &[u8]) -> bool {
        if data.len() > BUF_SIZE {
            return false;
        }
        let i = self.tx_idx;
        if self.tx_ring.0[i].flags & DESC_OWN != 0 {
            return false;
        }
        let eor = if i == TX_DESC_COUNT - 1 { DESC_EOR } else { 0 };
        let buf_phys = super::virt_to_phys(self.tx_bufs.0[i].as_ptr() as u64);
        self.tx_bufs.0[i][..data.len()].copy_from_slice(data);
        self.tx_ring.0[i] = Desc {
            flags: DESC_OWN | DESC_FS | DESC_LS | eor | (data.len() as u32),
            vlan: 0,
            buf_lo: buf_phys as u32,
            buf_hi: (buf_phys >> 32) as u32,
        };
        self.write8(0x38, 0x40);
        self.tx_idx = (i + 1) % TX_DESC_COUNT;
        true
    }

    fn recv(&mut self, handler: &mut dyn FnMut(&[u8])) {
        loop {
            let i = self.rx_idx;
            let flags = self.rx_ring.0[i].flags;
            if flags & DESC_OWN != 0 {
                break;
            }
            let len = (flags & 0x3FFF) as usize;
            if len > 4 && len <= BUF_SIZE {
                handler(&self.rx_bufs.0[i][..len - 4]);
            }
            let buf_phys = super::virt_to_phys(self.rx_bufs.0[i].as_ptr() as u64);
            let eor = if i == RX_DESC_COUNT - 1 { DESC_EOR } else { 0 };
            self.rx_ring.0[i] = Desc {
                flags: DESC_OWN | eor | (BUF_SIZE as u32),
                vlan: 0,
                buf_lo: buf_phys as u32,
                buf_hi: (buf_phys >> 32) as u32,
            };
            self.rx_idx = (i + 1) % RX_DESC_COUNT;
        }
        self.write16(0x3E, 0xFFFF);
    }

    fn has_packet(&self) -> bool {
        self.rx_ring.0[self.rx_idx].flags & DESC_OWN == 0
    }

    fn link_up(&self) -> bool {
        self.read8(0x6C) & 0x02 != 0
    }

    fn get_mac(&self) -> [u8; 6] {
        self.mac
    }
}

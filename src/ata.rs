use core::hint::spin_loop;
use core::sync::atomic::Ordering;
use x86_64::instructions::port::Port;

const STATUS_BSY: u8 = 0x80;
const STATUS_DRQ: u8 = 0x08;
const STATUS_ERR: u8 = 0x01;
const STATUS_DF:  u8 = 0x20;

const CMD_READ_PIO:    u8 = 0x20;
const CMD_WRITE_PIO:   u8 = 0x30;
const CMD_CACHE_FLUSH: u8 = 0xE7;

#[derive(Debug, Clone, Copy)]
pub enum AtaError {
    DeviceFault,
    ErrorBitSet,
    BufferTooSmall,
    NoDevice,
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AtaRole {
    Master,
    Slave,
}

pub struct AtaDrive {
    base_port: u16,
    role:      AtaRole,
}

impl AtaDrive {
    pub const EMPTY: Self = Self { base_port: 0, role: AtaRole::Master };

    pub const fn new(base_port: u16, role: AtaRole) -> Self {
        Self { base_port, role }
    }

    pub fn primary()         -> Self { Self::new(0x1F0, AtaRole::Master) }
    pub fn primary_slave()   -> Self { Self::new(0x1F0, AtaRole::Slave)  }
    pub fn secondary()       -> Self { Self::new(0x170, AtaRole::Master) }
    pub fn secondary_slave() -> Self { Self::new(0x170, AtaRole::Slave)  }

    pub fn from_idx(idx: usize) -> Self {
        match idx {
            0 => Self::primary(),
            1 => Self::primary_slave(),
            2 => Self::secondary(),
            _ => Self::secondary_slave(),
        }
    }

    fn device_select_byte(&self, lba_top: u8) -> u8 {
        let base = if self.role == AtaRole::Slave { 0xF0 } else { 0xE0 };
        base | (lba_top & 0x0F)
    }

    fn control_port(&self) -> u16 {
        self.base_port + 0x206
    }

    fn irq_flag(&self) -> &'static core::sync::atomic::AtomicBool {
        if self.base_port == 0x1F0 {
            &crate::interrupts::ATA_PRIMARY_IRQ
        } else {
            &crate::interrupts::ATA_SECONDARY_IRQ
        }
    }

    #[inline]
    unsafe fn delay_400ns(&self) {
        let alt = self.control_port();
        for _ in 0..4 {
            let _ = Port::<u8>::new(alt).read();
        }
    }

    unsafe fn wait_not_busy(&self) -> Result<u8, AtaError> {
        let status_port = self.base_port + 7;
        for _ in 0..50_000 {
            let s = Port::<u8>::new(status_port).read();
            if s & STATUS_BSY == 0 {
                return Ok(s);
            }
            spin_loop();
        }
        Err(AtaError::Timeout)
    }

    unsafe fn wait_drq(&self) -> Result<(), AtaError> {
        let status_port = self.base_port + 7;
        for _ in 0..50_000 {
            let s = Port::<u8>::new(status_port).read();
            if s & STATUS_BSY == 0 {
                if s & STATUS_DF  != 0 { return Err(AtaError::DeviceFault); }
                if s & STATUS_ERR != 0 { return Err(AtaError::ErrorBitSet); }
                if s & STATUS_DRQ != 0 { return Ok(()); }
            }
            spin_loop();
        }
        Err(AtaError::Timeout)
    }

    unsafe fn prepare_pio(&mut self, lba: u32, cmd: u8) -> Result<(), AtaError> {
        let bp = self.base_port;

        if Port::<u8>::new(bp + 7).read() == 0xFF {
            return Err(AtaError::NoDevice);
        }

        Port::<u8>::new(self.control_port()).write(0x02);
        self.wait_not_busy()?;

        Port::<u8>::new(bp + 6).write(self.device_select_byte((lba >> 24) as u8));
        self.delay_400ns();

        Port::<u8>::new(bp + 2).write(1);
        Port::<u8>::new(bp + 3).write(lba as u8);
        Port::<u8>::new(bp + 4).write((lba >> 8)  as u8);
        Port::<u8>::new(bp + 5).write((lba >> 16) as u8);
        Port::<u8>::new(bp + 7).write(cmd);

        self.delay_400ns();
        Ok(())
    }

    pub fn read_sector(&mut self, lba: u32, buffer: &mut [u8]) -> Result<(), AtaError> {
        if buffer.len() < 512 { return Err(AtaError::BufferTooSmall); }
        if self.base_port == 0 { return Err(AtaError::NoDevice); }
        unsafe { self.do_read_sector(lba, buffer) }
    }

    unsafe fn do_read_sector(&mut self, lba: u32, buffer: &mut [u8]) -> Result<(), AtaError> {
        self.prepare_pio(lba, CMD_READ_PIO)?;
        self.wait_drq()?;

        let mut data_port = Port::<u16>::new(self.base_port);
        for i in 0..256 {
            let data = data_port.read();
            buffer[i * 2]     = data as u8;
            buffer[i * 2 + 1] = (data >> 8) as u8;
        }

        Port::<u8>::new(self.control_port()).write(0x00);
        Ok(())
    }

    pub fn write_sector(&mut self, lba: u32, data: &[u8]) -> Result<(), AtaError> {
        if data.len() < 512 { return Err(AtaError::BufferTooSmall); }
        if self.base_port == 0 { return Err(AtaError::NoDevice); }
        unsafe { self.do_write_sector(lba, data) }
    }

    unsafe fn do_write_sector(&mut self, lba: u32, data: &[u8]) -> Result<(), AtaError> {
        self.prepare_pio(lba, CMD_WRITE_PIO)?;
        self.wait_drq()?;

        let mut data_port = Port::<u16>::new(self.base_port);
        for i in 0..256 {
            let word = (data[i * 2] as u16) | ((data[i * 2 + 1] as u16) << 8);
            data_port.write(word);
        }

        let status = self.wait_not_busy()?;
        Port::<u8>::new(self.control_port()).write(0x00);

        if status & STATUS_DF  != 0 { return Err(AtaError::DeviceFault); }
        if status & STATUS_ERR != 0 { return Err(AtaError::ErrorBitSet); }

        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), AtaError> {
        if self.base_port == 0 { return Ok(()); }

        unsafe {
            Port::<u8>::new(self.control_port()).write(0x02);
            Port::<u8>::new(self.base_port + 7).write(CMD_CACHE_FLUSH);

            self.delay_400ns();
            let status = self.wait_not_busy()?;
            Port::<u8>::new(self.control_port()).write(0x00);

            if status & STATUS_DF  != 0 { return Err(AtaError::DeviceFault); }
            if status & STATUS_ERR != 0 { return Err(AtaError::ErrorBitSet); }

            Ok(())
        }
    }
}

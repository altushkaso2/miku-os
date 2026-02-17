use crate::vfs::types::*;

pub struct DevFs;

impl DevFs {
    pub const fn new() -> Self {
        Self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DevType {
    Null = 0,
    Zero = 1,
    Random = 2,
    Console = 3,
}

impl DevType {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "null" => Some(Self::Null),
            "zero" => Some(Self::Zero),
            "random" | "urandom" => Some(Self::Random),
            "console" => Some(Self::Console),
            _ => None,
        }
    }

    pub fn major(&self) -> u8 {
        match self {
            Self::Null | Self::Zero | Self::Random => 1,
            Self::Console => 5,
        }
    }

    pub fn minor(&self) -> u8 {
        match self {
            Self::Null => 3,
            Self::Zero => 5,
            Self::Random => 8,
            Self::Console => 1,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Null => "null device (discards all)",
            Self::Zero => "zero device (reads zeros)",
            Self::Random => "pseudo-random generator",
            Self::Console => "system console",
        }
    }
}

use core::sync::atomic::{AtomicU32, Ordering};

static RANDOM_STATE: AtomicU32 = AtomicU32::new(0xDEADBEEF);

fn next_random() -> u8 {
    let mut state = RANDOM_STATE.load(Ordering::Relaxed);
    state ^= state << 13;
    state ^= state >> 17;
    state ^= state << 5;
    RANDOM_STATE.store(state, Ordering::Relaxed);
    state as u8
}

pub fn dev_read(dev_type: DevType, buf: &mut [u8], _offset: u64) -> VfsResult<usize> {
    match dev_type {
        DevType::Null => Ok(0),
        DevType::Zero => {
            let len = buf.len();
            for b in buf.iter_mut() {
                *b = 0;
            }
            Ok(len)
        }
        DevType::Random => {
            let len = buf.len();
            for b in buf.iter_mut() {
                *b = next_random();
            }
            Ok(len)
        }
        DevType::Console => Ok(0),
    }
}

pub fn dev_write(dev_type: DevType, buf: &[u8], _offset: u64) -> VfsResult<usize> {
    match dev_type {
        DevType::Null | DevType::Zero | DevType::Random => Ok(buf.len()),
        DevType::Console => {
            for &b in buf {
                if b >= 0x20 && b <= 0x7E {
                    crate::print!("{}", b as char);
                } else if b == b'\n' {
                    crate::println!();
                } else if b == b'\r' {
                } else if b == b'\t' {
                    crate::print!("    ");
                }
            }
            Ok(buf.len())
        }
    }
}

pub fn dev_type_from_node(major: u8, minor: u8) -> Option<DevType> {
    match (major, minor) {
        (1, 3) => Some(DevType::Null),
        (1, 5) => Some(DevType::Zero),
        (1, 8) => Some(DevType::Random),
        (5, 1) => Some(DevType::Console),
        _ => None,
    }
}

pub const DEV_ENTRIES: &[(&str, DevType)] = &[
    ("null", DevType::Null),
    ("zero", DevType::Zero),
    ("random", DevType::Random),
    ("urandom", DevType::Random),
    ("console", DevType::Console),
];

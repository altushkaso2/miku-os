extern crate alloc;
use alloc::vec;
use alloc::vec::Vec;
use crate::elf_loader::{self, LoadError};
use crate::vmm::AddressSpace;
use crate::process::Process;
use core::sync::atomic::Ordering;

const READ_CHUNK: usize = 4096;
const FRAME_R8_SLOT: usize = 7;

#[derive(Debug)]
pub enum ExecError {
    FsNotMounted,
    FileNotFound,
    NotRegularFile,
    IoError,
    Load(LoadError),
    NoAddressSpace,
    SpawnFailed,
}

impl ExecError {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FsNotMounted   => "filesystem not mounted",
            Self::FileNotFound   => "file not found",
            Self::NotRegularFile => "not a regular file",
            Self::IoError        => "I/O error",
            Self::Load(e)        => e.as_str(),
            Self::NoAddressSpace => "failed to create address space",
            Self::SpawnFailed    => "failed to spawn process",
        }
    }
}

pub fn exec(path: &str, args: &[&str]) -> Result<u64, ExecError> {
    let file_data = read_file_from_ext2(path)?;

    let aspace = AddressSpace::new_user().ok_or(ExecError::NoAddressSpace)?;

    let read_file = |interp_path: &str| -> Option<Vec<u8>> {
        if interp_path.contains("ld-miku") || interp_path.contains("ld.so") {
            return Some(crate::ldso::LDSO_BYTES.to_vec());
        }
        read_file_from_ext2(interp_path).ok()
    };

    let image = elf_loader::load(&file_data, &aspace, args, Some(&read_file))
        .map_err(ExecError::Load)?;

    crate::serial_println!(
        "[exec] loaded '{}': entry={:#x} sp={:#x} brk={:#x} bias={:#x} tls={:#x} interp={}",
        path, image.entry, image.stack_top, image.brk, image.load_bias,
        image.tls_base, image.has_interp,
    );

    let cr3 = aspace.into_raw();

    crate::mmap::vma_set_brk(cr3, image.brk);

    let mut proc = Process::new_elf(image.entry, image.stack_top, AddressSpace::from_raw(cr3))
        .ok_or_else(|| {
            AddressSpace::from_raw(cr3).free_address_space_manual();
            ExecError::SpawnFailed
        })?;

    if image.tls_base != 0 {
        let rsp = proc.rsp.load(Ordering::Relaxed);
        unsafe { (rsp as *mut u64).add(FRAME_R8_SLOT).write(image.tls_base); }
        crate::serial_println!("[exec] TLS base={:#x} → r8 in initial frame", image.tls_base);
    }

    let pid = proc.pid;
    crate::user_stdin::set_foreground(pid);
    crate::scheduler::add_user_process(proc);

    crate::serial_println!("[exec] spawned pid={} from '{}' argc={}", pid, path, args.len());
    Ok(pid)
}

fn read_file_from_ext2(path: &str) -> Result<Vec<u8>, ExecError> {
    use crate::commands::ext2_cmds::with_ext2_pub;
    use crate::miku_extfs::error::FsError;

    let result = with_ext2_pub(|fs| -> Result<Vec<u8>, FsError> {
        let ino   = fs.resolve_path(path)?;
        let inode = fs.read_inode(ino)?;
        if !inode.is_regular() { return Err(FsError::NotRegularFile); }
        let total = inode.size() as usize;
        if total == 0 { return Ok(Vec::new()); }
        let mut buf    = vec![0u8; total];
        let mut offset = 0usize;
        while offset < total {
            let chunk = READ_CHUNK.min(total - offset);
            let n = fs.read_file(&inode, offset as u64, &mut buf[offset..offset + chunk])?;
            if n == 0 { break; }
            offset += n;
        }
        Ok(buf)
    });

    match result {
        Some(Ok(data))                     => {
            crate::serial_println!("[exec] read {} bytes from '{}'", data.len(), path);
            Ok(data)
        }
        Some(Err(FsError::NotFound))       => Err(ExecError::FileNotFound),
        Some(Err(FsError::NotRegularFile)) => Err(ExecError::NotRegularFile),
        Some(Err(_))                       => Err(ExecError::IoError),
        None                               => Err(ExecError::FsNotMounted),
    }
}

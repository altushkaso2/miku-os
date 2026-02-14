use crate::vfs::types::*;

pub trait FsOps {
    fn fs_type(&self) -> FsType;
    fn read(&self, buf: &mut [u8], offset: u64, size: u64) -> VfsResult<usize>;
    fn write(&mut self, buf: &[u8], offset: u64) -> VfsResult<usize>;
    fn sync(&mut self) -> VfsResult<()>;
    fn statfs(&self) -> VfsResult<StatFs>;
}

pub trait VNodeOps {
    fn lookup(&self, name: &str) -> VfsResult<InodeId>;
    fn create(&mut self, name: &str, kind: VNodeKind, mode: FileMode) -> VfsResult<InodeId>;
    fn remove(&mut self, name: &str) -> VfsResult<()>;
    fn readdir(&self, offset: usize) -> VfsResult<Option<DirEntry>>;
}

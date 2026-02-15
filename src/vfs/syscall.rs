use crate::vfs::core::MikuVFS;
use crate::vfs::types::*;

pub struct SyscallInterface;

impl SyscallInterface {
    pub fn sys_open(
        vfs: &mut MikuVFS,
        cwd: usize,
        path: &str,
        flags: OpenFlags,
        mode: FileMode,
    ) -> VfsResult<usize> {
        vfs.open(cwd, path, flags, mode)
    }

    pub fn sys_read(vfs: &mut MikuVFS, fd: usize, buf: &mut [u8]) -> VfsResult<usize> {
        vfs.read(fd, buf)
    }

    pub fn sys_write(vfs: &mut MikuVFS, fd: usize, data: &[u8]) -> VfsResult<usize> {
        vfs.write(fd, data)
    }

    pub fn sys_close(vfs: &mut MikuVFS, fd: usize) -> VfsResult<()> {
        vfs.close(fd)
    }

    pub fn sys_seek(vfs: &mut MikuVFS, fd: usize, whence: SeekFrom) -> VfsResult<u64> {
        vfs.seek(fd, whence)
    }

    pub fn sys_stat(vfs: &mut MikuVFS, cwd: usize, path: &str) -> VfsResult<VNodeStat> {
        vfs.stat(cwd, path)
    }

    pub fn sys_lstat(vfs: &mut MikuVFS, cwd: usize, path: &str) -> VfsResult<VNodeStat> {
        vfs.lstat(cwd, path)
    }

    pub fn sys_fstat(vfs: &MikuVFS, fd: usize) -> VfsResult<VNodeStat> {
        vfs.fstat(fd)
    }

    pub fn sys_mkdir(
        vfs: &mut MikuVFS,
        cwd: usize,
        name: &str,
        mode: FileMode,
    ) -> VfsResult<usize> {
        vfs.mkdir(cwd, name, mode)
    }

    pub fn sys_unlink(vfs: &mut MikuVFS, cwd: usize, path: &str) -> VfsResult<()> {
        vfs.unlink(cwd, path)
    }

    pub fn sys_rmdir(vfs: &mut MikuVFS, cwd: usize, path: &str) -> VfsResult<()> {
        vfs.rmdir(cwd, path)
    }

    pub fn sys_rename(vfs: &mut MikuVFS, cwd: usize, old: &str, new: &str) -> VfsResult<()> {
        vfs.rename(cwd, old, new)
    }

    pub fn sys_symlink(
        vfs: &mut MikuVFS,
        parent: usize,
        name: &str,
        target: &str,
    ) -> VfsResult<usize> {
        vfs.symlink(parent, name, target)
    }

    pub fn sys_readlink(vfs: &MikuVFS, cwd: usize, path: &str) -> VfsResult<NameBuf> {
        vfs.readlink(cwd, path)
    }

    pub fn sys_link(
        vfs: &mut MikuVFS,
        cwd: usize,
        existing: &str,
        new_parent: usize,
        new_name: &str,
    ) -> VfsResult<()> {
        vfs.link(cwd, existing, new_parent, new_name)
    }

    pub fn sys_chmod(vfs: &mut MikuVFS, cwd: usize, path: &str, mode: FileMode) -> VfsResult<()> {
        vfs.chmod(cwd, path, mode)
    }

    pub fn sys_chown(
        vfs: &mut MikuVFS,
        cwd: usize,
        path: &str,
        uid: Option<u16>,
        gid: Option<u16>,
    ) -> VfsResult<()> {
        vfs.chown(cwd, path, uid, gid)
    }

    pub fn sys_dup(vfs: &mut MikuVFS, fd: usize) -> VfsResult<usize> {
        vfs.dup(fd)
    }

    pub fn sys_fsync(vfs: &mut MikuVFS, fd: usize) -> VfsResult<()> {
        vfs.fsync(fd)
    }

    pub fn sys_statfs(vfs: &MikuVFS, cwd: usize, path: &str) -> VfsResult<StatFs> {
        vfs.statfs(cwd, path)
    }
}

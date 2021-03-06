#![allow(dead_code)]

use crate::Syscall::Errors;
use alloc::sync::Arc;
use spin::Mutex;
use alloc::vec::Vec;
use alloc::string::String;
use core::any::Any;

const FTYPE_DIR:  u64 = 0o0040000; /* directory */
const FTYPE_CSPL: u64 = 0o0020000; /* character special */
const FTYPE_BSPL: u64 = 0o0060000; /* block special */
const FTYPE_REG:  u64 = 0o0100000; /* regular */
const FTYPE_SLNK: u64 = 0o0120000; /* symbolic link */
const FTYPE_SOCK: u64 = 0o0140000; /* socket */
const FTYPE_FIFO: u64 = 0o0010000; /* fifo */

#[repr(C)]
pub struct Metadata {
    pub device_id: u64,
    pub inode_id: i64,
    pub mode: i32,
    pub nlinks: i32,
    pub uid: u32,
    pub gid: u32,
    pub rdev: u64, // Device ID (optional)
    pub size: i64,
    pub atime: i64,
    pub reserved1: u64,
    pub mtime: i64,
    pub reserved2: u64,
    pub ctime: i64,
    pub reserved3: u64,
    pub blksize: i64,
    pub blocks: i64,
}

pub trait Inode: Any + Send + Sync {
    fn Stat(&self) -> Result<Metadata, i64> {
        Err(Errors::ENOSYS as i64)
    }

    fn GetName(&self) -> Result<&str, i64> {
        Err(Errors::ENOSYS as i64)
    }

    fn GetParent(&self) -> Option<Arc<dyn Inode>> {
        None
    }

    fn Read(&self, _offset: i64, _buffer: &mut [u8]) -> i64 {
        -(Errors::ENOSYS as i64)
    }

    fn Write(&self, _offset: i64, _buffer: &[u8]) -> i64 {
        -(Errors::ENOSYS as i64)
    }

    fn MMap(&self, _offset: i64, _size: usize, _flags: u8, _is_shared: bool) -> Result<&[u8],i64> {
        Err(Errors::ENODEV as i64)
    }

    fn Truncate(&self, _size: usize) -> i64 {
        -(Errors::ENOSYS as i64)
    }

    fn Creat(&self, _name: &str, _mode: i32) -> Result<Arc<dyn Inode>, i64> { // Creat, Mknod, & Mkdir merged into one system call
        Err(Errors::ENOSYS as i64)
    }

    fn Unlink(&self, _name: &str) -> i64 {
        Errors::ENOSYS as i64
    }

    fn Lookup(&self, _name: &str) -> Result<Arc<dyn Inode>, i64> {
        Err(Errors::ENOSYS as i64)
    }

    fn ReadDir(&self, _index: usize) -> Result<Option<Arc<dyn Inode>>, i64> {
        Err(Errors::ENOSYS as i64)
    }

    fn IOCtl(&self, _cmd: usize, _arg: usize) -> Result<usize, i64> {
        Err(Errors::ENOSYS as i64)
    }

    fn Open(&self, _mode: usize) -> Result<(), i64> {
        Ok(())
    }

    fn Close(&self) {}

    fn ChOwn(&self, _uid: i32, _gid: i32) -> i64 {
        Errors::ENOSYS as i64
    }

    fn ChMod(&self, _mode: i32) -> i64 {
        Errors::ENOSYS as i64
    }
}

pub trait Filesystem: Send + Sync {
    fn GetRootInode(&self) -> Arc<dyn Inode> {
        unimplemented!();
    }
    fn UMount(&self) -> i64 {
        Errors::ENOSYS as i64
    }
}

#[derive(Clone)]
pub struct FileDescriptor {
    pub inode: Arc<dyn Inode>,
    pub path: String,
    pub offset: i64,
    pub mode: usize,
    pub is_dir: bool,
    pub close_on_exec: bool,
}

static MOUNTS: Mutex<Vec<(String,Arc<dyn Filesystem>)>> = Mutex::new(Vec::new());

pub fn Mount(path: &str, filesystem: Arc<dyn Filesystem>) {
    let mut mlock = MOUNTS.lock();
    mlock.push((String::from(path),filesystem));
    drop(mlock);
}

pub fn FindMount(path: &str) -> Result<(usize,Arc<dyn Filesystem>),i64> {
    let mlock = MOUNTS.lock();
    let mut best: (Option<Arc<dyn Filesystem>>, usize, usize) = (None,0,0);
    for (i,(name,val)) in mlock.iter().enumerate() {
        if name.cmp(&String::from(path)) == core::cmp::Ordering::Equal {
            if best.2 < name.len() {
                best = (Some(val.clone()),i,name.len());
            }
        }
    }
    drop(mlock);
    return if best.0.is_none() {Err(Errors::ENOENT as i64)} else {Ok((best.1,best.0.unwrap()))};
}

pub fn UMount(name: &str) -> i64 {
    match FindMount(name) {
        Ok(fs) => {
            let result = fs.1.UMount();
            if result != 0 {
                return result
            }
            let mut mlock = MOUNTS.lock();
            mlock.remove(fs.0);
            drop(mlock);
            0
        }
        Err(e) => e
    }
}

pub fn LookupPath(path: &str) -> Result<Arc<dyn Inode>, i64> {
    let mut current = FindMount("/")?.1.GetRootInode();
    let path_seg: Vec<_> = path.split("/").filter(|e| *e != "" && *e != ".").collect();
    for (i,name) in path_seg.iter().enumerate() {
        match name {
            &"." => continue,
            &".." => {
                if let Some(parent) = current.GetParent() {
                    current = parent;
                }
            },
            _ => {
                match current.Lookup(name) {
                    Ok(entry) => current = entry,
                    Err(e) => return Err(e),
                }
                if current.Stat()?.mode & (FTYPE_DIR as i32) != 0 {
                    if let Ok(mount_point) = FindMount(["",path_seg[0..=i].join("/").as_str()].join("/").as_str()) {
                        current = mount_point.1.GetRootInode();
                    }
                }
            }
        }
    }
    Ok(current)
}

pub fn GetAbsPath(path: &str, cwd: &str) -> String {
    if !path.starts_with('/') {
        let path_str = [cwd,"/",path].join("");
        let mut full_path: Vec<_> = path_str.split("/").filter(|e| *e != "" && *e != ".").collect();
        let mut i = 0;
        while i < full_path.len() {
            if full_path[i] == ".." {
                full_path.drain((i-1)..=i);
                continue;
            }
            i += 1;
        }
        return [String::from("/"),full_path.join("/")].join("");
    }
    return String::from(path);
}

pub fn HasPermission(data: &Metadata, uid: u32, gid: u32, supgroups: &Vec<u32>, bits: usize) -> bool {
    if uid == 0 && bits & 1 == 0 {
        return true;
    }
    let mut my_permission = data.mode & 0b111;
    if data.uid == uid {my_permission = (data.mode & 0b111000000) >> 6;} else if data.gid == gid || supgroups.contains(&gid) {my_permission = (data.mode & 0b000111000) >> 3;}
    return my_permission as usize & bits == bits;
}

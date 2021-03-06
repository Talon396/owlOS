use crate::FS::VFS;
use crate::FS::DevFS;
use alloc::sync::{Arc, Weak};
use spin::{Once,Mutex};
use alloc::collections::{BTreeMap,VecDeque};
use alloc::string::{String,ToString};
use crate::Syscall::Errors;
use core::sync::atomic::{AtomicUsize,Ordering};

pub const TCGETS: usize = 0x4000;
pub const TCSETS: usize = 0x4001;
pub const TCSETSW: usize = 0x4002;
pub const TCSETSF: usize = 0x4003;

pub const TCGETA: usize = TCGETS;
pub const TCSETA: usize = TCSETS;

pub const TCSBRK: usize = 0x4004;
pub const TCXONC: usize = 0x4005;
pub const TCFLSH: usize = 0x4006;

pub const TIOCEXCL: usize = 0x4007;
pub const TIOCNXCL: usize = 0x4008;
pub const TIOCSCTTY: usize = 0x4009;
pub const TIOCGPGRP: usize = 0x400A;
pub const TIOCSPGRP: usize = 0x400B;
pub const TIOCOUTQ: usize = 0x400C;
pub const TIOCSTI: usize = 0x400D;
pub const TIOCGWINSZ: usize = 0x400E;
pub const TIOCSWINSZ: usize = 0x400F;
pub const TIOCMGET: usize = 0x4010;
pub const TIOCMBIS: usize = 0x4011;
pub const TIOCMBIC: usize = 0x4012;
pub const TIOCMSET: usize = 0x4013;
pub const TIOCGSOFTCAR: usize = 0x4014;
pub const TIOCSSOFTCAR: usize = 0x4015;

struct PtsDir(usize);
impl PtsDir {
    fn new() -> Arc<Self> {
        Arc::new(Self(DevFS::ReserveDeviceID()))
    }
}
impl DevFS::Device for PtsDir {
    fn DeviceID(&self) -> usize {
        self.0
    }
    fn Inode(&self) -> Arc<dyn VFS::Inode> {
        PTSDIR.get().expect("device not ready").clone()
    }
}
impl VFS::Inode for PtsDir {
    fn Stat(&self) -> Result<VFS::Metadata, i64> {
        Ok(VFS::Metadata {
            device_id: 0,
            inode_id: i64::MAX,
            mode: 0o0040555, // dr-xr-xr-x
            nlinks: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            size: 0,
            blksize: 0,
            blocks: 0,

            atime: unsafe {crate::UNIX_EPOCH as i64},
            mtime: unsafe {crate::UNIX_EPOCH as i64},
            ctime: unsafe {crate::UNIX_EPOCH as i64},
            reserved1: 0,
            reserved2: 0,
            reserved3: 0,
        })
    }

    fn GetName(&self) -> Result<&str, i64> {
        Ok("pts")
    }
    fn GetParent(&self) -> Option<Arc<dyn VFS::Inode>> {
        Some(VFS::FindMount("/dev").ok().unwrap().1.GetRootInode())
    }

    fn Lookup(&self, name: &str) -> Result<Arc<dyn VFS::Inode>, i64> {
        let lock = PTYS.lock();
        for i in lock.iter() {
            if i.1.client.GetName()? == name {
                return Ok(i.1.client.clone())
            }
        }
        drop(lock);
        Err(0)
    }

    fn ReadDir(&self, index: usize) -> Result<Option<Arc<dyn VFS::Inode>>, i64> {
        let lock = PTYS.lock();
        if index < lock.len() {
            drop(lock);
            return Ok(Some(PTYS.lock().get(&index).unwrap().client.clone()))
        }
        drop(lock);
        Ok(None)
    }

    fn Open(&self, _mode: usize) -> Result<(), i64> {
        Ok(())
    }

    fn Close(&self) {}
}

pub struct PTY {
    pub index: usize,
    pub index_str: String,
    pub client: Arc<dyn VFS::Inode>,
    pub server: Arc<dyn VFS::Inode>,

    // Client read, Server write
    pub pty_read: Mutex<VecDeque<u8>>,
    // Client write, Server read
    pub pty_write: Mutex<VecDeque<u8>>,
}
impl PTY {
    pub fn new(index: usize) -> Arc<Self> {
        let arc = Arc::new_cyclic(|x| {
            Self {
                index,
                index_str: index.to_string(),
                client: Arc::new(PTClient {p: x.clone()}),
                server: Arc::new(PTServer {p: x.clone()}),

                pty_read: Mutex::new(VecDeque::new()),
                pty_write: Mutex::new(VecDeque::new()),
            }
        });
        return arc;
    }
}
pub struct PTClient {
    p: Weak<PTY>,
}
impl VFS::Inode for PTClient {
    fn Stat(&self) -> Result<VFS::Metadata, i64> {
        Ok(VFS::Metadata {
            device_id: 0,
            inode_id: i64::MAX,
            mode: 0o0020666, // crw-rw-rw-
            nlinks: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            size: 0,
            blksize: 0,
            blocks: 0,

            atime: unsafe {crate::UNIX_EPOCH as i64},
            mtime: unsafe {crate::UNIX_EPOCH as i64},
            ctime: unsafe {crate::UNIX_EPOCH as i64},
            reserved1: 0,
            reserved2: 0,
            reserved3: 0,
        })
    }
    fn GetName(&self) -> Result<&str, i64> {
        if let Some(arc) = self.p.upgrade() {
            let str_size = arc.index_str.len();
            let ptr = arc.index_str.as_str().as_ptr();
            let slice = unsafe {alloc::slice::from_raw_parts(ptr,str_size)};
            drop(arc);
            return Ok(alloc::str::from_utf8(slice).ok().unwrap())
        }
        Err(Errors::EPIPE as i64)
    }
    fn Read(&self, _offset: i64, buffer: &mut [u8]) -> i64 {
        if let Some(arc) = self.p.upgrade() {
            let mut i = 0;
            let mut lock = arc.pty_read.lock();
            if lock.len() == 0 {
                drop(lock);
                drop(arc);
                return -(Errors::EAGAIN as i64);
            }
            while i < buffer.len() && lock.len() > 0 {
                buffer[i] = lock.pop_front().unwrap();
                i += 1;
            }
            drop(lock);
            drop(arc);
            return i as i64;
        }
        return -(Errors::EPIPE as i64);
    }
    fn Write(&self, _offset: i64, buffer: &[u8]) -> i64 {
        if let Some(arc) = self.p.upgrade() {
            let mut i = 0;
            let mut lock = arc.pty_write.lock();
            while i < buffer.len() {
                lock.push_back(buffer[i]);
                i += 1;
            }
            drop(lock);
            drop(arc);
            return i as i64;
        }
        return -(Errors::EPIPE as i64);
    }
    fn IOCtl(&self, cmd: usize, _arg: usize) -> Result<usize, i64> {
        match cmd {
            
            _ => {}
        }
        Ok(0)
    }
}
pub struct PTServer {
    p: Weak<PTY>,
}
impl VFS::Inode for PTServer {
    fn Read(&self, _offset: i64, buffer: &mut [u8]) -> i64 {
        if let Some(arc) = self.p.upgrade() {
            let mut i = 0;
            let mut lock = arc.pty_write.lock();
            if lock.len() == 0 {
                drop(lock);
                //return -(Errors::EAGAIN as i64);
                return 0;
            }
            while i < buffer.len() && lock.len() > 0 {
                buffer[i] = lock.pop_front().unwrap();
                i += 1;
            }
            drop(lock);
            return i as i64;
        }
        return -(Errors::EPIPE as i64);
    }
    fn Write(&self, _offset: i64, buffer: &[u8]) -> i64 {
        if let Some(arc) = self.p.upgrade() {
            let mut i = 0;
            let mut lock = arc.pty_read.lock();
            while i < buffer.len() {
                lock.push_back(buffer[i]);
                i += 1;
            }
            drop(lock);
            return i as i64;
        }
        return -(Errors::EPIPE as i64);
    }
}
struct PtmxDev(usize);
impl PtmxDev {
    fn new() -> Arc<Self> {
        Arc::new(PtmxDev(DevFS::ReserveDeviceID()))
    }
}
impl VFS::Inode for PtmxDev {
    fn Stat(&self) -> Result<VFS::Metadata, i64> {
        Ok(VFS::Metadata {
            device_id: 0,
            inode_id: i64::MAX,
            mode: 0o0020666, // crw-rw-rw-
            nlinks: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            size: 0,
            blksize: 0,
            blocks: 0,

            atime: unsafe {crate::UNIX_EPOCH as i64},
            mtime: unsafe {crate::UNIX_EPOCH as i64},
            ctime: unsafe {crate::UNIX_EPOCH as i64},
            reserved1: 0,
            reserved2: 0,
            reserved3: 0,
        })
    }
    fn GetName(&self) -> Result<&str, i64> {
        Ok("ptmx")
    }
    fn Open(&self, _mode: usize) -> Result<(), i64> {
        panic!("You shouldn't be seeing this");
    }
}
impl DevFS::Device for PtmxDev {
    fn DeviceID(&self) -> usize {
        self.0
    }
    fn Inode(&self) -> Arc<dyn VFS::Inode> {
        Arc::new(Ptmx(AtomicUsize::new(usize::MAX)))
    }
}
struct Ptmx(AtomicUsize);
impl Drop for Ptmx {
    fn drop(&mut self) {
        if self.0.load(Ordering::SeqCst) != usize::MAX {
            DestroyPTY(self.0.load(Ordering::SeqCst));
            self.0.store(usize::MAX,Ordering::SeqCst);
        }
    }
}
impl VFS::Inode for Ptmx {
    fn Stat(&self) -> Result<VFS::Metadata, i64> {
        Ok(VFS::Metadata {
            device_id: 0,
            inode_id: i64::MAX,
            mode: 0o0020666, // crw-rw-rw-
            nlinks: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            size: 0,
            blksize: 0,
            blocks: 0,

            atime: unsafe {crate::UNIX_EPOCH as i64},
            mtime: unsafe {crate::UNIX_EPOCH as i64},
            ctime: unsafe {crate::UNIX_EPOCH as i64},
            reserved1: 0,
            reserved2: 0,
            reserved3: 0,
        })
    }
    fn GetName(&self) -> Result<&str, i64> {
        Ok("ptmx")
    }
    fn Open(&self, _mode: usize) -> Result<(), i64> {
        self.0.store(AddPTY(),Ordering::SeqCst);
        Ok(())
    }

    fn Close(&self) {
        
    }

    fn Read(&self, offset: i64, buffer: &mut [u8]) -> i64 {
        if self.0.load(Ordering::SeqCst) == usize::MAX {
            return -(Errors::EACCES as i64);
        }
        let lock = PTYS.lock();
        let ret = lock.get(&self.0.load(Ordering::SeqCst)).unwrap().server.Read(offset,buffer);
        drop(lock);
        ret
    }
    fn Write(&self, offset: i64, buffer: &[u8]) -> i64 {
        if self.0.load(Ordering::SeqCst) == usize::MAX {
            return -(Errors::EACCES as i64);
        }
        let lock = PTYS.lock();
        let ret = lock.get(&self.0.load(Ordering::SeqCst)).unwrap().server.Write(offset,buffer);
        drop(lock);
        ret
    }
    fn IOCtl(&self, cmd: usize, _arg: usize) -> Result<usize, i64> {
        match cmd {
            0x4F00 => {
                // This is a special Fox Kernel specific request that returns the index of the PTY.
                return Ok(self.0.load(Ordering::SeqCst));
            }
            0x4F02 => { // TTYLOGIN
                todo!();
            }
            _ => {}
        }
        Ok(0)
    }
}

#[allow(deref_nullptr)]
pub fn AddPTY() -> usize {
    let mut lock = PTYS.lock();
    let len = lock.keys().last().unwrap_or_else(|| &0) + 1;
    for i in 0..=len {
        if !(lock.contains_key(&i)) {
            lock.insert(i,PTY::new(i));
            drop(lock);
            if PTYS.is_locked() {
                panic!("PTY Deadlock!");
            }
            return i;
        }
    }
    drop(lock);
    unimplemented!();
}

pub fn DestroyPTY(index: usize) {
    let mut lock = PTYS.lock();
    lock.remove(&index);
    drop(lock);
}

static PTSDIR: Once<Arc<PtsDir>> = Once::new();
static PTYS: Mutex<BTreeMap<usize, Arc<PTY>>> = Mutex::new(BTreeMap::new());
static PTMXDEV: Once<Arc<PtmxDev>> = Once::new();

pub fn Initalize() {
    DevFS::InstallDevice(PTSDIR.call_once(|| PtsDir::new()).clone());
    DevFS::InstallDevice(PTMXDEV.call_once(|| PtmxDev::new()).clone());
}
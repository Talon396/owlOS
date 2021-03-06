#![allow(non_snake_case,unused_must_use,non_upper_case_globals,non_camel_case_types)]
#![no_std]
#![feature(lang_items,int_roundings,allow_internal_unstable,const_mut_refs,start)]

use core::panic::PanicInfo;

extern crate alloc;

pub mod syscall;
pub mod io;
pub mod allocator;
pub mod file;
pub mod errors;
pub mod process;
pub mod sys;
pub mod env;

#[macro_use]
pub mod macros;

#[cfg(target_arch="x86_64")]
#[path = "arch/AMD64.rs"]
pub mod arch;

////////////////////////////////////////////////
#[repr(C)]
pub struct Stat {
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
////////////////////////////////////////////////
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    print!("owlOS Programmer API: Rust Runtime Panic!\n{:?}\n", info);
    syscall::exit(255);
}

#[lang = "eh_personality"]
#[doc(hidden)]
extern "C" fn eh_personality() {}

pub trait ExitCode {
    fn GetCode(&self) -> u8 {unimplemented!();}
}

extern "Rust" {
    fn main() -> u8;
}

#[doc(hidden)]
#[no_mangle]
extern "C" fn _start(argc: isize, argv: *const *const u8, envp: *const *const u8) -> ! {
    syscall::exit(unsafe {main()});
}


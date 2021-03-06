use spin::Mutex;
use log::{Record, Metadata, Level};

pub static WRITER: Mutex<Writer> = Mutex::new(Writer {cursor_x:0,cursor_y:0,text_color: 0xFFFFFF});

pub struct Writer {
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub text_color: u32,
}

struct KernelLogger;

impl log::Log for KernelLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        match record.level() {
            Level::Trace => {},
            Level::Debug => crate::print!("{}:{} \x1b[37m\x1b[2mdebug\x1b[0m {}\n", record.file().unwrap(), record.line().unwrap(), record.args()),
            Level::Info => crate::print!("{}:{} \x1b[1m\x1b[34minfo\x1b[0m {}\n", record.file().unwrap(), record.line().unwrap(), record.args()),
            Level::Warn => crate::print!("{}:{} \x1b[35mwarn\x1b[0m {}\n", record.file().unwrap(), record.line().unwrap(), record.args()),
            Level::Error => crate::print!("{}:{} \x1b[31merror\x1b[0m {}\n", record.file().unwrap(), record.line().unwrap(), record.args()),
        }
    }

    fn flush(&self) {}
}

static LOGGER: KernelLogger = KernelLogger;
pub static mut QUIET: bool = false;
pub static mut NO_COLOR: bool = true;

impl core::fmt::Write for Writer {
    #[cfg(any(target_arch="x86_64"))]
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        use limine::*;
        static mut CACHED: Option<&'static LimineTerminalResponse> = None;
        crate::arch::UART::write_serial(s);
        unsafe {if QUIET {return Ok(());}}
        unsafe {
            if let Some(writer) = CACHED {
                let terminal = writer.terminals().unwrap().first().unwrap();
                writer.write().unwrap()(terminal, s);
            } else {
                let response = crate::arch::TERMINAL.get_response().get().unwrap();
                let terminal = response.terminals().unwrap().first().unwrap();
                let writer = response.write().unwrap();

                writer(&terminal, s);

                CACHED = Some(response);
            }
        }
        Ok(())
    }
    #[cfg(not(target_arch="x86_64"))]
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        use crate::Framebuffer::MainFramebuffer;
        use alloc::vec::Vec;
        crate::arch::UART::write_serial(s);
        unsafe {if QUIET {return Ok(());}}
        let mut lock = MainFramebuffer.lock();
        if lock.is_some() {
            let fb = lock.as_mut().unwrap();
            let mut ansi_seq: Vec<u8> = Vec::new();
            let mut parse_ansi = false;
            let console_height = fb.height.div_floor(16) * 16;
            for b in s.bytes() {
                if self.cursor_x*8 >= fb.width.div_floor(8)*8 || b == b'\n' {
                    self.cursor_x = 0;
                    if self.cursor_y*16 >= console_height-16 {
                        self.cursor_y = 0;
                    } else {
                        self.cursor_y += 1;
                    }
                    fb.DrawRect(0, self.cursor_y*16, fb.width, 16, 0x000000);
                }
                if b >= 32 && b < 127 {
                    if !parse_ansi {
                        fb.DrawSymbol(self.cursor_x*8, self.cursor_y*16, b, self.text_color, 1);
                        self.cursor_x += 1;
                    } else {
                        if b == b'm' {
                            if ansi_seq.len() == 3 {
                                if ansi_seq[1] == b'3' {
                                           if ansi_seq[2] == b'1' {
                                        self.text_color = 0xCC0000;
                                    } else if ansi_seq[2] == b'2' {
                                        self.text_color = 0x4E9A06;
                                    } else if ansi_seq[2] == b'3' {
                                        self.text_color = 0xC4A000;
                                    } else if ansi_seq[2] == b'4' {
                                        self.text_color = 0x3465A4;
                                    } else if ansi_seq[2] == b'5' {
                                        self.text_color = 0x75507B;
                                    } else if ansi_seq[2] == b'6' {
                                        self.text_color = 0x06989A;
                                    } else if ansi_seq[2] == b'7' {
                                        self.text_color = 0xFFFFFF;
                                    }
                                }
                            } else if ansi_seq.len() == 2 {
                                if ansi_seq[1] == b'0' {
                                    self.text_color = 0xFFFFFF;
                                } else if ansi_seq[1] == b'2' {
                                    self.text_color = 0x7F7F7F;
                                }
                            }
                            unsafe {if NO_COLOR {self.text_color = 0xFFFFFF;}}
                            parse_ansi = false;
                            ansi_seq.clear();
                        } else {
                            ansi_seq.push(b);
                        }
                    }
                } else if b == 0x1b {
                    parse_ansi = true;
                }
            }
        }
        drop(lock);
        Ok(())
    }
}

#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    use core::fmt::Write;
    WRITER.lock().write_fmt(args).unwrap();
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => (crate::Console::_print(format_args!($($arg)*)));
}

pub fn Initalize() {
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(log::LevelFilter::Trace));
}
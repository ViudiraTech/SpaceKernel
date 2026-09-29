//! Bounded POSIX-style canonical and raw input processing.

const INPUT_CAPACITY: usize = 4096;

#[derive(Clone, Copy, Debug)]
pub struct Termios {
    pub canonical: bool,
    pub echo: bool,
    pub signals: bool,
    pub icrnl: bool,
    pub erase: u8,
    pub kill: u8,
    pub eof: u8,
    pub interrupt: u8,
}

impl Default for Termios {
    fn default() -> Self {
        Self {
            canonical: true,
            echo: true,
            signals: true,
            icrnl: true,
            erase: 0x7f,
            kill: 0x15,
            eof: 0x04,
            interrupt: 0x03,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputEvent {
    None,
    Echo { bytes: [u8; 3], length: usize },
    LineReady,
    Interrupt,
    Overflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    WouldBlock,
    BufferFull,
    NoDevice,
}

mod discipline;
pub use discipline::LineDiscipline;

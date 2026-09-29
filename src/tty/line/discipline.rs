//! Per-device bounded canonical and raw input queues.

use super::{INPUT_CAPACITY, InputEvent, ReadError, Termios};

/// A line discipline is owned by one TTY. Its caller must synchronize feed,
/// read and termios changes; no callback or allocator runs inside this type.
pub struct LineDiscipline {
    termios: Termios,
    pending: [u8; INPUT_CAPACITY],
    pending_len: usize,
    ready: [u8; INPUT_CAPACITY],
    boundary: [bool; INPUT_CAPACITY],
    head: usize,
    ready_len: usize,
    eof_pending: bool,
}

impl LineDiscipline {
    pub const fn new() -> Self {
        Self {
            termios: Termios {
                canonical: true,
                echo: true,
                signals: true,
                icrnl: true,
                erase: 0x7f,
                kill: 0x15,
                eof: 0x04,
                interrupt: 0x03,
            },
            pending: [0; INPUT_CAPACITY],
            pending_len: 0,
            ready: [0; INPUT_CAPACITY],
            boundary: [false; INPUT_CAPACITY],
            head: 0,
            ready_len: 0,
            eof_pending: false,
        }
    }

    pub fn termios(&self) -> Termios {
        self.termios
    }

    pub fn set_termios(&mut self, settings: Termios) -> Result<(), ReadError> {
        if self.termios.canonical && !settings.canonical {
            if !self.commit() {
                return Err(ReadError::BufferFull);
            }
        }
        self.termios = settings;
        Ok(())
    }

    pub fn feed(&mut self, mut byte: u8) -> InputEvent {
        if self.termios.icrnl && byte == b'\r' {
            byte = b'\n';
        }
        if self.termios.signals && byte == self.termios.interrupt {
            self.pending_len = 0;
            return InputEvent::Interrupt;
        }
        if !self.termios.canonical {
            if !self.push(byte) {
                return InputEvent::Overflow;
            }
            return self.echo(byte);
        }
        if byte == self.termios.erase {
            if self.pending_len == 0 {
                return InputEvent::None;
            }
            self.pending_len -= 1;
            while self.pending_len > 0 && self.pending[self.pending_len] & 0xc0 == 0x80 {
                self.pending_len -= 1;
            }
            return if self.termios.echo {
                InputEvent::Echo {
                    bytes: [8, b' ', 8],
                    length: 3,
                }
            } else {
                InputEvent::None
            };
        }
        if byte == self.termios.kill {
            self.pending_len = 0;
            return if self.termios.echo {
                InputEvent::Echo {
                    bytes: [b'^', b'U', b'\n'],
                    length: 3,
                }
            } else {
                InputEvent::None
            };
        }
        if byte == self.termios.eof {
            if !self.commit() {
                return InputEvent::Overflow;
            }
            if self.ready_len == 0 {
                self.eof_pending = true;
            }
            return InputEvent::LineReady;
        }
        if self.pending_len == INPUT_CAPACITY
            || (byte == b'\n' && self.pending_len + 1 > INPUT_CAPACITY - self.ready_len)
        {
            return InputEvent::Overflow;
        }
        self.pending[self.pending_len] = byte;
        self.pending_len += 1;
        if byte == b'\n' {
            assert!(self.commit());
            return InputEvent::LineReady;
        }
        self.echo(byte)
    }

    pub fn read(&mut self, output: &mut [u8]) -> Result<usize, ReadError> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.ready_len == 0 {
            if self.eof_pending {
                self.eof_pending = false;
                return Ok(0);
            }
            return Err(ReadError::WouldBlock);
        }
        let limit = output.len().min(self.ready_len);
        let mut count = 0;
        for slot in &mut output[..limit] {
            *slot = self.ready[self.head];
            let boundary = self.boundary[self.head];
            self.boundary[self.head] = false;
            self.head = (self.head + 1) % INPUT_CAPACITY;
            count += 1;
            if self.termios.canonical && boundary {
                break;
            }
        }
        self.ready_len -= count;
        Ok(count)
    }

    fn commit(&mut self) -> bool {
        if self.pending_len > INPUT_CAPACITY - self.ready_len {
            return false;
        }
        for index in 0..self.pending_len {
            let byte = self.pending[index];
            assert!(self.push(byte));
        }
        if self.pending_len != 0 {
            let last = (self.head + self.ready_len - 1) % INPUT_CAPACITY;
            self.boundary[last] = true;
        }
        self.pending_len = 0;
        true
    }

    fn push(&mut self, byte: u8) -> bool {
        if self.ready_len == INPUT_CAPACITY {
            return false;
        }
        let tail = (self.head + self.ready_len) % INPUT_CAPACITY;
        self.ready[tail] = byte;
        self.ready_len += 1;
        true
    }

    fn echo(&self, byte: u8) -> InputEvent {
        if self.termios.echo {
            InputEvent::Echo {
                bytes: [byte, 0, 0],
                length: 1,
            }
        } else {
            InputEvent::None
        }
    }
}

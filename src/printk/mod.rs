//! Allocation-free kernel logging with bounded storage and ordered output.

mod drain;
mod ring;

pub use drain::emergency;
pub use ring::{next_sequence, read, record};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
}

impl Level {
    const fn label(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
        }
    }
}

#[macro_export]
macro_rules! kinfo {
    ($($arg:tt)*) => { $crate::printk::record($crate::printk::Level::Info, format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! kwarn {
    ($($arg:tt)*) => { $crate::printk::record($crate::printk::Level::Warn, format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! kerror {
    ($($arg:tt)*) => { $crate::printk::record($crate::printk::Level::Error, format_args!($($arg)*)) };
}

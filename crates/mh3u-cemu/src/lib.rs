//! Reading the memory of a running Cemu, to compare what the port does with what the emulator does. Linux only.

#[cfg(target_os = "linux")]
pub mod procmem;

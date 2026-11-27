#[cfg(windows)]
pub mod win;

#[cfg(windows)]
pub use win::*;

#[cfg(not(windows))]
pub mod fallback;

#[cfg(not(windows))]
pub use fallback::*;

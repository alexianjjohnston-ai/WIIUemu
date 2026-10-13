//! Win32 calls for Wii U Connect's desktop features, behind safe functions. Empty on other platforms.
//!
//! Handles cross this API as `u64` so callers never touch raw pointers.
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(windows)]
pub mod tv;

#[cfg(windows)]
#[doc(hidden)]
pub mod test_support;

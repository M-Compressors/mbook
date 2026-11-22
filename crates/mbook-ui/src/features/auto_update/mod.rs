//! Updates from the releases of the repository (RFC 0004, "Updates").
//!
//! [`check`] asks the releases for a version newer than the running one;
//! [`install`] downloads its archive, verifies the signature against the key
//! built into the application and only then installs it. Both block on the
//! network and the file system: call them on the background executor.

mod banner;
mod check;
mod error;
#[cfg(test)]
mod fixtures;
mod install;
#[cfg(any(target_os = "macos", target_os = "windows", test))]
mod verify;

pub(crate) use banner::UpdateBanner;

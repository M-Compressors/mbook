//! The file format end to end: reading, writing, and files that break the
//! rules of RFC 0001.

#[cfg(test)]
#[cfg(feature = "write")]
mod checking;
#[cfg(test)]
#[cfg(feature = "read")]
mod conformance;
#[cfg(test)]
#[cfg(feature = "read")]
mod raw;
#[cfg(test)]
#[cfg(feature = "read")]
mod reading;
#[cfg(test)]
#[cfg(feature = "write")]
mod sample;
#[cfg(test)]
mod texts;
#[cfg(test)]
#[cfg(all(feature = "read", feature = "write"))]
mod writing;

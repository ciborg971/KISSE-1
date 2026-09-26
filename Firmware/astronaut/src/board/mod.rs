//! Board-specific wiring. The display, panel init and UI in [`crate::display`] are shared.

#[cfg(feature = "astronaut")]
pub mod astronaut;
#[cfg(feature = "qualia")]
pub mod qualia;

//! The vario tone.
//!
//! * [`tone`] decides *what* should be heard for a climb rate: silence, beeps or a
//!   continuous sink tone.
//! * [`synth`] turns that into audio samples without clicks.
//! * [`alert`] makes the warning sounds (airspace).
//! * [`voice`] connects the two and can be controlled from another thread; it mixes the
//!   warnings over the vario tone.
//! * [`output`] plays a voice on the sound card (needs the `output` feature).
//! * [`wav`] writes samples to a file, for listening later and for tests.
//!
//! Everything except [`output`] is plain arithmetic without any operating system
//! dependency, so it runs unchanged on Linux and iOS.

pub mod alert;
#[cfg(feature = "output")]
pub mod output;
pub mod synth;
pub mod tone;
pub mod voice;
pub mod wav;

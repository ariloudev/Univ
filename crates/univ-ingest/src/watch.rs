//! What a long stage running on a blocking thread tells the run as it goes, and how the owner's
//! Ctrl-C reaches it: a flag the stage looks at only at safe points, where stopping leaves
//! nothing half made (AC-121).

use std::sync::atomic::{AtomicBool, Ordering};

/// Why a stage stopped when the owner asked it to.
pub const STOPPED: &str = "stopped by the owner";

/// A flag no one sets, for a stage no owner watches.
static NEVER: AtomicBool = AtomicBool::new(false);

fn nothing(_: String) {}

/// A long stage's steps, told as each is done, and the flag that asks it to stop.
#[derive(Clone, Copy)]
pub struct Watch<'a> {
    flag: &'a AtomicBool,
    told: &'a (dyn Fn(String) + Sync),
}

impl<'a> Watch<'a> {
    /// A stage that tells `told` each step it has done, and stops at its next safe point once
    /// `flag` is set.
    #[must_use]
    pub fn new(flag: &'a AtomicBool, told: &'a (dyn Fn(String) + Sync)) -> Self {
        Self { flag, told }
    }

    /// A stage that tells no one its steps and is never asked to stop.
    #[must_use]
    pub fn unwatched() -> Watch<'static> {
        Watch {
            flag: &NEVER,
            told: &nothing,
        }
    }

    /// Tells the run that a step is done, saying what it did.
    pub fn step(&self, line: String) {
        (self.told)(line);
    }

    /// Whether the owner asked the stage to stop.
    #[must_use]
    pub fn stopped(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }

    /// A safe point: the stage goes on unless the owner asked it to stop.
    ///
    /// # Errors
    ///
    /// Fails with [`STOPPED`] once the owner asked the stage to stop.
    pub fn check(&self) -> Result<(), String> {
        if self.stopped() {
            Err(STOPPED.to_owned())
        } else {
            Ok(())
        }
    }
}

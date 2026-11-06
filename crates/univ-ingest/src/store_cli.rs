//! The commands that act on the local D1 and R2: publish, prune and migrate, each run on the run
//! display and stopped by the owner's Ctrl-C.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::Args;
use univ_ingest::display::Outcome;
use univ_ingest::events::Event;
use univ_ingest::run::{
    DEFAULT_BUILD, DEFAULT_CONFIG, DEFAULT_PERSIST, StoreCommand, StoreRun, display_for_stderr,
    drawing, owner_interrupt, store_command,
};
use univ_ingest::state::DEFAULT_STATE;

#[derive(Args)]
pub struct MigrateArgs {
    /// Wrangler's local state directory, as `wrangler dev --persist-to` takes it.
    #[arg(long, value_name = "DIR", default_value = DEFAULT_PERSIST)]
    pub persist_to: PathBuf,
    /// The Wrangler configuration naming the local D1 and R2 bindings; Wrangler is run from
    /// the `node_modules` beside it.
    #[arg(long, value_name = "FILE", default_value = DEFAULT_CONFIG)]
    pub config: PathBuf,
}

#[derive(Args)]
pub struct StoreArgs {
    /// The release number.
    pub number: u32,
    /// The build output, whose `releases/{n}` holds the release.
    #[arg(long, value_name = "DIR", default_value = DEFAULT_BUILD)]
    pub out: PathBuf,
    /// The state directory, which records each release built; a release it does not record is
    /// not published.
    #[arg(long, value_name = "DIR", default_value = DEFAULT_STATE)]
    pub state: PathBuf,
    /// Wrangler's local state directory, as `wrangler dev --persist-to` takes it.
    #[arg(long, value_name = "DIR", default_value = DEFAULT_PERSIST)]
    pub persist_to: PathBuf,
    /// The Wrangler configuration naming the local D1 and R2 bindings; Wrangler is run from
    /// the `node_modules` beside it.
    #[arg(long, value_name = "FILE", default_value = DEFAULT_CONFIG)]
    pub config: PathBuf,
}

impl From<StoreArgs> for StoreRun {
    fn from(args: StoreArgs) -> Self {
        Self {
            number: args.number,
            out: args.out,
            state: args.state,
            persist_to: args.persist_to,
            config: args.config,
        }
    }
}

/// Runs the publish, prune or migrate `command` on the run display. A first Ctrl-C stops it once
/// the unit in hand is uploaded and checked, as the journal then lists it; a second stops it at
/// once.
pub async fn on_store(run: StoreRun, command: StoreCommand) -> ExitCode {
    let number = run.number;
    let display = display_for_stderr();
    let stop = Arc::new(AtomicBool::new(false));
    let (shown, flag) = (display.clone(), stop.clone());
    let work =
        tokio::task::spawn_blocking(move || store_command(&run, command, shown.as_ref(), &flag));
    let mut work = std::pin::pin!(drawing(display.as_ref(), work));
    let first = tokio::select! {
        done = &mut work => Some(done),
        () = owner_interrupt(tokio::signal::ctrl_c()) => None,
    };
    let interrupted_run = first.is_none();
    let done = if let Some(done) = first {
        done
    } else {
        stop.store(true, Ordering::Relaxed);
        display.emit(Event::Stopping {
            what: stopping(command, number),
        });
        tokio::select! {
            done = &mut work => done,
            () = owner_interrupt(tokio::signal::ctrl_c()) => {
                display.finish(vec![], Outcome::Stopped {
                    stage: command.stage(),
                    reason: "stopped at once by the owner".to_owned(),
                });
                // Returning would have the runtime wait for the unit in hand; the journal lists
                // only the units done, so the next run uploads that one again.
                std::process::exit(i32::from(crate::INTERRUPTED));
            }
        }
    };
    let outcome = done.unwrap_or_else(|e| Outcome::Stopped {
        stage: command.stage(),
        reason: e.to_string(),
    });
    let failed = matches!(outcome, Outcome::Stopped { .. });
    display.finish(vec![], outcome);
    if interrupted_run && failed {
        ExitCode::from(crate::INTERRUPTED)
    } else {
        ExitCode::from(u8::from(failed))
    }
}

/// What a publish or prune says once the owner interrupted it.
fn stopping(command: StoreCommand, number: u32) -> String {
    match command {
        StoreCommand::Publish => format!(
            "the publish of release {number} stops once the unit in hand is uploaded and \
             checked, latest left where it was; publishing release {number} again continues \
             from its journal. Ctrl-C again stops at once, and the unit in hand is uploaded \
             again by the next run"
        ),
        StoreCommand::Prune => format!(
            "the prune of release {number} stops once the step in hand is done; pruning \
             release {number} again finishes it. Ctrl-C again stops at once"
        ),
        StoreCommand::Migrate => "the migration stops once the step in hand is done; migrating \
             again finishes it. Ctrl-C again stops at once"
            .to_owned(),
    }
}

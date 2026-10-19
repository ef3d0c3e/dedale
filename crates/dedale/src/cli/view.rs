//! `dedale view`: browse runs and tournaments in the browser.

use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;

use clap::Args;

use super::CliResult;
use super::serve::{ServeArgs, wait_for_interrupt};
use crate::web::browse::Browse;

#[derive(Debug, Args)]
pub(crate) struct BrowseArgs {
    /// Directory of results to browse [default: dedale-out].
    #[arg(value_name = "DIR")]
    dir: Option<PathBuf>,
    #[command(flatten)]
    serve: ServeArgs,
}

pub(crate) fn run(args: &BrowseArgs) -> CliResult {
    let root = args
        .dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("dedale-out"));
    let browse =
        Browse::new(&root).map_err(|e| format!("cannot browse {}: {e}", root.display()))?;
    let handle = args.serve.start(Arc::new(browse), "")?;
    writeln!(
        io::stdout().lock(),
        "        serving {}, Ctrl-C to stop",
        root.display()
    )?;
    wait_for_interrupt();
    handle.stop();
    Ok(())
}

//! Serving the browser app from the command line (`dedale view`, `--view`).

use std::io::{self, Write};
use std::sync::Arc;
use std::thread;

use clap::Args;

use super::CliResult;
use crate::interrupt;
use crate::web::server::{Handler, Server, ServerHandle, open_browser};

#[derive(Debug, Clone, Args)]
pub(crate) struct ServeArgs {
    /// Port to serve the browser app on [default: any free port].
    #[arg(long, value_name = "N")]
    pub port: Option<u16>,
    /// Print the app's address without opening a browser.
    #[arg(long)]
    pub no_open: bool,
}

impl ServeArgs {
    /// Serves `handler`, prints the app's address (ending with `fragment`,
    /// a route like `#/live`) and opens it in a browser.
    pub(crate) fn start(
        &self,
        handler: Arc<dyn Handler>,
        fragment: &str,
    ) -> CliResult<ServerHandle> {
        let server = Server::bind(self.port).map_err(|e| match self.port {
            Some(port) => format!("cannot serve on 127.0.0.1:{port}: {e}"),
            None => format!("cannot serve on 127.0.0.1: {e}"),
        })?;
        let url = format!("{}{fragment}", server.url());
        let handle = server.spawn(handler)?;
        writeln!(io::stdout().lock(), "view    {url}")?;
        if !self.no_open
            && let Err(e) = open_browser(&url)
        {
            eprintln!("dedale: cannot open a browser ({e}); open the address above");
        }
        Ok(handle)
    }
}

/// Blocks until Ctrl-C.
pub(crate) fn wait_for_interrupt() {
    while !interrupt::requested() {
        thread::sleep(interrupt::POLL);
    }
}

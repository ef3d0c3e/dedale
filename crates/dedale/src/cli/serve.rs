//! Serving the browser app from the command line (`dedale view`, `--view`).

use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;
use std::thread;

use clap::Args;

use super::CliResult;
use crate::interrupt;
use crate::web::live::{Live, Plan};
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

#[derive(Debug, Clone, Args)]
pub(crate) struct LiveArgs {
    /// Follow in the browser app; play starts when Start is pressed there.
    #[arg(long)]
    pub view: bool,
    #[command(flatten)]
    pub serve: ServeArgs,
}

impl LiveArgs {
    /// With `--view`: serves a live session over `out` and waits for Start
    /// in the page (or Ctrl-C). `plan` is only made with `--view`.
    pub(crate) fn start(
        &self,
        plan: impl FnOnce() -> Plan,
        out: &Path,
    ) -> CliResult<Option<LiveServer>> {
        if !self.view {
            return Ok(None);
        }
        let live = Live::new(plan(), out)?;
        let handle = self.serve.start(live.clone(), "#/live")?;
        writeln!(
            io::stdout().lock(),
            "        press Start in the page to begin, Ctrl-C to quit"
        )?;
        live.wait_for_start()?;
        Ok(Some(LiveServer { live, handle }))
    }
}

/// A live session being served.
pub(crate) struct LiveServer {
    pub live: Arc<Live>,
    handle: ServerHandle,
}

impl LiveServer {
    /// Keeps the finished session in the page until Ctrl-C.
    pub(crate) fn linger(self) -> CliResult {
        writeln!(
            io::stdout().lock(),
            "        still serving the page, Ctrl-C to stop"
        )?;
        wait_for_interrupt();
        self.handle.stop();
        Ok(())
    }
}

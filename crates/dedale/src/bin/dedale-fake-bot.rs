//! Test helper: a raw-protocol bot whose behaviour, and misbehaviour, is
//! scripted by its arguments. Built only with the `fake-bot` feature.
//!
//! ```text
//! dedale-fake-bot [--moves up,left,...]     moves by turn, cycling (default: stay)
//!                 [--no-hello] [--protocol N]
//!                 [--name TEXT] [--author TEXT] [--description TEXT]
//!                 [--init-sleep-ms MS] [--init-error]
//!                 [--sleep-at TURN:MS]...  [--silent-at TURN]...
//!                 [--crash-at TURN]        exits with status 3
//!                 [--garbage-at TURN]...   [--oversized-at TURN]...
//!                 [--error-at TURN]...     move() "raised"
//!                 [--stale-at TURN]...     answers with the previous turn id
//!                 [--log TEXT]             attached to every move
//!                 [--stderr TEXT] [--stderr-bytes N]
//!                 [--ignore-end]           keeps running after `end`
//! ```

use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, Write};
use std::process::ExitCode;
use std::thread::sleep;
use std::time::Duration;

use dedale_core::protocol::{
    About, BotError, ErrorReply, FromBot, Hello, MAX_LINE, MoveReply, PROTOCOL_VERSION, Phase,
    Ready, ToBot, decode_dedale_line, encode_line,
};

#[derive(Debug)]
struct Script {
    moves: Vec<String>,
    hello: bool,
    protocol: u32,
    about: About,
    init_sleep: Duration,
    init_error: bool,
    sleep_at: HashMap<u32, Duration>,
    silent_at: HashSet<u32>,
    crash_at: Option<u32>,
    garbage_at: HashSet<u32>,
    oversized_at: HashSet<u32>,
    error_at: HashSet<u32>,
    stale_at: HashSet<u32>,
    log: Option<String>,
    stderr: Option<String>,
    stderr_bytes: usize,
    ignore_end: bool,
}

impl Script {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut script = Script {
            moves: Vec::new(),
            hello: true,
            protocol: PROTOCOL_VERSION,
            about: About::default(),
            init_sleep: Duration::ZERO,
            init_error: false,
            sleep_at: HashMap::new(),
            silent_at: HashSet::new(),
            crash_at: None,
            garbage_at: HashSet::new(),
            oversized_at: HashSet::new(),
            error_at: HashSet::new(),
            stale_at: HashSet::new(),
            log: None,
            stderr: None,
            stderr_bytes: 0,
            ignore_end: false,
        };
        while let Some(flag) = args.next() {
            let mut value = || args.next().ok_or(format!("{flag} needs a value"));
            match flag.as_str() {
                "--moves" => script.moves = value()?.split(',').map(str::to_owned).collect(),
                "--no-hello" => script.hello = false,
                "--protocol" => script.protocol = number(&value()?)?,
                "--name" => script.about.name = Some(value()?),
                "--author" => script.about.author = Some(value()?),
                "--description" => script.about.description = Some(value()?),
                "--init-sleep-ms" => script.init_sleep = millis(&value()?)?,
                "--init-error" => script.init_error = true,
                "--sleep-at" => {
                    let value = value()?;
                    let (turn, ms) = value.split_once(':').ok_or("--sleep-at takes TURN:MS")?;
                    script.sleep_at.insert(number(turn)?, millis(ms)?);
                }
                "--silent-at" => _ = script.silent_at.insert(number(&value()?)?),
                "--crash-at" => script.crash_at = Some(number(&value()?)?),
                "--garbage-at" => _ = script.garbage_at.insert(number(&value()?)?),
                "--oversized-at" => _ = script.oversized_at.insert(number(&value()?)?),
                "--error-at" => _ = script.error_at.insert(number(&value()?)?),
                "--stale-at" => _ = script.stale_at.insert(number(&value()?)?),
                "--log" => script.log = Some(value()?),
                "--stderr" => script.stderr = Some(value()?),
                "--stderr-bytes" => script.stderr_bytes = number(&value()?)?,
                "--ignore-end" => script.ignore_end = true,
                _ => return Err(format!("unknown flag {flag}")),
            }
        }
        Ok(script)
    }

    fn move_for(&self, turn: u32) -> String {
        let index = usize::try_from(turn.saturating_sub(1)).unwrap_or(0);
        self.moves
            .get(index % self.moves.len().max(1))
            .cloned()
            .unwrap_or_else(|| "stay".to_owned())
    }
}

fn number<T: std::str::FromStr>(text: &str) -> Result<T, String> {
    text.parse().map_err(|_| format!("not a number: {text}"))
}

fn millis(text: &str) -> Result<Duration, String> {
    number(text).map(Duration::from_millis)
}

fn send(out: &mut impl Write, msg: &FromBot) -> io::Result<()> {
    out.write_all(encode_line(msg).as_bytes())?;
    out.flush()
}

fn run(script: &Script) -> io::Result<ExitCode> {
    let mut out = io::stdout().lock();
    if let Some(text) = &script.stderr {
        eprintln!("{text}");
    }
    if script.stderr_bytes > 0 {
        io::stderr().write_all(&vec![b'e'; script.stderr_bytes])?;
    }
    if script.hello {
        let hello = Hello {
            protocol: script.protocol,
            about: script.about.clone(),
        };
        send(&mut out, &FromBot::Hello(hello))?;
    }
    for line in io::stdin().lock().lines() {
        let line = line?;
        match decode_dedale_line(line.as_bytes()) {
            Ok(ToBot::Init(_)) => {
                sleep(script.init_sleep);
                let reply = if script.init_error {
                    FromBot::Error(ErrorReply {
                        phase: Phase::Init,
                        error: BotError {
                            message: "scripted init error".to_owned(),
                            traceback: None,
                        },
                        log: Vec::new(),
                    })
                } else {
                    FromBot::Ready(Ready {
                        init_us: Some(1),
                        log: Vec::new(),
                        marks: Vec::new(),
                    })
                };
                send(&mut out, &reply)?;
            }
            Ok(ToBot::Turn(turn)) => {
                let t = turn.turn;
                if script.crash_at == Some(t) {
                    return Ok(ExitCode::from(3));
                }
                if let Some(&pause) = script.sleep_at.get(&t) {
                    sleep(pause);
                }
                if script.silent_at.contains(&t) {
                    continue;
                }
                if script.garbage_at.contains(&t) {
                    writeln!(out, "this is not json")?;
                    out.flush()?;
                } else if script.oversized_at.contains(&t) {
                    writeln!(out, "{}", "x".repeat(MAX_LINE + 1))?;
                    out.flush()?;
                } else {
                    let stale = script.stale_at.contains(&t);
                    let raised = script.error_at.contains(&t);
                    let reply = FromBot::Move(MoveReply {
                        turn: if stale { t - 1 } else { t },
                        mv: (!raised).then(|| script.move_for(t)),
                        think_us: Some(1),
                        log: script.log.iter().cloned().collect(),
                        marks: Vec::new(),
                        error: raised.then(|| BotError {
                            message: "scripted move error".to_owned(),
                            traceback: Some("Traceback: scripted".to_owned()),
                        }),
                    });
                    send(&mut out, &reply)?;
                }
            }
            Ok(ToBot::End(_)) => {
                if script.ignore_end {
                    loop {
                        sleep(Duration::from_secs(3600));
                    }
                }
                return Ok(ExitCode::SUCCESS);
            }
            Err(e) => eprintln!("dedale-fake-bot: {e}"),
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    let script = match Script::parse(std::env::args().skip(1)) {
        Ok(script) => script,
        Err(e) => {
            eprintln!("dedale-fake-bot: {e}");
            return ExitCode::from(2);
        }
    };
    // A closed stdout just means Dedale is done with us.
    run(&script).unwrap_or(ExitCode::SUCCESS)
}

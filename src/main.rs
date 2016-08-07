#[macro_use]
extern crate log;

extern crate tic;
extern crate bytes;
extern crate mio;
extern crate time;
extern crate getopts;

mod metrics;
mod proxy;
mod connection;
mod server;

use proxy::*;
use mio::*;
use mio::tcp::*;

use getopts::Options;
use std::net::ToSocketAddrs;
use log::{LogLevel, LogLevelFilter, LogMetadata, LogRecord};

use std::env;
use std::process;
use std::str;
use std::thread;

pub struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &LogMetadata) -> bool {
        metadata.level() <= LogLevel::Trace
    }

    fn log(&self, record: &LogRecord) {
        if self.enabled(record.metadata()) {
            println!("{} {:<5} [{}] {}",
                     time::strftime("%Y-%m-%d %H:%M:%S", &time::now()).unwrap(),
                     record.level().to_string(),
                     "perfprox",
                     record.args());
        }
    }
}

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} [options]", program);
    print!("{}", opts.usage(&brief));
}

pub fn opts() -> Options {
    let mut opts = Options::new();

    opts.optopt("b", "backend", "backend address", "HOST:PORT");
    opts.optopt("l", "listen", "frontend listen address", "HOST:PORT");
    opts.optopt("s", "stats", "HTTP stats port", "HOST:PORT");
    opts.optflag("", "version", "show version and exit");
    opts.optflagmulti("v", "verbose", "verbosity (stacking)");
    opts.optflag("h", "help", "print this help menu");

    opts
}

fn set_log_level(level: usize) {
    let log_filter;
    match level {
        0 => {
            log_filter = LogLevelFilter::Info;
        }
        1 => {
            log_filter = LogLevelFilter::Debug;
        }
        _ => {
            log_filter = LogLevelFilter::Trace;
        }
    }
    let _ = log::set_logger(|max_log_level| {
        max_log_level.set(log_filter);
        Box::new(SimpleLogger)
    });
}

pub fn main() {
    let args: Vec<String> = env::args().collect();

    let program = &args[0];

    let opts = opts();

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(f) => {
            error!("Failed to parse command line args: {}", f);
            return;
        }
    };

    if matches.opt_present("help") {
        print_usage(program, opts);
        return;
    }

    if matches.opt_present("version") {
        println!("rpc-perf {}", VERSION);
        return;
    }
    set_log_level(matches.opt_count("verbose"));

    info!("version {}", VERSION);
    info!("initializing");

    let backend = matches.opt_str("backend").unwrap_or("127.0.0.1:11211".to_owned());
    let listen = matches.opt_str("listen").unwrap_or("127.0.0.1:23432".to_owned());
    let stats = matches.opt_str("stats").unwrap_or("127.0.0.1:23433".to_owned());

    info!("backend: {}", backend);
    info!("listen: {}", listen);
    info!("stats: {}", stats);

    let mut receiver = metrics::build_receiver(stats);

    let sender = receiver.get_sender();

    let mut event_loop = match EventLoop::new() {
        Ok(v) => v,
        Err(e) => {
            error!("failed to initialize the mio EventLoop: {}", e);
            process::exit(1);
        }
    };

    let addr = &listen.to_socket_addrs().unwrap().next().unwrap();
    let srv = TcpListener::bind(addr).unwrap();

    event_loop.register(&srv,
                  SERVER,
                  EventSet::readable(),
                  PollOpt::edge() | PollOpt::oneshot())
        .unwrap();
    info!("listening");

    // Start the event loop
    thread::spawn(move || {
        event_loop.run(&mut Proxy::new(srv, sender, backend)).unwrap();
    });

    // run the stats receiver
    loop {
        receiver.run();
    }
}

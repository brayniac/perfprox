#[macro_use]
extern crate log;

extern crate tic;
extern crate bytes;
extern crate mio;
extern crate time;

mod proxy;
mod connection;
mod server;

use proxy::*;
use mio::*;
use mio::tcp::*;

use std::net::ToSocketAddrs;
use log::{LogLevel, LogLevelFilter, LogMetadata, LogRecord};

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
    set_log_level(0);
    info!("version {}", VERSION);
    info!("initializing");

    let receiver = tic::Receiver::new();

    let sender = receiver.get_sender();

    let mut event_loop = match EventLoop::new() {
        Ok(v) => v,
        Err(e) => {
            error!("failed to initialize the mio EventLoop: {}", e);
            process::exit(1);
        }
    };

    let listen = "127.0.0.1:23432";
    let addr = listen.to_socket_addrs().unwrap().next().unwrap();
    let srv = TcpListener::bind(&addr).unwrap();

    event_loop.register(&srv,
                  SERVER,
                  EventSet::readable(),
                  PollOpt::edge() | PollOpt::oneshot())
        .unwrap();
    info!("listening");

    // Start the event loop
    thread::spawn(move || {
        event_loop.run(&mut Proxy::new(srv, sender)).unwrap();
    });

    receiver.run();
}

use server::*;
use metrics::Metric;

use tic;
use mio::*;
use mio::tcp::*;
use mio::util::Slab;

use std::str;

pub const SERVER: Token = Token(0);

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

pub struct Proxy {
    server: Server,
}

impl Proxy {
    pub fn new(srv: TcpListener, stats: tic::Sender<Metric>, clocksource: tic::Clocksource, backend: String) -> Proxy {
        Proxy {
            server: Server {
                sock: srv,
                conns: Slab::new_starting_at(Token(1), 32768),
                stats: stats,
                clocksource: clocksource,
                backend: backend,
            },
        }
    }
}

impl Handler for Proxy {
    type Timeout = usize;
    type Message = ();

    fn ready(&mut self, event_loop: &mut EventLoop<Proxy>, token: Token, events: EventSet) {
        debug!("ready {:?} {:?}", token, events);
        if events.is_readable() {
            debug!("readable");
            match token {
                SERVER => {
                    self.server.accept(event_loop);
                    let _ = event_loop.reregister(&self.server.sock,
                                                  SERVER,
                                                  EventSet::readable(),
                                                  PollOpt::edge());
                }
                i => {
                    self.server.conn_readable(event_loop, i);
                }
            }
        }

        if events.is_writable() {
            debug!("writable");
            match token {
                SERVER => panic!("received writable for token 0"),
                _ => self.server.conn_writable(event_loop, token),
            };
        }
    }
}


use proxy::*;
use metrics::Metric;

use tic;
use mio::*;
use mio::tcp::*;
use bytes::{ByteBuf, MutByteBuf};

use std::str;

/// the connection holds both streams, the buffers, and other metadata
pub struct Connection {
    client: TcpStream,
    server: TcpStream,
    buf: Option<ByteBuf>,
    mut_buf: Option<MutByteBuf>,
    token: Option<Token>,
    interest: EventSet,
    stats: tic::Sender<Metric>,
    clocksource: tic::Clocksource,
    t0: Option<u64>,
    t1: Option<u64>,
    mode: Mode,
    state: State,
}

/// this helps us track if WE are acting as the server, or the client
enum Mode {
    Server,
    Client,
}

#[derive(PartialEq, Copy, Clone, Debug)]
enum State {
    Open,
    Closed,
}

impl Connection {
    /// create a new `Connection` from the two `TcpStream` and a `tic::Sender`
    pub fn new(
        client: TcpStream,
        server: TcpStream,
        stats: tic::Sender<Metric>,
        clocksource: tic::Clocksource,
    ) -> Connection {
        Connection {
            client: client,
            server: server,
            buf: None,
            mut_buf: Some(ByteBuf::mut_with_capacity(2048)),
            token: None,
            interest: EventSet::hup(),
            stats: stats,
            clocksource: clocksource,
            t0: None,
            t1: None,
            mode: Mode::Server,
            state: State::Open,
        }
    }

    /// get a reference to the client stream
    pub fn client(&self) -> &TcpStream {
        &self.client
    }

    /// get a reference to the server stream
    pub fn server(&self) -> &TcpStream {
        &self.server
    }

    /// set connection token
    pub fn set_token(&mut self, token: Token) {
        self.token = Some(token);
    }

    /// true if connection is closed
    pub fn is_closed(&self) -> bool {
        if self.state == State::Closed {
            return true;
        }
        false
    }

    /// call this when the connection is writable
    pub fn writable(&mut self, event_loop: &mut EventLoop<Proxy>) {
        let mut buf = self.buf.take().unwrap();

        let status = match self.mode {
            Mode::Server => self.client.try_write_buf(&mut buf),
            Mode::Client => self.server.try_write_buf(&mut buf),
        };

        match status {
            Ok(None) => {
                debug!("client flushing buf; WOULDBLOCK");

                self.buf = Some(buf);
                self.interest.insert(EventSet::writable());
            }
            Ok(Some(r)) => {
                debug!("CONN : we wrote {} bytes!", r);

                self.mut_buf = Some(buf.flip());

                self.interest.insert(EventSet::readable());
                self.interest.remove(EventSet::writable());

                match self.mode {
                    Mode::Server => {
                        // this is t1
                        debug!("Mode Change: Server Write -> Server Read");
                        self.t1 = Some(self.clocksource.counter());
                        let _ = event_loop.reregister(
                            &self.client,
                            self.token.unwrap(),
                            self.interest,
                            PollOpt::edge(),
                        );
                        return;
                    }
                    Mode::Client => {
                        // this is t3
                        debug!("Mode Change: Client Write -> Client Read");
                        if let Some(t0) = self.t0 {
                            let _ = self.stats.send(tic::Sample::new(
                                t0,
                                self.clocksource.counter(),
                                Metric::Frontend,
                            ));
                        }
                        let _ = event_loop.reregister(
                            &self.server,
                            self.token.unwrap(),
                            self.interest,
                            PollOpt::edge(),
                        );
                        return;
                    }
                }
            }
            Err(e) => debug!("not implemented; client err={:?}", e),
        }

        let _ = event_loop.reregister(
            &self.client,
            self.token.unwrap(),
            self.interest,
            PollOpt::edge() | PollOpt::oneshot(),
        );
    }

    /// call this when the connection is readable
    pub fn readable(&mut self, event_loop: &mut EventLoop<Proxy>) {
        if let Some(mut buf) = self.mut_buf.take() {
            let status = match self.mode {
                Mode::Server => self.client.try_read_buf(&mut buf),
                Mode::Client => self.server.try_read_buf(&mut buf),
            };

            match status {
                Ok(None) => {
                    debug!("CONN : spurious read wakeup");
                    self.mut_buf = Some(buf);

                    match self.mode {
                        Mode::Server => {
                            debug!("Flip: Server -> Client");
                            self.mode = Mode::Client;
                            let _ = event_loop.reregister(
                                &self.server,
                                self.token.unwrap(),
                                self.interest,
                                PollOpt::edge(),
                            );
                            return;
                        }
                        Mode::Client => {
                            debug!("Flip: Client -> Server");
                            self.mode = Mode::Server;
                            let _ = event_loop.reregister(
                                &self.client,
                                self.token.unwrap(),
                                self.interest,
                                PollOpt::edge(),
                            );
                            return;
                        }
                    }
                }
                Ok(Some(r)) => {
                    debug!("CONN : we read {} bytes!", r);

                    if r == 0 {
                        debug!("CONN: hangup");
                        self.state = State::Closed;
                        self.interest.remove(EventSet::readable());
                        return;
                    }

                    self.buf = Some(buf.flip());
                    self.interest.remove(EventSet::readable());
                    self.interest.insert(EventSet::writable());
                    match self.mode {
                        Mode::Server => {
                            // this is t2
                            if let Some(t1) = self.t1 {
                                let _ = self.stats.send(tic::Sample::new(
                                    t1,
                                    self.clocksource.counter(),
                                    Metric::Backend,
                                ));
                            }
                            debug!("Mode Change: Server Read -> Client Write");
                            self.mode = Mode::Client;
                            let _ = event_loop.reregister(
                                &self.server,
                                self.token.unwrap(),
                                self.interest,
                                PollOpt::edge(),
                            );
                            return;
                        }
                        Mode::Client => {
                            // this is t0
                            self.t0 = Some(self.clocksource.counter());
                            debug!("Mode Change: Client Read -> Server Write");
                            self.mode = Mode::Server;
                            let _ = event_loop.reregister(
                                &self.client,
                                self.token.unwrap(),
                                self.interest,
                                PollOpt::edge(),
                            );
                            return;
                        }
                    }
                }
                Err(e) => {
                    debug!("not implemented; client err={:?}", e);
                    self.interest.remove(EventSet::readable());
                }
            }
        } else {
            self.state = State::Closed;
            self.interest.remove(EventSet::readable());
            return;
        }

        let _ = event_loop.reregister(
            &self.client,
            self.token.unwrap(),
            self.interest,
            PollOpt::edge(),
        );
    }
}

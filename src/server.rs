use proxy::*;
use connection::*;
use metrics::Metric;

use tic;

use mio::*;
use mio::tcp::*;
use mio::util::Slab;
use std::net::ToSocketAddrs;

pub struct Server {
    pub sock: TcpListener,
    pub conns: Slab<Connection>,
    pub stats: tic::Sender<Metric>,
    pub clocksource: tic::Clocksource,
    pub backend: String,
}

impl Server {
    pub fn accept(&mut self, event_loop: &mut EventLoop<Proxy>) {
        info!("server accepting socket");

        let client = self.sock.accept().unwrap().unwrap().0;

        let addr = &self.backend.to_socket_addrs().unwrap().next().unwrap();

        let server = TcpStream::connect(addr).unwrap();

        let conn = Connection::new(client, server, self.stats.clone(), self.clocksource.clone());
        let tok = self.conns.insert(conn).ok().expect(
            "could not add connection to slab",
        );

        // register the client connection
        self.conns[tok].set_token(tok);
        event_loop
            .register(
                self.conns[tok].client(),
                tok,
                EventSet::readable(),
                PollOpt::edge() | PollOpt::oneshot(),
            )
            .expect("could not register socket with event loop");

        // register the server connection
        event_loop
            .register(
                self.conns[tok].server(),
                tok,
                EventSet::hup(),
                PollOpt::edge() | PollOpt::oneshot(),
            )
            .expect("could not register socket with event loop");

        debug!("socket registered with event loop");

    }

    pub fn conn_readable(&mut self, event_loop: &mut EventLoop<Proxy>, tok: Token) {
        debug!("server conn readable; tok={:?}", tok);
        self.conn(tok).readable(event_loop);
        if self.conn(tok).is_closed() {
            debug!("closing");
            let connection = self.conns.remove(tok).unwrap();
            let _ = event_loop.deregister(connection.client());
            let _ = event_loop.deregister(connection.server());
        }

    }

    pub fn conn_writable(&mut self, event_loop: &mut EventLoop<Proxy>, tok: Token) {
        debug!("server conn writable; tok={:?}", tok);
        self.conn(tok).writable(event_loop)
    }

    pub fn conn(&mut self, tok: Token) -> &mut Connection {
        &mut self.conns[tok]
    }
}

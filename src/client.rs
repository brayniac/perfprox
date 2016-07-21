use proxy::*;

use mio::*;
use mio::tcp::*;
use bytes::{Buf, ByteBuf, MutByteBuf, SliceBuf};
use std::io;

use std::str;

pub struct Client {
    sock: TcpStream,
    msgs: Vec<&'static str>,
    tx: SliceBuf<'static>,
    rx: SliceBuf<'static>,
    mut_buf: Option<MutByteBuf>,
    token: Token,
    interest: EventSet,
}

// Sends a message and expects to receive the same exact message, one at a time
impl Client {
    pub fn new(sock: TcpStream, tok: Token, mut msgs: Vec<&'static str>) -> Client {
        let curr = msgs.remove(0);

        Client {
            sock: sock,
            msgs: msgs,
            tx: SliceBuf::wrap(curr.as_bytes()),
            rx: SliceBuf::wrap(curr.as_bytes()),
            mut_buf: Some(ByteBuf::mut_with_capacity(2048)),
            token: tok,
            interest: EventSet::none(),
        }
    }

    pub fn readable(&mut self, event_loop: &mut EventLoop<Proxy>) {
        debug!("client socket readable");

        let mut buf = match self.mut_buf.take() {
            Some(b) => b,
            None => {
                error!("readable() cannot take buffer")
            }
        }

        match self.sock.try_read_buf(&mut buf) {
            Ok(None) => {
                debug!("CLIENT : spurious read wakeup");
                self.mut_buf = Some(buf);
            }
            Ok(Some(r)) => {
                debug!("CLIENT : We read {} bytes!", r);

                // prepare for reading
                let mut buf = buf.flip();

                while buf.has_remaining() {
                    let actual = buf.read_byte().unwrap();
                    let expect = self.rx.read_byte().unwrap();

                    assert!(actual == expect, "actual={}; expect={}", actual, expect);
                }

                self.mut_buf = Some(buf.flip());

                self.interest.remove(EventSet::readable());

                if !self.rx.has_remaining() {
                    self.next_msg(event_loop).unwrap();
                }
            }
            Err(e) => {
                panic!("not implemented; client err={:?}", e);
            }
        };

        event_loop.reregister(&self.sock,
                              self.token,
                              self.interest,
                              PollOpt::edge() | PollOpt::oneshot())
    }

    pub fn writable(&mut self, event_loop: &mut EventLoop<Proxy>) -> io::Result<()> {
        debug!("client socket writable");

        match self.sock.try_write_buf(&mut self.tx) {
            Ok(None) => {
                debug!("client flushing buf; WOULDBLOCK");
                self.interest.insert(EventSet::writable());
            }
            Ok(Some(r)) => {
                debug!("CLIENT : we wrote {} bytes!", r);
                self.interest.insert(EventSet::readable());
                self.interest.remove(EventSet::writable());
            }
            Err(e) => debug!("not implemented; client err={:?}", e),
        }

        event_loop.reregister(&self.sock,
                              self.token,
                              self.interest,
                              PollOpt::edge() | PollOpt::oneshot())
    }

    pub fn next_msg(&mut self, event_loop: &mut EventLoop<Proxy>) -> io::Result<()> {
        if self.msgs.is_empty() {
            return Ok(());
        }

        let curr = self.msgs.remove(0);

        debug!("client prepping next message");
        self.tx = SliceBuf::wrap(curr.as_bytes());
        self.rx = SliceBuf::wrap(curr.as_bytes());

        self.interest.insert(EventSet::writable());
        event_loop.reregister(&self.sock,
                              self.token,
                              self.interest,
                              PollOpt::edge() | PollOpt::oneshot())
    }
}

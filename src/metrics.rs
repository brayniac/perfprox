// metrics
use tic;
use std::fmt;

#[derive(PartialEq, Eq, Hash, Clone)]
pub enum Metric {
    Ok,
    Error,
    Close,
    Connect,
    Frontend,
    Backend,
}

impl fmt::Display for Metric {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Metric::Ok => write!(f, "ok"),
            Metric::Error => write!(f, "error"),
            Metric::Close => write!(f, "close"),
            Metric::Connect => write!(f, "connect"),
            Metric::Frontend => write!(f, "frontend"),
            Metric::Backend => write!(f, "backend"),
        }
    }
}

pub fn build_receiver(listen: String) -> tic::Receiver<Metric> {
    let mut r = tic::Receiver::configure().http_listen(listen).build();

    r.add_interest(tic::Interest::Count(Metric::Ok));
    r.add_interest(tic::Interest::Count(Metric::Error));
    r.add_interest(tic::Interest::Count(Metric::Close));
    r.add_interest(tic::Interest::Count(Metric::Connect));
    r.add_interest(tic::Interest::Count(Metric::Connect));
    r.add_interest(tic::Interest::Count(Metric::Frontend));
    r.add_interest(tic::Interest::Percentile(Metric::Frontend));
    r.add_interest(tic::Interest::Count(Metric::Backend));
    r.add_interest(tic::Interest::Percentile(Metric::Backend));
    r
}

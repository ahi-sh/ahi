//! A health tick resumes the same socket read, never the HTTP request.
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport,
};

#[derive(Debug)]
pub(super) struct PollingConnector {
    pub poll: Duration,
    pub cancel: Arc<AtomicBool>,
}

impl Connector<Box<dyn Transport>> for PollingConnector {
    type Out = PollingTransport;
    fn connect(
        &self,
        _: &ConnectionDetails,
        inner: Option<Box<dyn Transport>>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        Ok(inner.map(|inner| PollingTransport {
            inner,
            poll: self.poll,
            cancel: self.cancel.clone(),
        }))
    }
}
impl PollingConnector {
    pub fn agent(self, config: ureq::config::Config) -> ureq::Agent {
        ureq::Agent::with_parts(
            config,
            DefaultConnector::default().chain(self),
            ureq::unversioned::resolver::DefaultResolver::default(),
        )
    }
}
#[derive(Debug)]
pub(super) struct PollingTransport {
    inner: Box<dyn Transport>,
    poll: Duration,
    cancel: Arc<AtomicBool>,
}
impl Transport for PollingTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }
    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
    fn is_open(&mut self) -> bool {
        self.inner.is_open()
    }
    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.inner.transmit_output(amount, timeout)
    }
    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        let start = Instant::now();
        loop {
            if self.cancel.load(Ordering::Relaxed) {
                return Err(
                    io::Error::new(io::ErrorKind::ConnectionAborted, "response cancelled").into(),
                );
            }
            let mut next = timeout;
            if !timeout.after.is_not_happening() {
                let remaining = timeout.after.saturating_sub(start.elapsed());
                if remaining.is_zero() {
                    return Err(ureq::Error::Timeout(timeout.reason));
                }
                next.after = remaining.into();
            }
            next.per_read = Some(self.poll.into());
            match self.inner.await_input(next) {
                Err(ureq::Error::Timeout(ureq::Timeout::PerRead)) => continue,
                other => return other,
            }
        }
    }
}

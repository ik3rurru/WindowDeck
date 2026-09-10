//! LAN discovery is a locator, not authentication. The TCP handshake remains mandatory.
use mdns_sd::{Receiver, ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::BTreeMap;
use std::io;
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

pub const SERVICE: &str = "_windowdeck._tcp.local.";
pub struct Discovery(ServiceDaemon);
impl Drop for Discovery {
    fn drop(&mut self) {
        let _ = self.0.shutdown();
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Host {
    pub name: String,
    pub identity: String,
    pub addresses: Vec<SocketAddr>,
}
fn error(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}

pub fn advertise(bound: SocketAddr, codec: &str) -> io::Result<Discovery> {
    let name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "WindowDeck".into());
    let label: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(50)
        .collect();
    let label = if label.is_empty() {
        "windowdeck"
    } else {
        &label
    };
    let daemon = Discovery(ServiceDaemon::new().map_err(error)?);
    let properties = [("version", "1"), ("codec", codec), ("name", name.as_str())];
    let ip = if bound.ip().is_unspecified() {
        String::new()
    } else {
        bound.ip().to_string()
    };
    let mut info = ServiceInfo::new(
        SERVICE,
        &format!("{label}-{}", bound.port()),
        &format!("{label}.local."),
        ip.as_str(),
        bound.port(),
        &properties[..],
    )
    .map_err(error)?;
    if bound.ip().is_unspecified() {
        info = info.enable_addr_auto();
    }
    daemon.0.set_ip_check_interval(2).map_err(error)?;
    daemon.0.register(info).map_err(error)?;
    Ok(daemon)
}

/// Keeps mDNS running independently of video, including while a session is active.
/// Drain updates before reconnecting so an old address is never pinned forever.
pub struct Browser {
    _daemon: Discovery,
    events: Receiver<ServiceEvent>,
    codec: String,
    hosts: BTreeMap<String, Host>,
}

impl Browser {
    pub fn new(codec: &str) -> io::Result<Self> {
        let daemon = Discovery(ServiceDaemon::new().map_err(error)?);
        daemon.0.set_ip_check_interval(2).map_err(error)?;
        let events = daemon.0.browse(SERVICE).map_err(error)?;
        Ok(Self {
            _daemon: daemon,
            events,
            codec: codec.into(),
            hosts: BTreeMap::new(),
        })
    }

    fn update(&mut self, event: ServiceEvent) {
        match event {
            ServiceEvent::ServiceResolved(info)
                if info.get_property_val_str("version") == Some("1")
                    && info.get_property_val_str("codec") == Some(self.codec.as_str()) =>
            {
                let mut addresses: Vec<_> = info
                    .get_addresses_v4()
                    .into_iter()
                    .filter(|ip| !ip.is_unspecified() && !ip.is_multicast())
                    .map(|ip| SocketAddr::from((ip, info.get_port())))
                    .collect();
                addresses.sort_unstable();
                if info.get_port() != 0 && !addresses.is_empty() {
                    self.hosts.insert(
                        info.get_fullname().to_owned(),
                        Host {
                            name: info
                                .get_property_val_str("name")
                                .unwrap_or(info.get_hostname())
                                .to_owned(),
                            identity: info.get_fullname().to_owned(),
                            addresses,
                        },
                    );
                } else {
                    self.hosts.remove(info.get_fullname());
                }
            }
            ServiceEvent::ServiceResolved(info) => {
                self.hosts.remove(info.get_fullname());
            }
            ServiceEvent::ServiceRemoved(_, name) => {
                self.hosts.remove(&name);
            }
            _ => {}
        }
    }

    pub fn snapshot(&mut self) -> Vec<Host> {
        while let Ok(event) = self.events.try_recv() {
            self.update(event);
        }
        self.hosts.values().cloned().collect()
    }

    /// Returns as soon as a matching host exists; the timeout is only an upper bound.
    pub fn wait(&mut self, identity: Option<&str>, timeout: Duration) -> io::Result<Vec<Host>> {
        let deadline = Instant::now() + timeout;
        loop {
            let hosts: Vec<_> = self
                .snapshot()
                .into_iter()
                .filter(|host| identity.is_none_or(|id| id == host.identity))
                .collect();
            if !hosts.is_empty() || Instant::now() >= deadline {
                return Ok(hosts);
            }
            match self
                .events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(event) => self.update(event),
                Err(mdns_sd::RecvTimeoutError::Timeout) => return Ok(Vec::new()),
                Err(mdns_sd::RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::other("mDNS stopped"));
                }
            }
        }
    }
}

pub fn browse(codec: &str, identity: Option<&str>) -> io::Result<Vec<Host>> {
    Browser::new(codec)?.wait(identity, Duration::from_secs(4))
}

pub fn connect_host(host: &Host) -> io::Result<TcpStream> {
    let mut last = io::Error::new(io::ErrorKind::NotFound, "El PC no anuncia direcciones IPv4");
    for address in &host.addresses {
        match TcpStream::connect_timeout(address, Duration::from_millis(700)) {
            Ok(stream) => return Ok(stream),
            Err(e) => last = e,
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn tries_other_addresses_of_the_same_host() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let host = Host {
            name: "test".into(),
            identity: "test".into(),
            addresses: vec![
                "127.0.0.1:0".parse().unwrap(),
                listener.local_addr().unwrap(),
            ],
        };
        assert_eq!(
            connect_host(&host).unwrap().peer_addr().unwrap(),
            listener.local_addr().unwrap()
        );
    }

    #[test]
    #[ignore = "requires multicast LAN; run explicitly outside sandbox"]
    fn discovers_announced_port_and_filters_identity_and_codec() {
        let listener = TcpListener::bind("0.0.0.0:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let _service = advertise(listener.local_addr().unwrap(), "test-codec").unwrap();
        let hosts = browse("test-codec", None).unwrap();
        let host = hosts
            .iter()
            .find(|h| h.addresses.iter().any(|a| a.port() == port))
            .expect("local service was not discovered");
        assert!(connect_host(host).is_ok());
        assert!(
            browse("different-codec", Some(&host.identity))
                .unwrap()
                .is_empty()
        );
        assert!(
            browse("test-codec", Some("different-host"))
                .unwrap()
                .is_empty()
        );
        assert_eq!(browse("test-codec", Some(&host.identity)).unwrap().len(), 1);
    }
}

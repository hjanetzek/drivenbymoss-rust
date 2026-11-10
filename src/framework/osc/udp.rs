// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The OSC server and client over UDP (former `IOpenSoundControlServer` and
//! `IOpenSoundControlClient`, which Bitwig's `OscModule` implements in the Java version), with
//! `std::net::UdpSocket` only.
//!
//! Both sockets are non-blocking and are used from the thread which calls into the core (see
//! `doc/HOST_BRIDGE_GUIDE.md`): the controller polls the server at every tick of the core
//! ([`OscServer::receive`]) and sends its bundles there ([`OscClient::send_bundle`],
//! [`OscClient::send_pending`]). Nothing blocks and no thread is started.

use std::collections::VecDeque;
use std::io::ErrorKind;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

use super::message::{OscDecodeError, OscMessage, OscPacket};

/// The largest UDP datagram.
const MAX_DATAGRAM: usize = 65536;

/// The number of messages per bundle (former `OpenSoundControlClientImpl.sendBundle`: "stay
/// below 64K, which is the maximum of an UDP message").
pub const MESSAGES_PER_BUNDLE: usize = 101;

/// The most datagrams which are read in one call of [`OscServer::receive`]; the rest stays in
/// the socket for the next call.
const MAX_DATAGRAMS_PER_RECEIVE: usize = 512;

/// Receives OSC messages on a UDP port (former `IOpenSoundControlServer`).
#[derive(Default)]
pub struct OscServer {
    socket: Option<UdpSocket>,
    port: Option<u16>,
    buffer: Vec<u8>,
}

/// A datagram which could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiveError {
    pub source: SocketAddr,
    pub error: OscDecodeError,
}

impl OscServer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start listening on the port of all network interfaces (former `start`); a running server
    /// is stopped first. Port 0 picks a free port (see [`Self::listening_port`]).
    pub fn start(&mut self, port: u16) -> std::io::Result<()> {
        self.stop();
        let socket = UdpSocket::bind(("0.0.0.0", port))?;
        socket.set_nonblocking(true)?;
        self.port = Some(socket.local_addr()?.port());
        self.socket = Some(socket);
        Ok(())
    }

    /// Close the socket.
    pub fn stop(&mut self) {
        self.socket = None;
        self.port = None;
    }

    /// The port the server listens on, None if it is not started (former `getListeningPort`,
    /// -1).
    pub fn listening_port(&self) -> Option<u16> {
        self.port
    }

    /// Read the waiting datagrams (at most a fixed number per call) and decode them. The
    /// messages of bundles are returned in their order, time tags are ignored.
    pub fn receive(&mut self) -> (Vec<OscMessage>, Vec<ReceiveError>) {
        let mut messages = Vec::new();
        let mut errors = Vec::new();
        let Some(socket) = &self.socket else {
            return (messages, errors);
        };
        if self.buffer.len() != MAX_DATAGRAM {
            self.buffer.resize(MAX_DATAGRAM, 0);
        }
        for _ in 0..MAX_DATAGRAMS_PER_RECEIVE {
            match socket.recv_from(&mut self.buffer) {
                Ok((size, source)) => match OscPacket::decode(&self.buffer[..size]) {
                    Ok(packet) => messages.extend(packet.into_messages()),
                    Err(error) => errors.push(ReceiveError { source, error }),
                },
                // WouldBlock: nothing left. Other errors (e.g. ICMP port unreachable reported
                // on Windows) are not fatal for a UDP server.
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        (messages, errors)
    }
}

/// Sends OSC bundles to a host and port (former `IOpenSoundControlClient`).
pub struct OscClient {
    socket: UdpSocket,
    target: SocketAddr,
    /// The encoded bundles which wait for the next call of [`Self::send_pending`].
    pending: VecDeque<Vec<u8>>,
}

impl OscClient {
    /// Connect to the host (a name or an address) and port (former
    /// `host.connectToOSCServer`). The name is resolved once, here.
    pub fn connect(host: &str, port: u16) -> std::io::Result<Self> {
        let target = (host, port)
            .to_socket_addrs()?
            .find(SocketAddr::is_ipv4)
            .or_else(|| (host, port).to_socket_addrs().ok().and_then(|mut addresses| addresses.next()))
            .ok_or_else(|| std::io::Error::new(ErrorKind::NotFound, format!("Unknown host: {host}")))?;
        let bind: SocketAddr = if target.is_ipv4() { ([0, 0, 0, 0], 0).into() } else { (std::net::Ipv6Addr::UNSPECIFIED, 0).into() };
        let socket = UdpSocket::bind(bind)?;
        socket.set_nonblocking(true)?;
        Ok(OscClient { socket, target, pending: VecDeque::new() })
    }

    /// The address the client sends to.
    pub fn target(&self) -> SocketAddr {
        self.target
    }

    /// Send a single message.
    pub fn send_message(&self, message: &OscMessage) -> std::io::Result<()> {
        self.send_datagram(&message.encode())
    }

    /// Send the messages in bundles of at most [`MESSAGES_PER_BUNDLE`] messages (former
    /// `sendBundle`). The first bundle is sent now, the further ones wait for the next calls of
    /// [`Self::send_pending`] (one per call). The Java version sleeps 10 ms between the bundles
    /// "otherwise clients like Open Stage Control cannot keep up"; the core must not block, so it
    /// sends one bundle per tick instead.
    pub fn send_bundle(&mut self, messages: Vec<OscMessage>) -> std::io::Result<()> {
        if messages.is_empty() {
            return Ok(());
        }
        let mut messages = messages.into_iter().peekable();
        while messages.peek().is_some() {
            let chunk: Vec<OscMessage> = messages.by_ref().take(MESSAGES_PER_BUNDLE).collect();
            self.pending.push_back(OscPacket::bundle(chunk).encode());
        }
        self.send_pending()
    }

    /// Send the next waiting bundle, if any.
    pub fn send_pending(&mut self) -> std::io::Result<()> {
        match self.pending.pop_front() {
            Some(datagram) => self.send_datagram(&datagram),
            None => Ok(()),
        }
    }

    /// True if bundles wait to be sent.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    fn send_datagram(&self, datagram: &[u8]) -> std::io::Result<()> {
        match self.socket.send_to(datagram, self.target) {
            Ok(_) => Ok(()),
            // The socket buffer is full: the datagram is dropped, as UDP may do anyway
            Err(error) if error.kind() == ErrorKind::WouldBlock => Ok(()),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framework::osc::message::OscArg;
    use std::time::{Duration, Instant};

    /// Receive until the condition holds or a second passed.
    fn receive_until(server: &mut OscServer, mut done: impl FnMut(&[OscMessage]) -> bool) -> Vec<OscMessage> {
        let start = Instant::now();
        let mut all = Vec::new();
        while start.elapsed() < Duration::from_secs(2) {
            let (messages, errors) = server.receive();
            assert!(errors.is_empty(), "{errors:?}");
            all.extend(messages);
            if done(&all) {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        all
    }

    #[test]
    fn client_and_server_over_localhost() {
        let mut server = OscServer::new();
        assert_eq!(server.listening_port(), None);
        assert!(server.receive().0.is_empty(), "a stopped server receives nothing");
        server.start(0).unwrap();
        let port = server.listening_port().unwrap();
        assert!(server.receive().0.is_empty());

        let mut client = OscClient::connect("localhost", port).unwrap();
        assert!(client.target().ip().is_loopback());
        client.send_message(&OscMessage::new("/play", vec![OscArg::Int(1)])).unwrap();
        let messages: Vec<OscMessage> = (0..250).map(|i| OscMessage::new(format!("/m/{i}"), vec![OscArg::Int(i)])).collect();
        client.send_bundle(messages).unwrap();
        // 250 messages: 3 bundles, the first one is sent at once
        assert!(client.has_pending());
        client.send_pending().unwrap();
        client.send_pending().unwrap();
        assert!(!client.has_pending());
        client.send_pending().unwrap();

        let received = receive_until(&mut server, |all| all.len() >= 251);
        assert_eq!(received.len(), 251);
        assert_eq!(received[0], OscMessage::new("/play", vec![OscArg::Int(1)]));
        assert_eq!(received[250], OscMessage::new("/m/249", vec![OscArg::Int(249)]));

        // A broken datagram is reported with its source
        let raw = UdpSocket::bind("127.0.0.1:0").unwrap();
        raw.send_to(b"garbage!", ("127.0.0.1", port)).unwrap();
        let start = Instant::now();
        let mut errors = Vec::new();
        while errors.is_empty() && start.elapsed() < Duration::from_secs(2) {
            errors = server.receive().1;
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error, OscDecodeError::NotOsc);
        assert_eq!(errors[0].source, raw.local_addr().unwrap());

        server.stop();
        assert_eq!(server.listening_port(), None);
    }

    #[test]
    fn the_port_is_taken() {
        let mut first = OscServer::new();
        first.start(0).unwrap();
        let mut second = OscServer::new();
        assert!(second.start(first.listening_port().unwrap()).is_err());
        assert_eq!(second.listening_port(), None);
    }

    #[test]
    fn unknown_host() {
        assert!(OscClient::connect("no-such-host.invalid", 9000).is_err());
    }
}

// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The OSC controller over real UDP sockets on localhost: an `App` with one OSC instance (as the
//! Bitwig extension and the Python reference host create it) and a test client which sends
//! commands to the receive port and listens on the send port.

use std::collections::HashMap;
use std::net::UdpSocket;
use std::rc::Rc;
use std::time::Duration;

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordedCalls, RecordingBackend};
use drivenbymoss::config::osc as osc_config;
use drivenbymoss::config::SettingId;
use drivenbymoss::controller::osc::OSC_UUID;
use drivenbymoss::framework::osc::{OscArg, OscMessage, OscPacket};
use drivenbymoss::protocol::Cmd;

/// A free UDP port (bound and released again).
fn free_port() -> u16 {
    UdpSocket::bind("0.0.0.0:0").unwrap().local_addr().unwrap().port()
}

struct Harness {
    app: App,
    backend: Rc<RecordingBackend>,
    /// The test client: receives what the controller sends, sends commands to it.
    client: UdpSocket,
    receive_port: u16,
    time_ms: u64,
    /// All values received so far, by address (the last one).
    values: HashMap<String, Vec<OscArg>>,
}

impl Harness {
    fn start(name: &str, extra: &[(SettingId, &str)]) -> Harness {
        let dir = std::env::temp_dir().join(format!("dbm-test-osc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        client.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
        let send_port = client.local_addr().unwrap().port();
        let receive_port = free_port();

        // The ports in the controller configuration file
        let config = osc_config::for_reaper();
        let key = |id| config.descriptor(id).unwrap().key();
        let mut text = format!("{}={receive_port}\n{}={send_port}\n", key(osc_config::RECEIVE_PORT), key(osc_config::SEND_PORT));
        for (id, value) in extra {
            text.push_str(&format!("{}={value}\n", key(*id)));
        }
        std::fs::write(dir.join("DrivenByMoss4Reaper-OSC.config"), text).unwrap();

        let backend = Rc::new(RecordingBackend { inputs: vec![(0, "OSC".into())], ..Default::default() });
        let mut app = App::new_single(backend.clone(), dir.to_str().unwrap(), OSC_UUID, "test").expect("the OSC controller");
        app.set_time_ms(0);
        app.start_infrastructure();
        Harness { app, backend, client, receive_port, time_ms: 0, values: HashMap::new() }
    }

    /// Advance the time by ms in ticks of 30 ms.
    fn advance(&mut self, ms: u64) {
        let end = self.time_ms + ms;
        while self.time_ms < end {
            self.time_ms += 30;
            self.app.set_time_ms(self.time_ms);
            self.app.tick();
        }
    }

    /// Send a command to the controller.
    fn send(&self, address: &str, args: Vec<OscArg>) {
        let datagram = OscMessage::new(address, args).encode();
        self.client.send_to(&datagram, ("127.0.0.1", self.receive_port)).unwrap();
    }

    /// Tick until the datagrams sent to the controller arrived and were executed.
    fn deliver(&mut self) {
        // The datagrams travel through the loopback interface: tick a few times
        for _ in 0..5 {
            std::thread::sleep(Duration::from_millis(5));
            self.advance(30);
        }
    }

    /// Receive the messages of all bundles which arrive (ticking so that the queued bundles of a
    /// large update are sent), in their order.
    fn receive(&mut self) -> Vec<OscMessage> {
        let mut messages = Vec::new();
        let mut buffer = vec![0; 65536];
        self.client.set_nonblocking(true).unwrap();
        let mut idle = 0;
        while idle < 10 {
            match self.client.recv_from(&mut buffer) {
                Ok((size, _)) => {
                    idle = 0;
                    let packet = OscPacket::decode(&buffer[..size]).expect("a valid OSC packet");
                    assert!(matches!(packet, OscPacket::Bundle { .. }), "the controller sends bundles");
                    messages.extend(packet.into_messages());
                }
                Err(_) => {
                    idle += 1;
                    std::thread::sleep(Duration::from_millis(2));
                    self.advance(30);
                }
            }
        }
        for message in &messages {
            self.values.insert(message.address.clone(), message.args.clone());
        }
        messages
    }

    fn value(&self, address: &str) -> Option<&[OscArg]> {
        self.values.get(address).map(Vec::as_slice)
    }

    fn logs(&self) -> Vec<String> {
        self.backend.logs.borrow().clone()
    }
}

#[test]
fn full_update_changes_and_commands() {
    let mut h = Harness::start("full", &[]);
    let calls = h.backend.take_calls();
    assert!(calls.contains(&BackendCall::OpenInput(0)));
    assert!(!calls.iter().any(|c| matches!(c, BackendCall::OpenOutput(_))), "no MIDI output: {calls:?}");
    assert!(calls.iter().any(|c| matches!(c, BackendCall::NoteInputFilters(0, 0, f) if !f.is_empty())), "a note input for the virtual keyboard");
    assert!(h.logs().iter().any(|l| l.contains(&format!("Started OSC server on port {}.", h.receive_port))), "{:?}", h.logs());

    h.app.update_model("track/count 2\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/1/exists 1\ntrack/1/name Bass\nplay 0\n");

    // Changes are sent from the start on, the complete update 1 s after the startup
    h.advance(900);
    let first = h.receive();
    assert!(!first.is_empty());
    assert_eq!(first.first().map(|m| m.log_text()).as_deref(), Some("/update [ 1 ]"));
    assert_eq!(first.last().map(|m| m.log_text()).as_deref(), Some("/update [ 0 ]"));
    assert_eq!(h.value("/track/1/name"), Some([OscArg::Str("Drums".into())].as_slice()));
    assert_eq!(h.value("/track/2/name"), Some([OscArg::Str("Bass".into())].as_slice()));
    assert_eq!(h.value("/track/1/selected"), Some([OscArg::Int(1)].as_slice()));
    assert_eq!(h.value("/track/1/volume"), Some([OscArg::Int(64)].as_slice()), "0.5 in the low resolution (128)");
    assert_eq!(h.value("/play"), Some([OscArg::Int(0)].as_slice()));
    assert_eq!(h.value("/track/selected/name"), Some([OscArg::Str("Drums".into())].as_slice()));
    assert!(h.value("/vkb_midi/note/60/color").is_some());
    assert!(h.value("/vkb_midi/note/127/color").is_none(), "Java sends the colors of the notes 0-126");

    // Nothing changed: nothing is sent
    h.advance(300);
    assert!(h.receive().is_empty());

    // The complete update after the startup (2 s after the start): more than one bundle
    h.advance(1000);
    let dump = h.receive();
    assert!(dump.len() > 1000, "{}", dump.len());
    assert_eq!(dump.iter().filter(|m| m.address == "/update").count(), 2);

    // A change: only the changed values
    h.app.update_model("track/1/name Bass 2\n");
    h.advance(30);
    let changes: Vec<String> = h.receive().iter().map(OscMessage::log_text).collect();
    assert_eq!(changes, ["/update [ 1 ]", "/track/2/name [ Bass 2 ]", "/update [ 0 ]"]);

    // Commands
    h.backend.take_calls();
    h.send("/track/1/volume", vec![OscArg::Int(100)]);
    h.send("/play", vec![]);
    h.send("/track/2/mute", vec![OscArg::Int(1)]);
    h.send("/vkb_midi/1/note/60", vec![OscArg::Int(100)]);
    h.send("/tempo/raw", vec![OscArg::Float(133.0)]);
    h.deliver();
    let calls = h.backend.take_calls();
    assert!(calls.iter().any(|c| matches!(c, BackendCall::Midi(0, 0x90, 60, 100))), "{calls:?}");
    assert!(calls.has_cmd(|c| matches!(c, Cmd::ChannelVolume { .. })), "{calls:?}");
    assert!(calls.has_cmd(|c| matches!(c, Cmd::ChannelMute { .. })), "{calls:?}");
    assert!(calls.has_cmd(|c| matches!(c, Cmd::TempoSet { .. })), "{calls:?}");
    assert!(calls.has_cmd(|c| matches!(c, Cmd::PlayToggle {} | Cmd::TransportPlay {})), "{calls:?}");

    // The played note is marked on the virtual keyboard
    h.receive();
    assert_eq!(h.value("/vkb_midi/note/60/color"), Some([OscArg::Str("rgb(0,255,0)".into())].as_slice()));

    // Errors are logged
    h.send("/nonsense/1", vec![]);
    h.send("/track/1/volume", vec![OscArg::Str("loud".into())]);
    h.send("/track/1/nonsense", vec![]);
    h.send("/track", vec![]);
    h.deliver();
    let logs = h.logs();
    assert!(logs.iter().any(|l| l == "Unknown OSC command: /nonsense/1 nonsense"), "{logs:?}");
    assert!(logs.iter().any(|l| l == "Illegal parameter: /track/1/volume Parameter is not a Number"), "{logs:?}");
    assert!(logs.iter().any(|l| l == "Unknown OSC command: /track/1/nonsense nonsense"), "{logs:?}");
    assert!(logs.iter().any(|l| l == "Missing command: /track"), "{logs:?}");

    // A refresh sends everything again
    h.receive();
    h.send("/refresh", vec![]);
    h.deliver();
    assert!(h.receive().len() > 1000);

    h.app.shutdown();
    let calls = h.backend.take_calls();
    assert!(calls.contains(&BackendCall::CloseInput(0)));
    assert!(!calls.iter().any(|c| matches!(c, BackendCall::CloseOutput(_))));
}

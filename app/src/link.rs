use crate::protocol::{CdcDecoder, MessageType, Packet, state_payload};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, Sender},
    },
    thread,
    time::{Duration, Instant, SystemTime},
};

#[derive(Clone, Debug)]
pub enum LinkCommand {
    Begin,
    State {
        modifiers: u8,
        bitmap: [u8; 32],
        press: bool,
        captured_us: u64,
    },
    Stop,
    SetDebug(bool),
    Shutdown,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkState {
    Connecting,
    Searching,
    Connected,
    Capturing,
    Error(String),
}
#[derive(Clone, Debug)]
pub enum LinkEvent {
    State(LinkState),
    CaptureAuthorized,
    Ack {
        sequence: u32,
        press: bool,
        captured_us: u64,
        hid_us: Option<u64>,
    },
    Status {
        a_boot_us: u32,
        b_boot_us: u32,
        radio_ready_us: u32,
    },
    Leds(u8),
    ClockOffset {
        offset_us: f64,
        round_trip_us: u64,
    },
}

struct Pending {
    sequence: u32,
    payload: Vec<u8>,
    press: bool,
    captured_us: u64,
    last_sent: Option<Instant>,
    queued_at: Instant,
}

impl Pending {
    fn expired(&self, now: Instant) -> bool {
        now.duration_since(self.queued_at) >= Duration::from_millis(250)
    }
    fn packet(&self, session: u64, epoch: u64) -> Packet {
        let mut packet = Packet::new(
            MessageType::State,
            session,
            epoch,
            self.sequence,
            self.payload.clone(),
        );
        packet.keypress_sample = self.press;
        packet
    }
}

pub fn spawn(
    path: String,
    debug: bool,
    command_rx: Receiver<LinkCommand>,
    event_tx: Sender<LinkEvent>,
    gate: Arc<AtomicBool>,
    lease_ms: Arc<AtomicU64>,
    anchor: Instant,
) {
    thread::spawn(move || run(path, debug, command_rx, event_tx, gate, lease_ms, anchor));
}

fn run(
    path: String,
    mut debug: bool,
    command_rx: Receiver<LinkCommand>,
    event_tx: Sender<LinkEvent>,
    gate: Arc<AtomicBool>,
    lease_ms: Arc<AtomicU64>,
    anchor: Instant,
) {
    let mut port: Option<Box<dyn serialport::SerialPort>> = None;
    let mut decoder = CdcDecoder::default();
    let mut state = LinkState::Connecting;
    let mut session = 0u64;
    let mut epoch = 0u64;
    let mut next_sequence = 1u32;
    let mut pending: VecDeque<Pending> = VecDeque::new();
    let mut capturing = false;
    let mut start_pending = false;
    let mut start_sent: Option<Instant> = None;
    let mut start_deadline: Option<Instant> = None;
    let mut hello_at = Instant::now() - Duration::from_secs(1);
    let mut heartbeat_at = Instant::now();
    let mut sync_at = Instant::now();
    let mut last_open_error = None;
    loop {
        let now = Instant::now();
        if session != 0 && (port.is_none() || matches!(state, LinkState::Error(_))) {
            gate.store(false, Ordering::Release);
            if port.is_none() {
                fail(&event_tx, &mut state, "USB CDC disconnected");
            }
            send_stop(&mut port, &path, session, epoch);
            pending.clear();
            capturing = false;
            start_pending = false;
            session = 0;
            epoch = 0;
            decoder = CdcDecoder::default();
        }
        if start_pending && start_deadline.is_some_and(|deadline| now >= deadline) {
            gate.store(false, Ordering::Release);
            fail(&event_tx, &mut state, "activation timeout");
            send_stop(&mut port, &path, session, epoch);
            pending.clear();
            capturing = false;
            start_pending = false;
            start_sent = None;
            start_deadline = None;
            session = 0;
            epoch = 0;
            continue;
        }
        let lease_expired = (anchor.elapsed().as_millis() as u64)
            .saturating_sub(lease_ms.load(Ordering::Acquire))
            > 150;
        let capture_requested = gate.load(Ordering::Acquire);
        if (capturing || start_pending) && (!capture_requested || lease_expired) {
            gate.store(false, Ordering::Release);
            send_stop(&mut port, &path, session, epoch);
            pending.clear();
            capturing = false;
            start_pending = false;
            start_sent = None;
            start_deadline = None;
            session = 0;
            epoch = 0;
            if capture_requested && lease_expired {
                fail(&event_tx, &mut state, "capture focus lease expired");
            } else {
                state = LinkState::Searching;
                hello_at = now - Duration::from_secs(1);
                let _ = event_tx.send(LinkEvent::State(state.clone()));
            }
        }
        if port.is_none() {
            match serialport::new(&path, 115_200)
                .timeout(Duration::from_millis(5))
                .open()
            {
                Ok(new_port) => {
                    port = Some(new_port);
                    last_open_error = None;
                    decoder = CdcDecoder::default();
                    state = LinkState::Searching;
                    let _ = event_tx.send(LinkEvent::State(state.clone()));
                    hello_at = Instant::now() - Duration::from_secs(1);
                }
                Err(error) => {
                    let detail = error.to_string();
                    if last_open_error.as_deref() != Some(detail.as_str()) {
                        report_serial_failure(&path, "open", &error);
                        last_open_error = Some(detail);
                    }
                    if state != LinkState::Connecting {
                        state = LinkState::Connecting;
                        let _ = event_tx.send(LinkEvent::State(state.clone()));
                    }
                }
            }
        }
        while let Ok(command) = command_rx.try_recv() {
            match command {
                LinkCommand::Shutdown => {
                    send_stop(&mut port, &path, session, epoch);
                    close_port(&mut port, &path);
                    return;
                }
                LinkCommand::Stop => {
                    gate.store(false, Ordering::Release);
                    send_stop(&mut port, &path, session, epoch);
                    pending.clear();
                    capturing = false;
                    start_pending = false;
                    start_sent = None;
                    start_deadline = None;
                    session = 0;
                    epoch = 0;
                    state = if port.is_some() {
                        hello_at = now - Duration::from_secs(1);
                        LinkState::Searching
                    } else {
                        LinkState::Connecting
                    };
                    let _ = event_tx.send(LinkEvent::State(state.clone()));
                }
                LinkCommand::SetDebug(enabled) => {
                    debug = enabled;
                    send_or_drop(
                        &mut port,
                        &path,
                        "write DEBUG",
                        Packet::new(MessageType::Debug, session, epoch, 0, vec![enabled as u8]),
                    );
                }
                LinkCommand::Begin => {
                    if matches!(state, LinkState::Connected)
                        && !capturing
                        && !start_pending
                        && gate.load(Ordering::Acquire)
                    {
                        session = fresh_session();
                        pending.clear();
                        next_sequence = 1;
                        start_pending = true;
                        start_sent = None;
                        start_deadline = Some(now + Duration::from_millis(250));
                    }
                }
                LinkCommand::State {
                    modifiers,
                    bitmap,
                    press,
                    captured_us,
                } if capturing && gate.load(Ordering::Acquire) => {
                    if pending.len() >= 32 || next_sequence == u32::MAX {
                        fail(&event_tx, &mut state, "transition queue overflow");
                        send_stop(&mut port, &path, session, epoch);
                        pending.clear();
                        capturing = false;
                    } else {
                        pending.push_back(Pending {
                            sequence: next_sequence,
                            payload: state_payload(modifiers, bitmap),
                            press,
                            captured_us,
                            last_sent: None,
                            queued_at: now,
                        });
                        next_sequence = next_sequence + 1;
                    }
                }
                _ => {}
            }
        }
        if port.is_some() {
            // STATUS freshness is required during capture too. A forwards these
            // app-origin probes to B; HEARTBEAT only renews the capture lease.
            if now.duration_since(hello_at) >= Duration::from_millis(250) {
                // Reapply settings before probing: either bridge may have rebooted
                // since the last connection, resetting its debug flag.
                if !send_or_drop(
                    &mut port,
                    &path,
                    "write DEBUG",
                    Packet::new(MessageType::Debug, session, epoch, 0, vec![debug as u8]),
                ) || !send_or_drop(
                    &mut port,
                    &path,
                    "write HELLO",
                    Packet::new(MessageType::Hello, 0, 0, 0, vec![]),
                ) {
                    continue;
                }
                hello_at = now;
            }
            if capturing && now.duration_since(heartbeat_at) >= Duration::from_millis(50) {
                if !send_or_drop(
                    &mut port,
                    &path,
                    "write HEARTBEAT",
                    Packet::new(MessageType::Heartbeat, session, epoch, 0, vec![]),
                ) {
                    continue;
                }
                heartbeat_at = now;
            }
            if debug && capturing && now.duration_since(sync_at) >= Duration::from_secs(1) {
                let t0 = anchor.elapsed().as_micros() as u64;
                if !send_or_drop(
                    &mut port,
                    &path,
                    "write SYNC",
                    Packet::new(
                        MessageType::Sync,
                        session,
                        epoch,
                        0,
                        t0.to_le_bytes().to_vec(),
                    ),
                ) {
                    continue;
                }
                sync_at = now;
            }
            if start_pending
                && (start_sent.is_none()
                    || now.duration_since(start_sent.unwrap()) >= Duration::from_millis(10))
            {
                if !send_or_drop(
                    &mut port,
                    &path,
                    "write START",
                    Packet::new(MessageType::Start, session, epoch, 0, vec![]),
                ) {
                    continue;
                }
                start_sent = Some(now);
            }
            if let Some(oldest) = pending.front_mut() {
                if oldest.expired(now) {
                    fail(&event_tx, &mut state, "transition timeout");
                    send_stop(&mut port, &path, session, epoch);
                    pending.clear();
                    capturing = false;
                    continue;
                }
                if oldest.last_sent.is_none()
                    || now.duration_since(oldest.last_sent.unwrap()) >= Duration::from_millis(10)
                {
                    if !send_or_drop(
                        &mut port,
                        &path,
                        "write STATE",
                        oldest.packet(session, epoch),
                    ) {
                        continue;
                    }
                    oldest.last_sent = Some(now);
                }
            }
            let mut buf = [0u8; 256];
            let read_result = port.as_mut().unwrap().read(&mut buf);
            match read_result {
                Ok(count) => {
                    for &byte in &buf[..count] {
                        if let Some(Ok(packet)) = decoder.push(byte) {
                            handle_packet(
                                packet,
                                &mut state,
                                &event_tx,
                                &mut epoch,
                                &mut capturing,
                                &mut start_pending,
                                &mut pending,
                                session,
                                debug,
                                anchor,
                            );
                        }
                    }
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::TimedOut
                        || error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => {
                    report_serial_failure(&path, "read", &error);
                    close_port(&mut port, &path);
                }
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
}

fn handle_packet(
    packet: Packet,
    state: &mut LinkState,
    tx: &Sender<LinkEvent>,
    epoch: &mut u64,
    capturing: &mut bool,
    start_pending: &mut bool,
    pending: &mut VecDeque<Pending>,
    session: u64,
    debug: bool,
    anchor: Instant,
) {
    match packet.kind {
        MessageType::Status if packet.payload.len() == 16 => {
            if *capturing && packet.epoch != *epoch {
                fail(tx, state, "bridge session changed");
                *capturing = false;
                pending.clear();
                return;
            }
            *epoch = packet.epoch;
            let ready = packet.payload[0] != 0;
            if (*capturing || *start_pending) && (!ready || packet.payload[2..4] != [0, 0]) {
                fail(tx, state, "bridge is no longer ready");
                *capturing = false;
                *start_pending = false;
                pending.clear();
                return;
            }
            let _ = tx.send(LinkEvent::Leds(packet.payload[1]));
            let a_boot_us = u32::from_le_bytes(packet.payload[4..8].try_into().unwrap());
            let b_boot_us = u32::from_le_bytes(packet.payload[8..12].try_into().unwrap());
            let radio_ready_us = u32::from_le_bytes(packet.payload[12..16].try_into().unwrap());
            let _ = tx.send(LinkEvent::Status {
                a_boot_us,
                b_boot_us,
                radio_ready_us,
            });
            if ready && !*capturing {
                *state = LinkState::Connected;
                let _ = tx.send(LinkEvent::State(state.clone()));
            } else if !ready && !*capturing {
                *state = LinkState::Searching;
                let _ = tx.send(LinkEvent::State(state.clone()));
            }
        }
        MessageType::Ready
            if *start_pending
                && session != 0
                && packet.session == session
                && packet.epoch == *epoch
                && packet.payload.is_empty() =>
        {
            *start_pending = false;
            *capturing = true;
            *state = LinkState::Capturing;
            let _ = tx.send(LinkEvent::State(state.clone()));
            let _ = tx.send(LinkEvent::CaptureAuthorized);
        }
        MessageType::Ack if *capturing && packet.session == session && packet.epoch == *epoch => {
            let hid_us = if packet.payload.len() == 8 {
                Some(u64::from_le_bytes(packet.payload[..8].try_into().unwrap()))
            } else {
                None
            };
            while pending
                .front()
                .is_some_and(|value| value.sequence <= packet.sequence)
            {
                let value = pending.pop_front().unwrap();
                let _ = tx.send(LinkEvent::Ack {
                    sequence: value.sequence,
                    press: value.press,
                    captured_us: value.captured_us,
                    hid_us,
                });
            }
        }
        MessageType::Leds if packet.payload.len() == 1 => {
            let _ = tx.send(LinkEvent::Leds(packet.payload[0]));
        }
        MessageType::SyncReply
            if debug
                && packet.payload.len() == 24
                && packet.session == session
                && packet.epoch == *epoch =>
        {
            let t0 = u64::from_le_bytes(packet.payload[0..8].try_into().unwrap());
            let b_recv = u64::from_le_bytes(packet.payload[8..16].try_into().unwrap());
            let b_send = u64::from_le_bytes(packet.payload[16..24].try_into().unwrap());
            let t3 = anchor.elapsed().as_micros() as u64;
            if t3 >= t0 && b_send >= b_recv && t3 - t0 >= b_send - b_recv {
                let offset =
                    ((b_recv as f64 + b_send as f64) / 2.0) - ((t0 as f64 + t3 as f64) / 2.0);
                let _ = tx.send(LinkEvent::ClockOffset {
                    offset_us: offset,
                    round_trip_us: (t3 - t0) - (b_send - b_recv),
                });
            }
        }
        MessageType::Error
            if session != 0
                && (*capturing || *start_pending)
                && packet.payload.len() == 2
                && packet.session == session
                && packet.epoch == *epoch =>
        {
            let code = u16::from_le_bytes(packet.payload[..2].try_into().unwrap());
            fail(tx, state, &format!("bridge error {code}"));
            *capturing = false;
            pending.clear();
        }
        _ => {}
    }
}
fn send(port: &mut Box<dyn serialport::SerialPort>, packet: Packet) -> std::io::Result<()> {
    port.write_all(&packet.encode_cdc().map_err(std::io::Error::other)?)
}
fn send_or_drop(
    port: &mut Option<Box<dyn serialport::SerialPort>>,
    path: &str,
    operation: &str,
    packet: Packet,
) -> bool {
    let result = match port.as_mut() {
        Some(port) => send(port, packet),
        None => return false,
    };
    if let Err(error) = result {
        report_serial_failure(path, operation, &error);
        close_port(port, path);
        false
    } else {
        true
    }
}
fn send_stop(
    port: &mut Option<Box<dyn serialport::SerialPort>>,
    path: &str,
    session: u64,
    epoch: u64,
) {
    if session == 0 {
        return;
    }
    let _ = send_or_drop(
        port,
        path,
        "write STOP",
        Packet::new(MessageType::Stop, session, epoch, 0, vec![]),
    );
}
fn close_port(port: &mut Option<Box<dyn serialport::SerialPort>>, path: &str) {
    if let Some(port) = port.take()
        && let Err(error) = port.clear(serialport::ClearBuffer::All)
    {
        report_serial_failure(path, "clear before close", &error);
    }
}
fn report_serial_failure(path: &str, operation: &str, error: &dyn std::fmt::Display) {
    eprintln!("serial {operation} on {path} failed: {error}");
}
fn fail(tx: &Sender<LinkEvent>, state: &mut LinkState, detail: &str) {
    *state = LinkState::Error(detail.into());
    let _ = tx.send(LinkEvent::State(state.clone()));
}
fn fresh_session() -> u64 {
    use std::io::Read as _;
    let mut bytes = [0u8; 8];
    let random = std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .is_ok();
    let value = if random {
        u64::from_le_bytes(bytes)
    } else {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(1, |value| value.as_nanos() as u64)
            ^ ((std::process::id() as u64) << 32)
    };
    if value == 0 { 1 } else { value }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn pending(at: Instant, press: bool) -> Pending {
        Pending {
            sequence: 1,
            payload: state_payload(0, [0; 32]),
            press,
            captured_us: 0,
            last_sent: None,
            queued_at: at,
        }
    }

    #[test]
    fn retries_do_not_extend_transition_deadline() {
        let start = Instant::now();
        let mut item = pending(start, true);
        for ms in (0..250).step_by(10) {
            item.last_sent = Some(start + Duration::from_millis(ms));
            assert!(!item.expired(start + Duration::from_millis(ms)));
        }
        assert!(item.expired(start + Duration::from_millis(250)));
    }

    #[test]
    fn press_and_release_preserve_sample_flag_on_wire() {
        for press in [false, true] {
            let packet = pending(Instant::now(), press).packet(3, 4);
            let mut decoder = CdcDecoder::default();
            let decoded = packet
                .encode_cdc()
                .unwrap()
                .into_iter()
                .filter_map(|byte| decoder.push(byte))
                .last()
                .unwrap()
                .unwrap();
            assert_eq!(decoded.keypress_sample, press);
        }
    }

    #[test]
    fn ready_requires_pending_current_activation_and_is_not_replayed() {
        let (tx, rx) = mpsc::channel();
        let mut state = LinkState::Connected;
        let mut epoch = 4;
        let mut capturing = false;
        let mut starting = true;
        let mut queue = VecDeque::new();
        for (session, expected) in [(2, false), (3, true), (3, true)] {
            handle_packet(
                Packet::new(MessageType::Ready, session, 4, 0, vec![]),
                &mut state,
                &tx,
                &mut epoch,
                &mut capturing,
                &mut starting,
                &mut queue,
                3,
                false,
                Instant::now(),
            );
            assert_eq!(capturing, expected);
        }
        assert_eq!(
            rx.try_iter()
                .filter(|event| matches!(event, LinkEvent::CaptureAuthorized))
                .count(),
            1
        );
    }

    #[test]
    fn stale_error_does_not_abort_current_session() {
        let (tx, _) = mpsc::channel();
        let mut state = LinkState::Capturing;
        let mut epoch = 4;
        let mut capturing = true;
        let mut starting = false;
        let mut queue = VecDeque::from([pending(Instant::now(), true)]);
        for (session, remains_active) in [(2, true), (3, false)] {
            handle_packet(
                Packet::new(MessageType::Error, session, 4, 0, vec![3, 0]),
                &mut state,
                &tx,
                &mut epoch,
                &mut capturing,
                &mut starting,
                &mut queue,
                3,
                false,
                Instant::now(),
            );
            assert_eq!(capturing, remains_active);
        }
        assert!(queue.is_empty());
        assert!(matches!(state, LinkState::Error(_)));
    }

    #[test]
    fn idle_error_does_not_trigger_a_stop_or_state_change() {
        let (tx, rx) = mpsc::channel();
        let mut state = LinkState::Connected;
        let mut epoch = 4;
        let mut capturing = false;
        let mut starting = false;
        let mut queue = VecDeque::new();
        handle_packet(
            Packet::new(MessageType::Error, 0, 4, 0, vec![3, 0]),
            &mut state,
            &tx,
            &mut epoch,
            &mut capturing,
            &mut starting,
            &mut queue,
            0,
            false,
            Instant::now(),
        );
        assert_eq!(state, LinkState::Connected);
        assert!(rx.try_recv().is_err());
    }
    #[cfg(unix)]
    #[test]
    fn active_serial_disconnect_revokes_gate_reports_error_and_reopens() {
        use serialport::SerialPort;
        use std::os::unix::fs::symlink;

        let (mut remote, local) = serialport::TTYPort::pair().unwrap();
        let link = std::env::temp_dir().join(format!(
            "keyboard-bridge-link-{}-{}",
            std::process::id(),
            fresh_session()
        ));
        symlink(local.name().unwrap(), &link).unwrap();
        let path = link.to_string_lossy().into_owned();
        drop(local);
        remote.set_timeout(Duration::from_millis(100)).unwrap();
        let (commands, command_rx) = mpsc::channel();
        let (events, event_rx) = mpsc::channel();
        let gate = Arc::new(AtomicBool::new(false));
        let lease = Arc::new(AtomicU64::new(0));
        let anchor = Instant::now();
        let worker_gate = gate.clone();
        let worker_lease = lease.clone();
        let worker = thread::spawn(move || {
            run(
                path,
                false,
                command_rx,
                events,
                worker_gate,
                worker_lease,
                anchor,
            )
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut decoder = CdcDecoder::default();
        let mut began = false;
        let mut authorized = false;
        while Instant::now() < deadline && !authorized {
            lease.store(anchor.elapsed().as_millis() as u64, Ordering::Release);
            let mut buf = [0; 256];
            if let Ok(count) = remote.read(&mut buf) {
                for &byte in &buf[..count] {
                    if let Some(Ok(packet)) = decoder.push(byte) {
                        let reply = match packet.kind {
                            MessageType::Hello => {
                                let mut payload = vec![0; 16];
                                payload[0] = 1;
                                Some(Packet::new(MessageType::Status, 0, 4, 0, payload))
                            }
                            MessageType::Start => Some(Packet::new(
                                MessageType::Ready,
                                packet.session,
                                4,
                                0,
                                vec![],
                            )),
                            _ => None,
                        };
                        if let Some(reply) = reply {
                            remote.write_all(&reply.encode_cdc().unwrap()).unwrap();
                        }
                    }
                }
            }
            for event in event_rx.try_iter() {
                match event {
                    LinkEvent::State(LinkState::Connected) if !began => {
                        began = true;
                        gate.store(true, Ordering::Release);
                        commands.send(LinkCommand::Begin).unwrap();
                    }
                    LinkEvent::CaptureAuthorized => authorized = true,
                    _ => {}
                }
            }
        }
        drop(remote);
        let mut disconnected = false;
        let deadline = Instant::now() + Duration::from_millis(120);
        while Instant::now() < deadline {
            if let Ok(LinkEvent::State(LinkState::Error(_))) =
                event_rx.recv_timeout(Duration::from_millis(10))
            {
                disconnected = true;
                break;
            }
        }
        let revoked = !gate.load(Ordering::Acquire);
        let (mut recovered_remote, recovered_local) = serialport::TTYPort::pair().unwrap();
        recovered_remote
            .set_timeout(Duration::from_millis(10))
            .unwrap();
        std::fs::remove_file(&link).unwrap();
        symlink(recovered_local.name().unwrap(), &link).unwrap();
        drop(recovered_local);

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut decoder = CdcDecoder::default();
        let mut reconnected = false;
        while Instant::now() < deadline && !reconnected {
            let mut buf = [0; 256];
            if let Ok(count) = recovered_remote.read(&mut buf) {
                for &byte in &buf[..count] {
                    if let Some(Ok(packet)) = decoder.push(byte)
                        && packet.kind == MessageType::Hello
                    {
                        let mut payload = vec![0; 16];
                        payload[0] = 1;
                        recovered_remote
                            .write_all(
                                &Packet::new(MessageType::Status, 0, 5, 0, payload)
                                    .encode_cdc()
                                    .unwrap(),
                            )
                            .unwrap();
                    }
                }
            }
            reconnected |= event_rx
                .try_iter()
                .any(|event| matches!(event, LinkEvent::State(LinkState::Connected)));
        }
        commands.send(LinkCommand::Shutdown).unwrap();
        worker.join().unwrap();
        std::fs::remove_file(link).unwrap();
        assert!(authorized, "fake CDC peer did not reach capture readiness");
        assert!(
            disconnected,
            "active I/O failure must emit an error before the UI lease expires"
        );
        assert!(
            revoked,
            "serial failure must synchronously revoke input authorization"
        );
        assert!(
            reconnected,
            "worker did not reopen the replacement CDC endpoint"
        );
    }

    #[cfg(unix)]
    #[test]
    fn stop_rearms_the_worker_for_a_second_activation() {
        use serialport::SerialPort;

        let (mut remote, local) = serialport::TTYPort::pair().unwrap();
        let path = local.name().unwrap();
        drop(local);
        remote.set_timeout(Duration::from_millis(10)).unwrap();
        let (commands, command_rx) = mpsc::channel();
        let (events, event_rx) = mpsc::channel();
        let gate = Arc::new(AtomicBool::new(false));
        let lease = Arc::new(AtomicU64::new(0));
        let anchor = Instant::now();
        let worker = {
            let gate = gate.clone();
            let lease = lease.clone();
            thread::spawn(move || run(path, false, command_rx, events, gate, lease, anchor))
        };

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut decoder = CdcDecoder::default();
        let mut sessions = Vec::new();
        let mut stop_sessions = Vec::new();
        let mut peer_epoch = 4;
        let mut phase = 0;
        while Instant::now() < deadline && phase != 3 {
            lease.store(anchor.elapsed().as_millis() as u64, Ordering::Release);
            let mut buf = [0; 256];
            if let Ok(count) = remote.read(&mut buf) {
                for &byte in &buf[..count] {
                    if let Some(Ok(packet)) = decoder.push(byte) {
                        let reply = match packet.kind {
                            MessageType::Hello => {
                                let mut payload = vec![0; 16];
                                payload[0] = 1;
                                Some(Packet::new(MessageType::Status, 0, peer_epoch, 0, payload))
                            }
                            MessageType::Start => {
                                if !sessions.contains(&packet.session) {
                                    sessions.push(packet.session);
                                }
                                Some(Packet::new(
                                    MessageType::Ready,
                                    packet.session,
                                    peer_epoch,
                                    0,
                                    vec![],
                                ))
                            }
                            MessageType::Stop => {
                                stop_sessions.push(packet.session);
                                peer_epoch = 5;
                                None
                            }
                            _ => None,
                        };
                        if let Some(reply) = reply {
                            remote.write_all(&reply.encode_cdc().unwrap()).unwrap();
                        }
                    }
                }
            }
            for event in event_rx.try_iter() {
                match (phase, event) {
                    (0, LinkEvent::State(LinkState::Connected)) => {
                        gate.store(true, Ordering::Release);
                        commands.send(LinkCommand::Begin).unwrap();
                        phase = 1;
                    }
                    (1, LinkEvent::CaptureAuthorized) => {
                        gate.store(false, Ordering::Release);
                        commands.send(LinkCommand::Stop).unwrap();
                        phase = 2;
                    }
                    (2, LinkEvent::State(LinkState::Connected)) => {
                        gate.store(true, Ordering::Release);
                        commands.send(LinkCommand::Begin).unwrap();
                        phase = 4;
                    }
                    (4, LinkEvent::CaptureAuthorized) => phase = 3,
                    _ => {}
                }
            }
        }
        commands.send(LinkCommand::Shutdown).unwrap();
        worker.join().unwrap();
        assert_eq!(phase, 3, "the second Begin did not reach READY");
        assert_eq!(sessions.len(), 2, "each activation needs a fresh session");
        assert_ne!(sessions[0], sessions[1]);
        assert!(
            stop_sessions.iter().all(|session| *session != 0),
            "the worker must not send STOP for an idle session"
        );
    }

    #[test]
    fn status_restores_target_lock_leds() {
        let (tx, rx) = mpsc::channel();
        let mut state = LinkState::Searching;
        let mut epoch = 0;
        let mut capturing = false;
        let mut start_pending = false;
        let mut pending = VecDeque::new();
        for leds in [3, 0] {
            let mut payload = vec![0; 16];
            payload[0] = 1;
            payload[1] = leds;
            handle_packet(Packet::new(MessageType::Status, 0, 4, 0, payload),
                &mut state, &tx, &mut epoch, &mut capturing, &mut start_pending,
                &mut pending, 0, false, Instant::now());
            assert!(rx.try_iter().any(|event| matches!(event, LinkEvent::Leds(value) if value == leds)));
        }
    }

    #[cfg(unix)]
    #[test]
    fn capture_keeps_status_probes_alive() {
        use serialport::SerialPort;

        let (mut remote, local) = serialport::TTYPort::pair().unwrap();
        let path = local.name().unwrap();
        drop(local);
        remote.set_timeout(Duration::from_millis(10)).unwrap();
        let (commands, command_rx) = mpsc::channel();
        let (events, event_rx) = mpsc::channel();
        let gate = Arc::new(AtomicBool::new(false));
        let lease = Arc::new(AtomicU64::new(0));
        let anchor = Instant::now();
        let worker = {
            let gate = gate.clone();
            let lease = lease.clone();
            thread::spawn(move || run(path, true, command_rx, events, gate, lease, anchor))
        };
        let mut decoder = CdcDecoder::default();
        let mut began = false;
        let mut active_since = None;
        let mut last_hello = Instant::now();
        let mut active_probes = 0;
        let mut debug_enabled = false;
        let mut sync_requests = 0;
        let mut timed_out = false;
        let mut errors = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline {
            lease.store(anchor.elapsed().as_millis() as u64, Ordering::Release);
            let mut buf = [0; 256];
            if let Ok(count) = remote.read(&mut buf) {
                for &byte in &buf[..count] {
                    if let Some(Ok(packet)) = decoder.push(byte) {
                        let reply = match packet.kind {
                            MessageType::Debug => {
                                debug_enabled = packet.payload == [1];
                                None
                            }
                            MessageType::Sync => {
                                sync_requests += 1;
                                if debug_enabled {
                                    // A peer restart can clear the setting after it
                                    // was successfully synchronized once.
                                    if sync_requests == 1 { debug_enabled = false; }
                                    None
                                } else {
                                    Some(Packet::new(MessageType::Error, packet.session, 4, 0, vec![1, 0]))
                                }
                            }
                            MessageType::Hello => {
                                last_hello = Instant::now();
                                if active_since.is_some() { active_probes += 1; }
                                let mut payload = vec![0; 16];
                                // A preserves ready during the active session.
                                payload[0] = 1;
                                Some(Packet::new(MessageType::Status, 0, 4, 0, payload))
                            }
                            MessageType::Start => Some(Packet::new(
                                MessageType::Ready, packet.session, 4, 0, vec![])),
                            _ => None,
                        };
                        if let Some(reply) = reply {
                            remote.write_all(&reply.encode_cdc().unwrap()).unwrap();
                        }
                    }
                }
            }
            for event in event_rx.try_iter() {
                match event {
                    LinkEvent::State(LinkState::Connected) if !began => {
                        began = true;
                        gate.store(true, Ordering::Release);
                        commands.send(LinkCommand::Begin).unwrap();
                    }
                    LinkEvent::CaptureAuthorized => active_since = Some(Instant::now()),
                    LinkEvent::State(LinkState::Error(error)) => errors.push(error),
                    _ => {}
                }
            }
            if let Some(started) = active_since {
                if last_hello.elapsed() > Duration::from_millis(750) {
                    timed_out = true;
                    break;
                }
                if started.elapsed() >= Duration::from_millis(2200) { break; }
            }
        }
        let still_authorized = gate.load(Ordering::Acquire);
        gate.store(false, Ordering::Release);
        lease.store(0, Ordering::Release);
        thread::sleep(Duration::from_millis(30));
        for event in event_rx.try_iter() {
            if let LinkEvent::State(LinkState::Error(error)) = event { errors.push(error); }
        }
        commands.send(LinkCommand::Shutdown).unwrap();
        worker.join().unwrap();
        assert!(active_since.is_some(), "capture never started");
        assert!(!timed_out, "A would expire B STATUS freshness during capture");
        assert!(active_probes >= 4, "capture must keep probing peer status");
        assert!(sync_requests >= 2, "debug must recover after the peer clears its setting");
        assert!(still_authorized);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    #[cfg(unix)]
    #[test]
    fn activation_timeout_is_reported_once_and_revokes_the_gate() {
        use serialport::SerialPort;

        let (mut remote, local) = serialport::TTYPort::pair().unwrap();
        let path = local.name().unwrap();
        drop(local);
        remote.set_timeout(Duration::from_millis(10)).unwrap();
        let (commands, command_rx) = mpsc::channel();
        let (events, event_rx) = mpsc::channel();
        let gate = Arc::new(AtomicBool::new(false));
        let lease = Arc::new(AtomicU64::new(0));
        let anchor = Instant::now();
        let worker = {
            let gate = gate.clone();
            let lease = lease.clone();
            thread::spawn(move || run(path, false, command_rx, events, gate, lease, anchor))
        };

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut decoder = CdcDecoder::default();
        let mut began = false;
        let mut timeout_count = 0;
        let mut observe_until = None;
        while Instant::now() < deadline {
            lease.store(anchor.elapsed().as_millis() as u64, Ordering::Release);
            let mut buf = [0; 256];
            if let Ok(count) = remote.read(&mut buf) {
                for &byte in &buf[..count] {
                    if let Some(Ok(packet)) = decoder.push(byte) {
                        if packet.kind == MessageType::Hello {
                            let mut payload = vec![0; 16];
                            payload[0] = 1;
                            remote
                                .write_all(
                                    &Packet::new(MessageType::Status, 0, 4, 0, payload)
                                        .encode_cdc()
                                        .unwrap(),
                                )
                                .unwrap();
                        }
                    }
                }
            }
            for event in event_rx.try_iter() {
                match event {
                    LinkEvent::State(LinkState::Connected) if !began => {
                        began = true;
                        gate.store(true, Ordering::Release);
                        commands.send(LinkCommand::Begin).unwrap();
                    }
                    LinkEvent::State(LinkState::Error(detail))
                        if detail == "activation timeout" =>
                    {
                        timeout_count += 1;
                        observe_until = Some(Instant::now() + Duration::from_millis(50));
                    }
                    _ => {}
                }
            }
            if observe_until.is_some_and(|until| Instant::now() >= until) {
                break;
            }
        }
        commands.send(LinkCommand::Shutdown).unwrap();
        worker.join().unwrap();
        assert!(began, "fake CDC peer did not reach Connected");
        assert_eq!(timeout_count, 1, "timeout must not spin and flood errors");
        assert!(
            !gate.load(Ordering::Acquire),
            "activation timeout must revoke input authorization"
        );
    }
}

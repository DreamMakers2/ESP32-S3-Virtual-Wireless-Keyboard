use crate::keymap::hid_usage;
use evdev::{Device, EventSummary};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub enum InputCommand {
    Start { path: PathBuf, generation: u64 },
    Stop { generation: u64 },
    Shutdown,
}
#[derive(Clone, Debug)]
pub enum InputEvent {
    Started {
        generation: u64,
    },
    Key {
        generation: u64,
        code: u16,
        pressed: bool,
        repeat: bool,
        received_us: u64,
    },
    Fault {
        generation: u64,
        detail: String,
    },
    Stopped {
        generation: u64,
    },
}

pub fn spawn(
    command_rx: Receiver<InputCommand>,
    event_tx: Sender<InputEvent>,
    gate: Arc<AtomicBool>,
    anchor: Instant,
) {
    thread::spawn(move || {
        let mut device: Option<Device> = None;
        let mut generation = 0;
        let mut preheld = HashSet::new();
        let mut stopping = false;
        loop {
            while let Ok(command) = command_rx.try_recv() {
                match command {
                    InputCommand::Start {
                        path,
                        generation: next_generation,
                    } => {
                        if next_generation < generation {
                            continue;
                        }
                        if let Some(mut opened) = device.take() {
                            let _ = opened.ungrab();
                        }
                        generation = next_generation;
                        stopping = false;
                        preheld.clear();
                        match Device::open(&path) {
                            Ok(mut opened) => match opened.grab() {
                                Ok(()) if gate.load(Ordering::Acquire) => {
                                    match opened.get_key_state().and_then(|keys| {
                                        opened.set_nonblocking(true)?;
                                        Ok(keys)
                                    }) {
                                        Ok(keys) => {
                                            preheld = keys.iter().map(|key| key.code()).collect();
                                            device = Some(opened);
                                            let _ =
                                                event_tx.send(InputEvent::Started { generation });
                                        }
                                        Err(error) => {
                                            let _ = opened.ungrab();
                                            gate.store(false, Ordering::Release);
                                            let _ = event_tx.send(InputEvent::Fault {
                                                generation,
                                                detail: format!("input setup: {error}"),
                                            });
                                        }
                                    }
                                }
                                Ok(()) => {
                                    let _ = opened.ungrab();
                                }
                                Err(error) => {
                                    let _ = event_tx.send(InputEvent::Fault {
                                        generation,
                                        detail: format!("input grab: {error}"),
                                    });
                                }
                            },
                            Err(error) => {
                                let _ = event_tx.send(InputEvent::Fault {
                                    generation,
                                    detail: format!("input access: {error}"),
                                });
                            }
                        }
                    }
                    InputCommand::Stop {
                        generation: next_generation,
                    } if next_generation >= generation => {
                        generation = next_generation;
                        if let Some(mut opened) = device.take() {
                            let _ = opened.ungrab();
                        }
                        preheld.clear();
                        stopping = false;
                        let _ = event_tx.send(InputEvent::Stopped { generation });
                    }
                    InputCommand::Shutdown => return,
                    _ => {}
                }
            }
            if stopping {
                if let Some(mut opened) = device.take() {
                    let _ = opened.ungrab();
                }
                let _ = event_tx.send(InputEvent::Stopped { generation });
                stopping = false;
            }
            let mut lost = None;
            if device.is_some() && !gate.load(Ordering::Acquire) {
                stopping = true;
            }
            if let Some(opened) = device.as_mut() {
                let result = match opened.fetch_events() {
                    Ok(events) => {
                        for event in events {
                            if let EventSummary::Key(_, key, value) = event.destructure() {
                                let code = key.code();
                                if should_forward(&mut preheld, code, value, &gate) {
                                    let _ = event_tx.send(InputEvent::Key {
                                        generation,
                                        code,
                                        pressed: value != 0,
                                        repeat: value == 2,
                                        received_us: anchor.elapsed().as_micros() as u64,
                                    });
                                }
                            }
                        }
                        None
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => None,
                    Err(error) => Some(error.to_string()),
                };
                lost = result;
            }
            if let Some(error) = lost {
                let _ = event_tx.send(InputEvent::Fault {
                    generation,
                    detail: format!("input lost: {error}"),
                });
                if let Some(mut opened) = device.take() {
                    let _ = opened.ungrab();
                }
            }
            thread::sleep(Duration::from_millis(2));
        }
    });
}
fn forwarding_allowed(gate: &AtomicBool) -> bool {
    gate.load(Ordering::Acquire)
}

/// Reject the release of a key held before capture and every queued event once
/// the UI has revoked its authorization.
fn should_forward(preheld: &mut HashSet<u16>, code: u16, value: i32, gate: &AtomicBool) -> bool {
    if preheld.contains(&code) {
        if value == 0 {
            preheld.remove(&code);
        }
        return false;
    }
    (value == 0 || value == 1 || value == 2) && forwarding_allowed(gate)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HidState {
    pub modifiers: u8,
    pub bitmap: [u8; 32],
}
impl HidState {
    pub fn apply(&mut self, linux_key: u16, pressed: bool) -> bool {
        let Some((usage, modifier)) = hid_usage(linux_key) else {
            return false;
        };
        if modifier != 0 {
            let before = self.modifiers;
            if pressed {
                self.modifiers |= modifier;
            } else {
                self.modifiers &= !modifier;
            }
            return before != self.modifiers;
        }
        let byte = (usage / 8) as usize;
        let bit = 1 << (usage % 8);
        let before = self.bitmap[byte];
        if pressed {
            self.bitmap[byte] |= bit;
        } else {
            self.bitmap[byte] &= !bit;
        }
        before != self.bitmap[byte]
    }
    pub fn is_empty(&self) -> bool {
        self.modifiers == 0 && self.bitmap.iter().all(|v| *v == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hid_state_tracks_full_bitmap() {
        let mut s = HidState::default();
        assert!(s.apply(30, true));
        assert!(!s.is_empty());
        assert!(s.apply(30, false));
        assert!(s.is_empty());
    }
    #[test]
    fn focus_revocation_closes_the_input_gate() {
        let gate = AtomicBool::new(true);
        assert!(forwarding_allowed(&gate));
        gate.store(false, Ordering::Release);
        assert!(!forwarding_allowed(&gate));
    }
    #[test]
    fn focus_revocation_blocks_queued_events_and_new_capture_ignores_preheld_keys() {
        let gate = AtomicBool::new(true);
        let mut preheld = HashSet::from([30]);
        // A key held before capture has to be released, then pressed again.
        assert!(!should_forward(&mut preheld, 30, 2, &gate));
        assert!(preheld.contains(&30));
        assert!(!should_forward(&mut preheld, 30, 0, &gate));
        assert!(should_forward(&mut preheld, 30, 1, &gate));
        assert!(should_forward(&mut preheld, 30, 2, &gate));
        // A focus-loss store happens before the event-drain gate check.
        gate.store(false, Ordering::Release);
        assert!(!should_forward(&mut preheld, 31, 1, &gate));
        assert!(!should_forward(&mut preheld, 31, 2, &gate));
    }
}

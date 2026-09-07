# Bridge protocol v1

This is the implementation contract shared by the two firmware targets and app.
Integers are little-endian; encode fields explicitly, never transmit C structs.
Packets are at most 128 bytes. CRC is standard reflected CRC-32/ISO-HDLC
(polynomial 0xedb88320, initial/final xor 0xffffffff), over header and payload.

| Offset | Bytes | Field |
|---|---|---|
| 0 | 1 | version = 1 |
| 1 | 1 | message type |
| 2 | 2 | flags (bit 0 = keypress sample; all other bits zero) |
| 4 | 8 | session (nonzero random capture ID; zero for idle discovery) |
| 12 | 8 | receiver epoch (B's current random challenge) |
| 20 | 4 | transition sequence (starts at 1; no wrap within session) |
| 24 | 2 | payload length, at most 96 |
| 26 | 2 | reserved = 0 |
| 28 | N | payload |
| 28+N | 4 | CRC32 |

CDC uses COBS(packet) followed by zero. Radio carries packet directly, encrypted
unicast only. Reject bad lengths/version/CRC/reserved flags, unknown messages and
wrong peer MAC before processing. Oversized CDC input discards through delimiter.

| Type | Name | Payload / meaning |
|---|---|---|
| 1 | HELLO | Empty. Readiness/liveness query in idle or capture; never arms capture. |
| 2 | STATUS | u8 ready (radio peer AND mounted, unsuspended HID), u8 LEDs, u16 error, u32 A boot us, u32 B boot us, u32 radio-ready us. Header epoch is current B challenge. |
| 3 | START | Empty. Request capture using fresh session and current epoch. |
| 4 | READY | Empty. START accepted only after released HID report completion. |
| 5 | STATE | u8 modifiers + 32-byte usage bitmap (bit u = HID usage u; modifier usages represented only in modifier byte). |
| 6 | ACK | Empty normally; u64 B HID-completion microseconds when debug enabled. Sequence is highest contiguous successfully completed transition. |
| 7 | HEARTBEAT | Empty. Valid only for active session/epoch. |
| 8 | STOP | Empty. Release and invalidate active session. |
| 9 | DEBUG | u8 enabled, 0 or 1. A forwards current setting to B. |
| 10 | LEDS | u8 target lock LEDs. B → A → app. |
| 11 | SYNC | u64 Linux monotonic t0 microseconds; debug only. |
| 12 | SYNC_REPLY | u64 echoed t0, u64 B receive us, u64 B send us; debug only. |
| 13 | ERROR | u16 error code: 1 protocol, 2 overflow, 3 timeout, 4 USB, 5 radio, 6 configuration. |
| 14 | IDENTIFY | u8 enabled. Local maintenance only; never accepted over radio. |

STATUS contains the latest target output-report LED byte, including after app
reconnection. The current B implementation leaves STATUS.error at zero; active
failures are detected through epoch/readiness changes and A's ERROR messages.
Boot scalars are the firmware timer readings after local USB/radio initialization
returns. Radio-ready is the separate timer reading when ESP-NOW becomes available;
these are not host USB-enumeration duration measurements.

## Session and transport rules

B creates a nonzero random epoch at boot and every invalidation. START must name
the currently advertised epoch and a nonzero session. Retransmitted identical
START may elicit READY again but must never release already active keys. A stale
START after STOP/timeout/reset is invalid because its epoch no longer matches.
B advertises idle readiness independently of capture. USB unmount/suspend clears
state and invalidates the epoch. On suspend B briefly disconnects and re-enumerates
to discard queued USB transfers; the first report after configuration is released.

Linux assigns ordered transition IDs; A accepts only its next expected sequence,
retains up to 32 unacknowledged states, and retransmits the oldest every 10 ms.
B accepts the next expected state only, with one outstanding HID transfer. An
ahead packet gets the last cumulative ACK; an already completed duplicate gets
that ACK again. Never ACK an uncompleted HID transfer or emit duplicate reports.
Maintain the full bitmap even though USB v1 uses a boot-compatible 6KRO report.

Linux emits HEARTBEAT every 50 ms only while capture remains authorized. A emits
radio heartbeats only while the controller lease is valid (150 ms). B releases
and invalidates after 250 ms without valid liveness, or after 250 ms without
progress on an outstanding transition. Heartbeats cannot extend a progress fault.
Queue overflow fails closed; do not coalesce or silently discard transitions.
STOP bypasses queued key states. A fails the current session when B changes epoch.

HELLO/STATUS run at 250 ms while idle and during capture so A can keep checking
B status freshness. B advertises idle readiness; A preserves established peer
readiness during an active session. Idle readiness excludes active capture;
activating the app requires START/READY. New readiness never re-grabs the keyboard.
Do not use CDC DTR as a substitute for the application heartbeat lease.

## Diagnostics

Retain only boot-duration scalars when debug is off. With debug on, SYNC exchanges
at one-second cadence estimate Linux/B offset from minimum-round-trip samples.
ACK completion timestamps correlate by session, epoch and sequence. Keep only
the latest 20 completed keypress latency samples. Report unavailable until a
valid clock estimate exists; USB completion is not target application receipt.

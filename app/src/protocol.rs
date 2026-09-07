//! Wire-compatible implementation of docs/PROTOCOL.md.
use anyhow::{Result, bail};

pub const VERSION: u8 = 1;
pub const HEADER_LEN: usize = 28;
pub const MAX_PAYLOAD: usize = 96;
pub const MAX_PACKET: usize = HEADER_LEN + MAX_PAYLOAD + 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum MessageType {
    Hello = 1,
    Status = 2,
    Start = 3,
    Ready = 4,
    State = 5,
    Ack = 6,
    Heartbeat = 7,
    Stop = 8,
    Debug = 9,
    Leds = 10,
    Sync = 11,
    SyncReply = 12,
    Error = 13,
    Identify = 14,
}

impl TryFrom<u8> for MessageType {
    type Error = anyhow::Error;
    fn try_from(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Self::Hello),
            2 => Ok(Self::Status),
            3 => Ok(Self::Start),
            4 => Ok(Self::Ready),
            5 => Ok(Self::State),
            6 => Ok(Self::Ack),
            7 => Ok(Self::Heartbeat),
            8 => Ok(Self::Stop),
            9 => Ok(Self::Debug),
            10 => Ok(Self::Leds),
            11 => Ok(Self::Sync),
            12 => Ok(Self::SyncReply),
            13 => Ok(Self::Error),
            14 => Ok(Self::Identify),
            _ => bail!("unknown message type {value}"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Packet {
    pub kind: MessageType,
    pub keypress_sample: bool,
    pub session: u64,
    pub epoch: u64,
    pub sequence: u32,
    pub payload: Vec<u8>,
}

impl Packet {
    pub fn new(
        kind: MessageType,
        session: u64,
        epoch: u64,
        sequence: u32,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            kind,
            keypress_sample: false,
            session,
            epoch,
            sequence,
            payload,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.payload.len() > MAX_PAYLOAD {
            bail!("payload exceeds {MAX_PAYLOAD} bytes");
        }
        let mut raw = Vec::with_capacity(HEADER_LEN + self.payload.len() + 4);
        raw.push(VERSION);
        raw.push(self.kind as u8);
        raw.extend_from_slice(&(u16::from(self.keypress_sample)).to_le_bytes());
        raw.extend_from_slice(&self.session.to_le_bytes());
        raw.extend_from_slice(&self.epoch.to_le_bytes());
        raw.extend_from_slice(&self.sequence.to_le_bytes());
        raw.extend_from_slice(&(self.payload.len() as u16).to_le_bytes());
        raw.extend_from_slice(&0u16.to_le_bytes());
        raw.extend_from_slice(&self.payload);
        raw.extend_from_slice(&crc32(&raw).to_le_bytes());
        Ok(raw)
    }

    pub fn decode(raw: &[u8]) -> Result<Self> {
        if raw.len() < HEADER_LEN + 4 || raw.len() > MAX_PACKET {
            bail!("invalid packet size");
        }
        if raw[0] != VERSION {
            bail!("unsupported protocol version");
        }
        let flags = u16::from_le_bytes([raw[2], raw[3]]);
        if flags & !1 != 0 {
            bail!("reserved flags are set");
        }
        let payload_len = u16::from_le_bytes([raw[24], raw[25]]) as usize;
        if raw[26] != 0
            || raw[27] != 0
            || payload_len > MAX_PAYLOAD
            || raw.len() != HEADER_LEN + payload_len + 4
        {
            bail!("bad packet length or reserved bytes");
        }
        let expected = u32::from_le_bytes(raw[raw.len() - 4..].try_into().unwrap());
        if crc32(&raw[..raw.len() - 4]) != expected {
            bail!("CRC mismatch");
        }
        Ok(Self {
            kind: raw[1].try_into()?,
            keypress_sample: flags & 1 != 0,
            session: u64::from_le_bytes(raw[4..12].try_into().unwrap()),
            epoch: u64::from_le_bytes(raw[12..20].try_into().unwrap()),
            sequence: u32::from_le_bytes(raw[20..24].try_into().unwrap()),
            payload: raw[HEADER_LEN..HEADER_LEN + payload_len].to_vec(),
        })
    }

    pub fn encode_cdc(&self) -> Result<Vec<u8>> {
        let raw = self.encode()?;
        let mut framed = cobs_encode(&raw);
        framed.push(0);
        Ok(framed)
    }
}

pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

pub fn cobs_encode(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len() + input.len() / 254 + 1);
    let mut code_index = 0usize;
    output.push(0);
    let mut code = 1u8;
    for &byte in input {
        if byte == 0 {
            output[code_index] = code;
            code_index = output.len();
            output.push(0);
            code = 1;
        } else {
            output.push(byte);
            code = code.wrapping_add(1);
            if code == 0xff {
                output[code_index] = code;
                code_index = output.len();
                output.push(0);
                code = 1;
            }
        }
    }
    output[code_index] = code;
    output
}

pub fn cobs_decode(input: &[u8]) -> Result<Vec<u8>> {
    if input.is_empty() {
        bail!("empty COBS frame");
    }
    let mut output = Vec::with_capacity(input.len());
    let mut at = 0;
    while at < input.len() {
        let code = input[at] as usize;
        if code == 0 || at + code > input.len() {
            bail!("invalid COBS frame");
        }
        let data_end = (at + code).min(input.len() + 1);
        output.extend_from_slice(&input[at + 1..data_end]);
        at += code;
        if code != 0xff && at < input.len() {
            output.push(0);
        }
    }
    Ok(output)
}

#[derive(Default)]
pub struct CdcDecoder {
    buffer: Vec<u8>,
    overflowed: bool,
}
impl CdcDecoder {
    pub fn push(&mut self, byte: u8) -> Option<Result<Packet>> {
        if byte == 0 {
            if self.overflowed || self.buffer.is_empty() {
                self.buffer.clear();
                self.overflowed = false;
                return None;
            }
            let encoded = std::mem::take(&mut self.buffer);
            return Some(cobs_decode(&encoded).and_then(|raw| Packet::decode(&raw)));
        }
        if self.buffer.len() >= MAX_PACKET + 2 {
            self.overflowed = true;
        } else if !self.overflowed {
            self.buffer.push(byte);
        }
        None
    }
}

pub fn state_payload(modifiers: u8, bitmap: [u8; 32]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(33);
    payload.push(modifiers);
    payload.extend_from_slice(&bitmap);
    payload
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packet_fixture_roundtrip() {
        let packet = Packet::new(
            MessageType::State,
            0x0807_0605_0403_0201,
            0x1817_1615_1413_1211,
            42,
            state_payload(0x22, [0x5a; 32]),
        );
        let raw = packet.encode().unwrap();
        assert_eq!(raw.len(), 65);
        assert_eq!(Packet::decode(&raw).unwrap(), packet);
        let frame = packet.encode_cdc().unwrap();
        assert_eq!(*frame.last().unwrap(), 0);
        assert_eq!(
            Packet::decode(&cobs_decode(&frame[..frame.len() - 1]).unwrap()).unwrap(),
            packet
        );
    }
    #[test]
    fn invalid_crc_is_rejected() {
        let mut raw = Packet::new(MessageType::Hello, 0, 0, 0, vec![])
            .encode()
            .unwrap();
        raw[4] = 1;
        assert!(Packet::decode(&raw).is_err());
    }
    #[test]
    fn truncated_cobs_frame_is_rejected_without_panicking() {
        assert!(cobs_decode(&[2]).is_err());
    }
    #[test]
    fn canonical_hello_golden_vector() {
        let expected = hex("01010000000000000000000000000000000000000000000000000000b59149ef");
        assert_eq!(
            Packet::new(MessageType::Hello, 0, 0, 0, vec![])
                .encode()
                .unwrap(),
            expected
        );
    }
    #[test]
    fn canonical_state_golden_vector() {
        let expected = hex(
            "01050100080706050403020118171615141312110100000021000000021000000000000000000000000000000000000000000000000000000000000000e0059212",
        );
        let mut bitmap = [0u8; 32];
        bitmap[0] = 0x10;
        let packet = Packet {
            kind: MessageType::State,
            keypress_sample: true,
            session: 0x0102_0304_0506_0708,
            epoch: 0x1112_1314_1516_1718,
            sequence: 1,
            payload: state_payload(2, bitmap),
        };
        assert_eq!(packet.encode().unwrap(), expected);
    }
    fn hex(input: &str) -> Vec<u8> {
        (0..input.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&input[i..i + 2], 16).unwrap())
            .collect()
    }
    #[test]
    fn decoder_resynchronizes_after_oversize() {
        let mut decoder = CdcDecoder::default();
        for _ in 0..200 {
            assert!(decoder.push(1).is_none());
        }
        assert!(decoder.push(0).is_none());
        let valid = Packet::new(MessageType::Hello, 0, 0, 0, vec![])
            .encode_cdc()
            .unwrap();
        let mut result = None;
        for byte in valid {
            if let Some(value) = decoder.push(byte) {
                result = Some(value);
            }
        }
        assert_eq!(result.unwrap().unwrap().kind, MessageType::Hello);
    }
}

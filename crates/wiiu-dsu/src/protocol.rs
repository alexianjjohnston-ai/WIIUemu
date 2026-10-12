//! Cemuhook "DSU" protocol v1001 wire format.
//!
//! Layouts mirror Cemu's client structs in `src/input/api/DSU/DSUMessages.h` (packed, little endian):
//!
//! | message          | size | Cemu struct        |
//! |------------------|------|--------------------|
//! | header           | 16   | `MessageHeader`    |
//! | version response | 24   | `VersionResponse`  |
//! | port info        | 32   | `PortInfo`         |
//! | data response    | 100  | `DataResponse`     |
//!
//! Cemu validates server packets by recomputing the CRC32 over exactly these sizes, so the lengths are fixed.

use crate::pad::{Motion, PadData, TouchPoint};
use thiserror::Error;

pub const PROTOCOL_VERSION: u16 = 1001;
pub const MAGIC_CLIENT: [u8; 4] = *b"DSUC";
pub const MAGIC_SERVER: [u8; 4] = *b"DSUS";

pub const HEADER_LEN: usize = 16;
/// Header plus the 4-byte message type.
pub const MESSAGE_PREFIX_LEN: usize = 20;
pub const VERSION_RESPONSE_LEN: usize = 24;
pub const PORT_INFO_LEN: usize = 32;
pub const DATA_RESPONSE_LEN: usize = 100;

/// Cemu's `DSUControllerProvider::kMaxClients`.
pub const MAX_SLOTS: usize = 4;

pub const MSG_VERSION: u32 = 0x0010_0000;
pub const MSG_PORT_INFO: u32 = 0x0010_0001;
pub const MSG_PAD_DATA: u32 = 0x0010_0002;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SlotState {
    Disconnected = 0,
    Reserved = 1,
    Connected = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Model {
    None = 0,
    Ds3 = 1,
    /// "Full gyro" – tells clients that motion data is present.
    Ds4 = 2,
    Generic = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Connection {
    None = 0,
    Usb = 1,
    Bluetooth = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Battery {
    None = 0x00,
    Dying = 0x01,
    Low = 0x02,
    Medium = 0x03,
    High = 0x04,
    Full = 0x05,
    Charging = 0xEE,
    Charged = 0xEF,
}

/// Static description of a slot, sent in port-info and data packets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotInfo {
    pub slot: u8,
    pub state: SlotState,
    pub model: Model,
    pub connection: Connection,
    pub mac: [u8; 6],
    pub battery: Battery,
}

impl SlotInfo {
    /// A connected, motion-capable virtual controller with a locally administered MAC derived from the slot.
    pub fn virtual_connected(slot: u8) -> Self {
        Self {
            slot,
            state: SlotState::Connected,
            model: Model::Ds4,
            connection: Connection::Bluetooth,
            // 0x02 = locally administered unicast; "WU" marker; slot in the last byte.
            mac: [0x02, 0x57, 0x55, 0x43, 0x00, slot],
            battery: Battery::Full,
        }
    }
}

/// A request sent by a DSU client (e.g. Cemu).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientRequest {
    Version,
    /// Port info for the given slot indices (at most 4).
    PortInfo {
        slots: Vec<u8>,
    },
    /// Subscribe to pad data.
    PadData(PadDataRequest),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadDataRequest {
    AllSlots,
    Slot(u8),
    Mac([u8; 6]),
    SlotAndMac(u8, [u8; 6]),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientPacket {
    pub client_id: u32,
    pub request: ClientRequest,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("packet too short ({0} bytes)")]
    TooShort(usize),
    #[error("bad magic")]
    BadMagic,
    #[error("unsupported protocol version {0}")]
    UnsupportedVersion(u16),
    #[error("declared length {declared} does not match datagram ({actual} bytes)")]
    LengthMismatch { declared: usize, actual: usize },
    #[error("crc mismatch")]
    BadCrc,
    #[error("unknown message type {0:#x}")]
    UnknownType(u32),
    #[error("malformed payload")]
    Malformed,
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn i32_at(b: &[u8], o: usize) -> i32 {
    u32_at(b, o) as i32
}
fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_bits(u32_at(b, o))
}

/// CRC32 (zlib polynomial) of `packet` with the CRC field (bytes 8..12) treated as zero.
pub fn packet_crc(packet: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(&packet[..8]);
    h.update(&[0u8; 4]);
    h.update(&packet[12..]);
    h.finalize()
}

/// Validates the common header and returns `(message_type, id, packet)` where `packet` is trimmed to the declared length.
fn parse_header<'a>(buf: &'a [u8], magic: &[u8; 4]) -> Result<(u32, u32, &'a [u8]), ParseError> {
    if buf.len() < MESSAGE_PREFIX_LEN {
        return Err(ParseError::TooShort(buf.len()));
    }
    if &buf[..4] != magic {
        return Err(ParseError::BadMagic);
    }
    let version = u16_at(buf, 4);
    if version != PROTOCOL_VERSION {
        return Err(ParseError::UnsupportedVersion(version));
    }
    let declared = HEADER_LEN + u16_at(buf, 6) as usize;
    if declared < MESSAGE_PREFIX_LEN || declared > buf.len() {
        return Err(ParseError::LengthMismatch {
            declared,
            actual: buf.len(),
        });
    }
    let packet = &buf[..declared];
    if u32_at(packet, 8) != packet_crc(packet) {
        return Err(ParseError::BadCrc);
    }
    Ok((u32_at(packet, 16), u32_at(packet, 12), packet))
}

/// Parses a client → server datagram.
pub fn parse_client_packet(buf: &[u8]) -> Result<ClientPacket, ParseError> {
    let (msg_type, client_id, p) = parse_header(buf, &MAGIC_CLIENT)?;
    let payload = &p[MESSAGE_PREFIX_LEN..];
    let request = match msg_type {
        MSG_VERSION => ClientRequest::Version,
        MSG_PORT_INFO => {
            if payload.len() < 4 {
                return Err(ParseError::Malformed);
            }
            let count = i32_at(payload, 0);
            if !(0..=MAX_SLOTS as i32).contains(&count) || payload.len() < 4 + count as usize {
                return Err(ParseError::Malformed);
            }
            ClientRequest::PortInfo {
                slots: payload[4..4 + count as usize].to_vec(),
            }
        }
        MSG_PAD_DATA => {
            if payload.len() < 8 {
                return Err(ParseError::Malformed);
            }
            let flags = payload[0];
            let slot = payload[1];
            let mut mac = [0u8; 6];
            mac.copy_from_slice(&payload[2..8]);
            ClientRequest::PadData(match flags & 0x03 {
                0 => PadDataRequest::AllSlots,
                1 => PadDataRequest::Slot(slot),
                2 => PadDataRequest::Mac(mac),
                _ => PadDataRequest::SlotAndMac(slot, mac),
            })
        }
        other => return Err(ParseError::UnknownType(other)),
    };
    Ok(ClientPacket { client_id, request })
}

fn write_prefix(buf: &mut [u8], magic: &[u8; 4], id: u32, msg_type: u32) {
    buf[..4].copy_from_slice(magic);
    buf[4..6].copy_from_slice(&PROTOCOL_VERSION.to_le_bytes());
    let payload_len = (buf.len() - HEADER_LEN) as u16;
    buf[6..8].copy_from_slice(&payload_len.to_le_bytes());
    buf[8..12].fill(0);
    buf[12..16].copy_from_slice(&id.to_le_bytes());
    buf[16..20].copy_from_slice(&msg_type.to_le_bytes());
}

fn finalize(buf: &mut [u8]) {
    let crc = packet_crc(buf);
    buf[8..12].copy_from_slice(&crc.to_le_bytes());
}

fn write_slot_info(buf: &mut [u8], info: &SlotInfo) {
    buf[20] = info.slot;
    buf[21] = info.state as u8;
    buf[22] = info.model as u8;
    buf[23] = info.connection as u8;
    buf[24..30].copy_from_slice(&info.mac);
    buf[30] = info.battery as u8;
}

pub fn encode_version_response(server_id: u32) -> [u8; VERSION_RESPONSE_LEN] {
    let mut b = [0u8; VERSION_RESPONSE_LEN];
    write_prefix(&mut b, &MAGIC_SERVER, server_id, MSG_VERSION);
    b[20..22].copy_from_slice(&PROTOCOL_VERSION.to_le_bytes());
    finalize(&mut b);
    b
}

pub fn encode_port_info(server_id: u32, info: &SlotInfo) -> [u8; PORT_INFO_LEN] {
    let mut b = [0u8; PORT_INFO_LEN];
    write_prefix(&mut b, &MAGIC_SERVER, server_id, MSG_PORT_INFO);
    write_slot_info(&mut b, info);
    b[31] = 0; // trailing zero byte (Cemu: `is_active`, unused for port info)
    finalize(&mut b);
    b
}

fn write_touch(b: &mut [u8], o: usize, t: &Option<TouchPoint>) {
    if let Some(t) = t {
        b[o] = 1;
        b[o + 1] = t.id;
        b[o + 2..o + 4].copy_from_slice(&t.x.to_le_bytes());
        b[o + 4..o + 6].copy_from_slice(&t.y.to_le_bytes());
    }
}

pub fn encode_pad_data(server_id: u32, info: &SlotInfo, packet_number: u32, pad: &PadData) -> [u8; DATA_RESPONSE_LEN] {
    use crate::pad::buttons::*;
    let mut b = [0u8; DATA_RESPONSE_LEN];
    write_prefix(&mut b, &MAGIC_SERVER, server_id, MSG_PAD_DATA);
    write_slot_info(&mut b, info);
    b[31] = u8::from(info.state == SlotState::Connected);
    b[32..36].copy_from_slice(&packet_number.to_le_bytes());

    let [state1, state2] = pad.buttons.to_le_bytes();
    b[36] = state1;
    b[37] = state2;
    b[38] = u8::from(pad.home);
    b[39] = u8::from(pad.touch_click);
    b[40] = pad.left_stick[0];
    b[41] = pad.left_stick[1];
    b[42] = pad.right_stick[0];
    b[43] = pad.right_stick[1];

    let analog = |mask: u16| if pad.buttons & mask != 0 { 0xFF } else { 0x00 };
    b[44] = analog(LEFT);
    b[45] = analog(DOWN);
    b[46] = analog(RIGHT);
    b[47] = analog(UP);
    b[48] = analog(SQUARE);
    b[49] = analog(CROSS);
    b[50] = analog(CIRCLE);
    b[51] = analog(TRIANGLE);
    b[52] = analog(R1);
    b[53] = analog(L1);
    b[54] = analog(R2);
    b[55] = analog(L2);

    write_touch(&mut b, 56, &pad.touch[0]);
    write_touch(&mut b, 62, &pad.touch[1]);

    b[68..76].copy_from_slice(&pad.motion.timestamp_us.to_le_bytes());
    for (i, v) in pad.motion.accel.iter().chain(pad.motion.gyro.iter()).enumerate() {
        let o = 76 + i * 4;
        b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    finalize(&mut b);
    b
}

/// A server → client message, decoded the way Cemu's DSU client reads it. Used by the probe client and tests.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerMessage {
    Version {
        server_id: u32,
        version: u16,
    },
    PortInfo {
        server_id: u32,
        info: SlotInfo,
    },
    PadData {
        server_id: u32,
        info: SlotInfo,
        connected: bool,
        packet_number: u32,
        pad: PadData,
    },
}

fn decode_slot_info(b: &[u8]) -> Result<SlotInfo, ParseError> {
    let state = match b[21] {
        0 => SlotState::Disconnected,
        1 => SlotState::Reserved,
        2 => SlotState::Connected,
        _ => return Err(ParseError::Malformed),
    };
    let model = match b[22] {
        0 => Model::None,
        1 => Model::Ds3,
        2 => Model::Ds4,
        3 => Model::Generic,
        _ => return Err(ParseError::Malformed),
    };
    let connection = match b[23] {
        0 => Connection::None,
        1 => Connection::Usb,
        2 => Connection::Bluetooth,
        _ => return Err(ParseError::Malformed),
    };
    let battery = match b[30] {
        0x00 => Battery::None,
        0x01 => Battery::Dying,
        0x02 => Battery::Low,
        0x03 => Battery::Medium,
        0x04 => Battery::High,
        0x05 => Battery::Full,
        0xEE => Battery::Charging,
        0xEF => Battery::Charged,
        _ => return Err(ParseError::Malformed),
    };
    let mut mac = [0u8; 6];
    mac.copy_from_slice(&b[24..30]);
    Ok(SlotInfo {
        slot: b[20],
        state,
        model,
        connection,
        mac,
        battery,
    })
}

fn decode_touch(b: &[u8], o: usize) -> Option<TouchPoint> {
    (b[o] != 0).then(|| TouchPoint {
        id: b[o + 1],
        x: u16_at(b, o + 2),
        y: u16_at(b, o + 4),
    })
}

/// Parses a server → client datagram. Like Cemu, requires the exact struct size for CRC validation.
pub fn parse_server_packet(buf: &[u8]) -> Result<ServerMessage, ParseError> {
    let (msg_type, server_id, p) = parse_header(buf, &MAGIC_SERVER)?;
    let need = match msg_type {
        MSG_VERSION => VERSION_RESPONSE_LEN,
        MSG_PORT_INFO => PORT_INFO_LEN,
        MSG_PAD_DATA => DATA_RESPONSE_LEN,
        other => return Err(ParseError::UnknownType(other)),
    };
    if p.len() != need {
        return Err(ParseError::LengthMismatch {
            declared: p.len(),
            actual: need,
        });
    }
    Ok(match msg_type {
        MSG_VERSION => ServerMessage::Version {
            server_id,
            version: u16_at(p, 20),
        },
        MSG_PORT_INFO => ServerMessage::PortInfo {
            server_id,
            info: decode_slot_info(p)?,
        },
        _ => {
            let mut accel = [0f32; 3];
            let mut gyro = [0f32; 3];
            for i in 0..3 {
                accel[i] = f32_at(p, 76 + i * 4);
                gyro[i] = f32_at(p, 88 + i * 4);
            }
            ServerMessage::PadData {
                server_id,
                info: decode_slot_info(p)?,
                connected: p[31] != 0,
                packet_number: u32_at(p, 32),
                pad: PadData {
                    buttons: u16::from_le_bytes([p[36], p[37]]),
                    home: p[38] != 0,
                    touch_click: p[39] != 0,
                    left_stick: [p[40], p[41]],
                    right_stick: [p[42], p[43]],
                    touch: [decode_touch(p, 56), decode_touch(p, 62)],
                    motion: Motion {
                        timestamp_us: u64::from_le_bytes(p[68..76].try_into().map_err(|_| ParseError::Malformed)?),
                        accel,
                        gyro,
                    },
                },
            }
        }
    })
}

/// Builders for client requests, byte-identical to what Cemu's `DSUMessages.cpp` sends.
pub mod client {
    use super::*;

    fn finish(mut b: Vec<u8>) -> Vec<u8> {
        finalize(&mut b);
        b
    }

    pub fn version_request(client_id: u32) -> Vec<u8> {
        let mut b = vec![0u8; MESSAGE_PREFIX_LEN];
        write_prefix(&mut b, &MAGIC_CLIENT, client_id, MSG_VERSION);
        finish(b)
    }

    /// Cemu always sends 4 index bytes regardless of `count`.
    pub fn port_info_request(client_id: u32, slots: &[u8]) -> Vec<u8> {
        assert!(slots.len() <= MAX_SLOTS);
        let mut b = vec![0u8; MESSAGE_PREFIX_LEN + 8];
        write_prefix(&mut b, &MAGIC_CLIENT, client_id, MSG_PORT_INFO);
        b[20..24].copy_from_slice(&(slots.len() as i32).to_le_bytes());
        b[24..24 + slots.len()].copy_from_slice(slots);
        finish(b)
    }

    pub fn pad_data_request(client_id: u32, req: PadDataRequest) -> Vec<u8> {
        let mut b = vec![0u8; MESSAGE_PREFIX_LEN + 8];
        write_prefix(&mut b, &MAGIC_CLIENT, client_id, MSG_PAD_DATA);
        let (flags, slot, mac) = match req {
            PadDataRequest::AllSlots => (0, 0, [0; 6]),
            PadDataRequest::Slot(s) => (1, s, [0; 6]),
            PadDataRequest::Mac(m) => (2, 0, m),
            PadDataRequest::SlotAndMac(s, m) => (3, s, m),
        };
        b[20] = flags;
        b[21] = slot;
        b[22..28].copy_from_slice(&mac);
        finish(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::buttons;

    #[test]
    fn crc_is_zlib_crc32_with_crc_field_zeroed() {
        // Cemu uses the zlib polynomial; check value for "123456789" is 0xCBF43926.
        let mut h = crc32fast::Hasher::new();
        h.update(b"123456789");
        assert_eq!(h.finalize(), 0xCBF4_3926);

        let mut b = client::version_request(3);
        let crc = packet_crc(&b);
        b[8..12].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
        assert_eq!(packet_crc(&b), crc, "the stored CRC must not affect the computed CRC");
    }

    #[test]
    fn client_requests_have_cemu_sizes() {
        // sizeof(VersionRequest) == 20, sizeof(ListPorts) == 28, sizeof(DataRequest) == 28
        assert_eq!(client::version_request(1).len(), 20);
        assert_eq!(client::port_info_request(1, &[0, 1, 2, 3]).len(), 28);
        assert_eq!(client::pad_data_request(1, PadDataRequest::Slot(2)).len(), 28);
    }

    #[test]
    fn version_request_bytes_are_exact() {
        let b = client::version_request(0x1122_3344);
        assert_eq!(&b[0..4], b"DSUC");
        assert_eq!(u16_at(&b, 4), 1001);
        assert_eq!(u16_at(&b, 6), 4); // payload size excludes the 16-byte header
        assert_eq!(u32_at(&b, 12), 0x1122_3344);
        assert_eq!(u32_at(&b, 16), MSG_VERSION);
        assert_eq!(u32_at(&b, 8), packet_crc(&b));
    }

    #[test]
    fn parses_all_client_requests() {
        let p = parse_client_packet(&client::version_request(7)).unwrap();
        assert_eq!(p.client_id, 7);
        assert_eq!(p.request, ClientRequest::Version);

        let p = parse_client_packet(&client::port_info_request(7, &[0, 3])).unwrap();
        assert_eq!(p.request, ClientRequest::PortInfo { slots: vec![0, 3] });

        for req in [
            PadDataRequest::AllSlots,
            PadDataRequest::Slot(1),
            PadDataRequest::Mac([1, 2, 3, 4, 5, 6]),
            PadDataRequest::SlotAndMac(2, [6, 5, 4, 3, 2, 1]),
        ] {
            let p = parse_client_packet(&client::pad_data_request(9, req)).unwrap();
            assert_eq!(p.request, ClientRequest::PadData(req));
        }
    }

    #[test]
    fn rejects_corrupt_client_packets() {
        let good = client::pad_data_request(1, PadDataRequest::Slot(0));

        assert_eq!(parse_client_packet(&good[..10]), Err(ParseError::TooShort(10)));

        let mut bad = good.clone();
        bad[0] = b'X';
        assert_eq!(parse_client_packet(&bad), Err(ParseError::BadMagic));

        let mut bad = good.clone();
        bad[4..6].copy_from_slice(&1000u16.to_le_bytes());
        assert_eq!(parse_client_packet(&bad), Err(ParseError::UnsupportedVersion(1000)));

        let mut bad = good.clone();
        bad[25] ^= 0xFF; // payload flip -> crc mismatch
        assert_eq!(parse_client_packet(&bad), Err(ParseError::BadCrc));

        let mut bad = good.clone();
        bad[6..8].copy_from_slice(&500u16.to_le_bytes());
        assert!(matches!(
            parse_client_packet(&bad),
            Err(ParseError::LengthMismatch { .. })
        ));

        // Server packets are not client packets.
        assert_eq!(
            parse_client_packet(&encode_version_response(1)),
            Err(ParseError::BadMagic)
        );
    }

    #[test]
    fn rejects_port_info_with_bad_count() {
        let mut b = client::port_info_request(1, &[0]);
        b[20..24].copy_from_slice(&5i32.to_le_bytes());
        finalize(&mut b);
        assert_eq!(parse_client_packet(&b), Err(ParseError::Malformed));
        b[20..24].copy_from_slice(&(-1i32).to_le_bytes());
        finalize(&mut b);
        assert_eq!(parse_client_packet(&b), Err(ParseError::Malformed));
    }

    #[test]
    fn server_packet_sizes_match_cemu_structs() {
        let info = SlotInfo::virtual_connected(1);
        assert_eq!(encode_version_response(1).len(), 0x18);
        assert_eq!(encode_port_info(1, &info).len(), 0x20);
        assert_eq!(encode_pad_data(1, &info, 0, &PadData::neutral()).len(), 100);
    }

    #[test]
    fn pad_data_field_offsets_match_cemu_layout() {
        let info = SlotInfo::virtual_connected(2);
        let pad = PadData {
            buttons: buttons::CROSS | buttons::OPTIONS | buttons::LEFT,
            home: false,
            touch_click: false,
            left_stick: [10, 250],
            right_stick: [128, 128],
            touch: [Some(TouchPoint { id: 1, x: 960, y: 471 }), None],
            motion: Motion {
                timestamp_us: 0x0102_0304_0506_0708,
                accel: [0.0, -1.0, 0.5],
                gyro: [10.0, 20.0, -30.0],
            },
        };
        let b = encode_pad_data(0xAABB_CCDD, &info, 42, &pad);
        assert_eq!(&b[0..4], b"DSUS");
        assert_eq!(u16_at(&b, 6) as usize, 100 - 16);
        assert_eq!(u32_at(&b, 12), 0xAABB_CCDD);
        assert_eq!(u32_at(&b, 16), MSG_PAD_DATA);
        assert_eq!(b[20], 2); // slot
        assert_eq!(b[21], SlotState::Connected as u8);
        assert_eq!(b[31], 1); // connected
        assert_eq!(u32_at(&b, 32), 42); // m_packet_index
        assert_eq!(b[36], 0b1000_1000); // state1: Left(bit7) | Options(bit3)
        assert_eq!(b[37], 0b0100_0000); // state2: Cross(bit6)
        assert_eq!((b[40], b[41]), (10, 250));
        assert_eq!(b[44], 0xFF); // analog dpad left
        assert_eq!(b[49], 0xFF); // analog cross
        assert_eq!(b[50], 0x00); // analog circle
        assert_eq!(&b[56..62], &[1, 1, 0xC0, 0x03, 0xD7, 0x01]); // tpad1: active, id, x=960, y=471
        assert_eq!(&b[62..68], &[0; 6]); // tpad2 inactive
        assert_eq!(&b[68..76], &0x0102_0304_0506_0708u64.to_le_bytes());
        assert_eq!(f32_at(&b, 80), -1.0); // accel.y
        assert_eq!(f32_at(&b, 96), -30.0); // gyro.z
        assert_eq!(u32_at(&b, 8), packet_crc(&b));
    }

    #[test]
    fn server_packets_round_trip() {
        let info = SlotInfo::virtual_connected(3);
        assert_eq!(
            parse_server_packet(&encode_version_response(5)).unwrap(),
            ServerMessage::Version {
                server_id: 5,
                version: 1001
            }
        );
        assert_eq!(
            parse_server_packet(&encode_port_info(5, &info)).unwrap(),
            ServerMessage::PortInfo { server_id: 5, info }
        );
        let mut pad = PadData::neutral();
        pad.buttons = buttons::CIRCLE | buttons::L3;
        pad.touch[0] = Some(TouchPoint { id: 0, x: 1919, y: 941 });
        match parse_server_packet(&encode_pad_data(5, &info, 9, &pad)).unwrap() {
            ServerMessage::PadData {
                packet_number,
                pad: decoded,
                connected,
                ..
            } => {
                assert_eq!(packet_number, 9);
                assert!(connected);
                assert_eq!(decoded, pad);
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

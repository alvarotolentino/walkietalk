use crate::error::AppError;

/// Binary audio frame header size in bytes (spec §6.6).
pub const HEADER_SIZE: usize = 19;

/// Flag indicating the final frame of a transmission.
pub const FLAG_END_OF_TRANSMISSION: u8 = 0x01;

/// Binary audio frame (spec §6.6).
///
/// On the wire, fields are big-endian:
/// ```text
/// Offset  Size  Field
/// 0       8     room_id        u64
/// 8       4     speaker_id     u32
/// 12      4     sequence_num   u32
/// 16      1     flags          u8
/// 17      2     payload_len    u16
/// 19      N     opus_payload
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioFrame {
    pub room_id: u64,
    pub speaker_id: u32,
    pub sequence_num: u32,
    pub flags: u8,
    pub payload: Vec<u8>,
}

impl AudioFrame {
    /// Encode this frame into a byte buffer (big-endian wire format).
    pub fn encode(&self) -> Vec<u8> {
        let payload_len = self.payload.len() as u16;
        let mut buf = Vec::with_capacity(HEADER_SIZE + self.payload.len());
        buf.extend_from_slice(&self.room_id.to_be_bytes());
        buf.extend_from_slice(&self.speaker_id.to_be_bytes());
        buf.extend_from_slice(&self.sequence_num.to_be_bytes());
        buf.push(self.flags);
        buf.extend_from_slice(&payload_len.to_be_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Decode a complete audio frame from raw bytes.
    pub fn decode(data: &[u8]) -> Result<Self, AppError> {
        if data.len() < HEADER_SIZE {
            return Err(AppError::BadRequest(format!(
                "audio frame too short: {} bytes, minimum {}",
                data.len(),
                HEADER_SIZE,
            )));
        }

        let room_id = u64::from_be_bytes(data[0..8].try_into().unwrap());
        let speaker_id = u32::from_be_bytes(data[8..12].try_into().unwrap());
        let sequence_num = u32::from_be_bytes(data[12..16].try_into().unwrap());
        let flags = data[16];
        let payload_len = u16::from_be_bytes(data[17..19].try_into().unwrap()) as usize;

        if data.len() < HEADER_SIZE + payload_len {
            return Err(AppError::BadRequest(format!(
                "audio frame truncated: payload_len={payload_len} but only {} bytes remain",
                data.len() - HEADER_SIZE,
            )));
        }

        let payload = data[HEADER_SIZE..HEADER_SIZE + payload_len].to_vec();

        Ok(Self {
            room_id,
            speaker_id,
            sequence_num,
            flags,
            payload,
        })
    }

    /// Decode only the fixed header (17 bytes) without allocating the payload.
    ///
    /// Returns `(room_id, speaker_id, sequence_num, flags)`.
    pub fn decode_header(data: &[u8]) -> Result<(u64, u32, u32, u8), AppError> {
        // We only need the first 17 bytes (room_id + speaker_id + sequence_num + flags).
        const MIN_HEADER: usize = 17;
        if data.len() < MIN_HEADER {
            return Err(AppError::BadRequest(format!(
                "audio frame header too short: {} bytes, minimum {MIN_HEADER}",
                data.len(),
            )));
        }

        let room_id = u64::from_be_bytes(data[0..8].try_into().unwrap());
        let speaker_id = u32::from_be_bytes(data[8..12].try_into().unwrap());
        let sequence_num = u32::from_be_bytes(data[12..16].try_into().unwrap());
        let flags = data[16];

        Ok((room_id, speaker_id, sequence_num, flags))
    }

    /// Returns true if this is the final frame of a transmission.
    pub fn is_end_of_transmission(&self) -> bool {
        self.flags & FLAG_END_OF_TRANSMISSION != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_frame() -> AudioFrame {
        AudioFrame {
            room_id: 0x0001_0002_0003_0004,
            speaker_id: 42,
            sequence_num: 7,
            flags: 0,
            payload: vec![0xDE, 0xAD, 0xBE, 0xEF],
        }
    }

    #[test]
    fn encode_decode_roundtrip() {
        let frame = sample_frame();
        let encoded = frame.encode();
        let decoded = AudioFrame::decode(&encoded).unwrap();
        assert_eq!(frame, decoded);
    }

    #[test]
    fn decode_insufficient_bytes_errors() {
        let short = vec![0u8; HEADER_SIZE - 1];
        assert!(AudioFrame::decode(&short).is_err());
    }

    #[test]
    fn decode_truncated_payload_errors() {
        let mut frame = sample_frame();
        frame.payload = vec![0xAA; 100];
        let mut encoded = frame.encode();
        // Truncate payload to 50 bytes (header says 100)
        encoded.truncate(HEADER_SIZE + 50);
        assert!(AudioFrame::decode(&encoded).is_err());
    }

    #[test]
    fn decode_header_only() {
        let frame = sample_frame();
        let encoded = frame.encode();
        let (room_id, speaker_id, seq, flags) = AudioFrame::decode_header(&encoded).unwrap();
        assert_eq!(room_id, frame.room_id);
        assert_eq!(speaker_id, frame.speaker_id);
        assert_eq!(seq, frame.sequence_num);
        assert_eq!(flags, frame.flags);
    }

    #[test]
    fn decode_header_too_short() {
        let short = vec![0u8; 10];
        assert!(AudioFrame::decode_header(&short).is_err());
    }

    #[test]
    fn end_of_transmission_flag() {
        let mut frame = sample_frame();
        assert!(!frame.is_end_of_transmission());
        frame.flags = FLAG_END_OF_TRANSMISSION;
        assert!(frame.is_end_of_transmission());
        // flag combined with other bits
        frame.flags = FLAG_END_OF_TRANSMISSION | 0x80;
        assert!(frame.is_end_of_transmission());
    }

    #[test]
    fn byte_layout_matches_spec() {
        let frame = AudioFrame {
            room_id: 1,
            speaker_id: 2,
            sequence_num: 3,
            flags: FLAG_END_OF_TRANSMISSION,
            payload: vec![0xFF, 0x00],
        };
        let bytes = frame.encode();

        // room_id: u64 BE = 1
        assert_eq!(&bytes[0..8], &[0, 0, 0, 0, 0, 0, 0, 1]);
        // speaker_id: u32 BE = 2
        assert_eq!(&bytes[8..12], &[0, 0, 0, 2]);
        // sequence_num: u32 BE = 3
        assert_eq!(&bytes[12..16], &[0, 0, 0, 3]);
        // flags = 0x01
        assert_eq!(bytes[16], 0x01);
        // payload_len: u16 BE = 2
        assert_eq!(&bytes[17..19], &[0, 2]);
        // payload
        assert_eq!(&bytes[19..], &[0xFF, 0x00]);
        // total length
        assert_eq!(bytes.len(), HEADER_SIZE + 2);
    }

    #[test]
    fn empty_payload_roundtrip() {
        let frame = AudioFrame {
            room_id: 99,
            speaker_id: 1,
            sequence_num: 0,
            flags: 0,
            payload: vec![],
        };
        let encoded = frame.encode();
        assert_eq!(encoded.len(), HEADER_SIZE);
        let decoded = AudioFrame::decode(&encoded).unwrap();
        assert_eq!(frame, decoded);
    }
}

// Copyright 2026 nosqlbench contributors
// SPDX-License-Identifier: Apache-2.0

//! Page footer serialization and deserialization.
//!
//! Every slabtastic page ends with a fixed-size footer that carries all
//! metadata needed to interpret the page without external context.

use crate::constants::{FOOTER_V1_SIZE, PageType, VERSION_1};
use crate::error::{Result, SlabError};

/// A 16-byte page footer (v1 layout, little-endian).
///
/// ## Wire format
///
/// ```text
/// Byte   Field            Width   Encoding
/// 0–4    start_ordinal    5       signed LE (±2^39 range)
/// 5–7    record_count     3       unsigned LE (max 2^24 − 1 = 16,777,215)
/// 8–11   page_size        4       unsigned LE (512 .. 2^32)
/// 12     page_type        1       enum (0=Invalid, 1=Pages, 2=Data)
/// 13     version          1       must be 1 for v1
/// 14–15  footer_length    2       unsigned LE (≥ 16, multiple of 16)
/// ```
///
/// ## Examples
///
/// ```
/// use slabtastic::Footer;
/// use slabtastic::PageType;
/// use slabtastic::constants::FOOTER_V1_SIZE;
///
/// let footer = Footer::new(42, 10, 4096, PageType::Data);
/// let mut buf = [0u8; FOOTER_V1_SIZE];
/// footer.write_to(&mut buf);
/// let decoded = Footer::read_from(&buf).unwrap();
/// assert_eq!(footer, decoded);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footer {
    /// Starting ordinal for this page (5-byte signed, range ±2^39).
    pub start_ordinal: i64,
    /// Number of records in this page (3-byte unsigned, max 2^24-1).
    pub record_count: u32,
    /// Total page size in bytes.
    pub page_size: u32,
    /// Discriminator for the page type.
    pub page_type: PageType,
    /// Format version (must be 1 for v1).
    pub version: u8,
    /// Length of the footer in bytes (>= 16, multiple of 16).
    pub footer_length: u16,
}

impl Footer {
    /// Create a new v1 footer with the given parameters.
    pub fn new(start_ordinal: i64, record_count: u32, page_size: u32, page_type: PageType) -> Self {
        Footer {
            start_ordinal,
            record_count,
            page_size,
            page_type,
            version: VERSION_1,
            footer_length: FOOTER_V1_SIZE as u16,
        }
    }

    /// Serialize this footer into exactly 16 bytes (little-endian).
    pub fn write_to(&self, buf: &mut [u8]) {
        assert!(
            buf.len() >= FOOTER_V1_SIZE,
            "buffer too small for footer: {} < {FOOTER_V1_SIZE}",
            buf.len()
        );

        // start_ordinal: 5 bytes LE (signed, mask to 5 bytes)
        let ord_bytes = self.start_ordinal.to_le_bytes();
        buf[0..5].copy_from_slice(&ord_bytes[0..5]);

        // record_count: 3 bytes LE
        let rc_bytes = self.record_count.to_le_bytes();
        buf[5..8].copy_from_slice(&rc_bytes[0..3]);

        // page_size: 4 bytes LE
        buf[8..12].copy_from_slice(&self.page_size.to_le_bytes());

        // page_type: 1 byte
        buf[12] = self.page_type as u8;

        // version: 1 byte
        buf[13] = self.version;

        // footer_length: 2 bytes LE
        buf[14..16].copy_from_slice(&self.footer_length.to_le_bytes());
    }

    /// Deserialize a footer from exactly 16 bytes (little-endian).
    pub fn read_from(buf: &[u8]) -> Result<Footer> {
        if buf.len() < FOOTER_V1_SIZE {
            return Err(SlabError::InvalidFooter(format!(
                "buffer too small: {} < {FOOTER_V1_SIZE}",
                buf.len()
            )));
        }

        // start_ordinal: 5 bytes LE, sign-extend to i64
        let mut ord_bytes = [0u8; 8];
        ord_bytes[0..5].copy_from_slice(&buf[0..5]);
        // sign-extend: if bit 39 is set, fill bytes 5..8 with 0xFF
        if ord_bytes[4] & 0x80 != 0 {
            ord_bytes[5] = 0xFF;
            ord_bytes[6] = 0xFF;
            ord_bytes[7] = 0xFF;
        }
        let start_ordinal = i64::from_le_bytes(ord_bytes);

        // record_count: 3 bytes LE
        let mut rc_bytes = [0u8; 4];
        rc_bytes[0..3].copy_from_slice(&buf[5..8]);
        let record_count = u32::from_le_bytes(rc_bytes);

        // page_size: 4 bytes LE
        let page_size = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);

        // page_type: 1 byte
        let page_type_raw = buf[12];
        let page_type =
            PageType::from_u8(page_type_raw).ok_or(SlabError::InvalidPageType(page_type_raw))?;
        if page_type == PageType::Invalid {
            return Err(SlabError::InvalidPageType(page_type_raw));
        }

        // version: 1 byte
        let version = buf[13];
        if version != VERSION_1 {
            return Err(SlabError::InvalidVersion(version));
        }

        // footer_length: 2 bytes LE
        let footer_length = u16::from_le_bytes([buf[14], buf[15]]);
        if footer_length < FOOTER_V1_SIZE as u16 {
            return Err(SlabError::InvalidFooter(format!(
                "footer_length {footer_length} < {FOOTER_V1_SIZE}"
            )));
        }

        Ok(Footer {
            start_ordinal,
            record_count,
            page_size,
            page_type,
            version,
            footer_length,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialize a footer with typical values and deserialize it back,
    /// confirming all fields survive the round-trip unchanged.
    #[test]
    fn test_footer_roundtrip() {
        let footer = Footer::new(42, 10, 4096, PageType::Data);
        let mut buf = [0u8; FOOTER_V1_SIZE];
        footer.write_to(&mut buf);
        let decoded = Footer::read_from(&buf).unwrap();
        assert_eq!(footer, decoded);
    }

    /// Verify that a negative ordinal (−1) round-trips correctly through
    /// the 5-byte sign-extended encoding.
    #[test]
    fn test_footer_negative_ordinal() {
        let footer = Footer::new(-1, 1, 512, PageType::Data);
        let mut buf = [0u8; FOOTER_V1_SIZE];
        footer.write_to(&mut buf);
        let decoded = Footer::read_from(&buf).unwrap();
        assert_eq!(decoded.start_ordinal, -1);
    }

    /// Verify the maximum positive 5-byte signed ordinal (2^39 − 1)
    /// round-trips without truncation or sign-extension errors.
    #[test]
    fn test_footer_large_ordinal() {
        // Max positive 5-byte signed: 2^39 - 1 = 549_755_813_887
        let max_ord: i64 = (1i64 << 39) - 1;
        let footer = Footer::new(max_ord, 100, 65536, PageType::Pages);
        let mut buf = [0u8; FOOTER_V1_SIZE];
        footer.write_to(&mut buf);
        let decoded = Footer::read_from(&buf).unwrap();
        assert_eq!(decoded.start_ordinal, max_ord);
    }

    /// Verify the maximum 3-byte unsigned record count (2^24 − 1 =
    /// 16,777,215) round-trips correctly.
    #[test]
    fn test_footer_max_record_count() {
        // Max 3-byte unsigned: 2^24 - 1 = 16_777_215
        let footer = Footer::new(0, 0x00FF_FFFF, 65536, PageType::Data);
        let mut buf = [0u8; FOOTER_V1_SIZE];
        footer.write_to(&mut buf);
        let decoded = Footer::read_from(&buf).unwrap();
        assert_eq!(decoded.record_count, 0x00FF_FFFF);
    }

    /// A footer with version 99 (not the recognized v1) must be rejected
    /// during deserialization with an `InvalidVersion` error.
    #[test]
    fn test_footer_invalid_version() {
        let footer = Footer {
            start_ordinal: 0,
            record_count: 0,
            page_size: 512,
            page_type: PageType::Data,
            version: 99,
            footer_length: 16,
        };
        let mut buf = [0u8; FOOTER_V1_SIZE];
        footer.write_to(&mut buf);
        let result = Footer::read_from(&buf);
        assert!(result.is_err());
    }

    /// Corrupting the page_type byte to 0 (Invalid) after serializing a
    /// valid footer must cause `read_from` to reject it.
    #[test]
    fn test_footer_invalid_page_type() {
        let mut buf = [0u8; FOOTER_V1_SIZE];
        // Write a valid footer then corrupt page_type
        let footer = Footer::new(0, 0, 512, PageType::Data);
        footer.write_to(&mut buf);
        buf[12] = 0; // Invalid
        let result = Footer::read_from(&buf);
        assert!(result.is_err());
    }
}

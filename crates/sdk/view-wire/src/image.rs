use serde::{Deserialize, Serialize};

/// Copied raster data or an opaque host image resource. Native allocations never cross.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImageData {
    Encoded(
        #[serde(
            serialize_with = "crate::codec::bin::serialize",
            deserialize_with = "decode_bytes"
        )]
        Vec<u8>,
    ),
    Rgba {
        width: u32,
        height: u32,
        #[serde(
            serialize_with = "crate::codec::bin::serialize",
            deserialize_with = "decode_bytes"
        )]
        pixels: Vec<u8>,
    },
    /// A host-issued image key, resolved at paint time rather than cached pixels.
    /// This is neither a filesystem path nor a URL.
    Resource(String),
    /// A visible refusal produced before a native resource can be accessed.
    Refusal(String),
}

impl ImageData {
    pub fn byte_len(&self) -> usize {
        match self {
            Self::Resource(key) | Self::Refusal(key) => key.len(),
            Self::Encoded(bytes) => bytes.len(),
            Self::Rgba { pixels, .. } => pixels.len(),
        }
    }

    pub fn valid_rgba(&self) -> bool {
        match self {
            Self::Resource(key) | Self::Refusal(key) => !key.is_empty() && !key.contains('\0'),
            Self::Encoded(_) => true,
            Self::Rgba {
                width,
                height,
                pixels,
            } => {
                *width != 0
                    && *height != 0
                    && u64::from(*width)
                        .checked_mul(u64::from(*height))
                        .and_then(|count| count.checked_mul(4))
                        == Some(pixels.len() as u64)
            }
        }
    }

    pub(crate) fn sanitize(data: &mut Option<Self>, budgets: &mut crate::Budgets) {
        if let Some(Self::Refusal(reason)) = data {
            super::spend_text(reason, budgets);
            return;
        }
        if let Some(value) = data {
            if !value.valid_rgba() {
                *data = None;
            } else if value.byte_len() > budgets.pictures {
                *data = None;
                budgets.cut(|cuts| &mut cuts.pictures, 1);
            } else {
                budgets.pictures -= value.byte_len();
            }
        }
    }
}

// Bounded by the frame, not the picture allowance: frame sanitization applies
// that, dropping a picture whole rather than truncating it. A longer `bin` is
// refused before allocating, even when ImageData is decoded on its own.
fn decode_bytes<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    crate::codec::bin::bounded(
        deserializer,
        crate::MAX_FRAME_BYTES,
        "raster byte limit exceeded",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_rgba_is_dropped_without_spending_valid_picture_budget() {
        let mut budget = crate::Budgets::frame(&crate::styles::testing::held());
        budget.pictures = 4;
        let mut invalid = Some(ImageData::Rgba {
            width: u32::MAX,
            height: u32::MAX,
            pixels: vec![0; 4],
        });
        ImageData::sanitize(&mut invalid, &mut budget);
        assert_eq!(invalid, None);
        assert_eq!(budget.pictures, 4);
        let mut valid = Some(ImageData::Rgba {
            width: 1,
            height: 1,
            pixels: vec![255; 4],
        });
        ImageData::sanitize(&mut valid, &mut budget);
        assert!(valid.is_some());
        assert_eq!(budget.pictures, 0);
        let mut excess = Some(ImageData::Encoded(vec![1]));
        ImageData::sanitize(&mut excess, &mut budget);
        assert_eq!(excess, None);
    }
}

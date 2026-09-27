//! Static metadata for a ducktape view. Parsing never instantiates or runs a guest.

use crate::methods::Capability;

pub const MANIFEST_SECTION: &str = "ducktape.view.manifest";

/// A finite positive logical size, bounded like wire geometry. Private bits
/// keep equality and hashing exact without admitting NaN or signed zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PreferredSize([u32; 2]);

impl PreferredSize {
    pub fn new(width: f32, height: f32) -> Option<Self> {
        [width, height]
            .iter()
            .all(|value| value.is_finite() && *value > 0.0 && *value <= crate::MAX_PIXELS)
            .then_some(Self([width.to_bits(), height.to_bits()]))
    }

    pub fn dimensions(self) -> [f32; 2] {
        [f32::from_bits(self.0[0]), f32::from_bits(self.0[1])]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub wire_epoch: u32,
    /// the [`crate::methods::METHODS_REVISION`] the view was built against
    pub methods: u32,
    pub name: String,
    pub description: String,
    pub capabilities: Vec<Capability>,
    pub preferred_size: Option<PreferredSize>,
}

/// What a manifest may say about itself. The catalog is read before anything
/// is installed, and the store shapes every field of every entry on every
/// relayout — outside the sandbox, with no fuel and no memory limit — so a
/// module whose manifest is a megabyte of capability names is left out of the
/// catalog rather than laid out.
const MAX_NAME_BYTES: usize = 64;
const MAX_DESCRIPTION_BYTES: usize = 256;
const MAX_CAPABILITIES: usize = 16;

/// Extracts exactly one current manifest from a view's core module.
/// Returns `None` for missing, duplicate, malformed, or out-of-bounds metadata.
/// This parses the binary structure; it does not validate the exports or execute it.
#[cfg(feature = "manifest")]
pub fn read_manifest(bytes: &[u8]) -> Option<Manifest> {
    let mut payloads = wasmparser::Parser::new(0).parse_all(bytes);
    // A view is a core module; a component is not something the host
    // instantiates, so it is not in the catalog.
    let Some(Ok(wasmparser::Payload::Version {
        encoding: wasmparser::Encoding::Module,
        ..
    })) = payloads.next()
    else {
        return None;
    };
    let mut manifest = None;
    for payload in payloads {
        if let wasmparser::Payload::CustomSection(section) = payload.ok()?
            && section.name() == MANIFEST_SECTION
        {
            if manifest.is_some() {
                return None;
            }
            manifest = Some(Manifest::parse(std::str::from_utf8(section.data()).ok()?)?);
        }
    }
    manifest
}

impl Manifest {
    /// Parses the strict seven-line `ducktape.view.manifest.v2` text and its
    /// bounds. A capability this host does not know refuses the whole
    /// manifest: a grant is never silently narrowed.
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() > 1024 || text.chars().any(|c| c.is_control() && c != '\n') {
            return None;
        }
        let mut lines = text.split('\n');
        if lines.next()? != "ducktape.view.manifest.v2" {
            return None;
        }
        let name = lines.next()?.to_owned();
        let description = lines.next()?.to_owned();
        let caps = lines.next()?;
        let capabilities = if caps.is_empty() {
            Vec::new()
        } else {
            caps.strip_suffix(',')?
                .split(',')
                .map(Capability::parse)
                .collect::<Option<_>>()?
        };
        let preferred_size = match lines.next()? {
            "none" => None,
            value => {
                let (width, height) = value.split_once(',')?;
                Some(PreferredSize::new(
                    width.parse().ok()?,
                    height.parse().ok()?,
                )?)
            }
        };
        let wire_epoch = canonical(lines.next()?)?;
        let methods = canonical(lines.next()?)?;
        if lines.next().is_some() {
            return None;
        }
        let manifest = Self {
            wire_epoch,
            methods,
            name,
            description,
            capabilities,
            preferred_size,
        };
        manifest.within_bounds().then_some(manifest)
    }

    fn within_bounds(&self) -> bool {
        !self.name.is_empty()
            && self.name.len() <= MAX_NAME_BYTES
            && self.description.len() <= MAX_DESCRIPTION_BYTES
            && self.capabilities.len() <= MAX_CAPABILITIES
    }
}

/// A positive number written the one way: no sign, no leading zero.
fn canonical(text: &str) -> Option<u32> {
    let number = text.parse::<u32>().ok()?;
    (number != 0 && number.to_string() == text).then_some(number)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifest_requires_an_explicit_canonical_wire_epoch() {
        let current = "ducktape.view.manifest.v2\nSized\nDescription\nclock,\nnone\n1\n1";
        assert!(
            Manifest::parse(current).is_some(),
            "current epoch manifest rejected"
        );
        assert!(
            Manifest::parse("ducktape.view.manifest\nSized\nDescription\nclock,\nnone").is_none(),
            "a header that is not ours, short one line, must not parse"
        );
        for epoch in ["", "0", "01", "+1", "-1", " 1", "1 ", "4294967296"] {
            assert!(
                Manifest::parse(&format!(
                    "ducktape.view.manifest.v2\nSized\n\n\nnone\n{epoch}\n1"
                ))
                .is_none(),
                "accepted {epoch:?}"
            );
        }
        assert!(
            Manifest::parse("ducktape.view.manifest.v2\nSized\n\n\nnone\n2\n1").is_some(),
            "unsupported is distinct from malformed"
        );
    }

    #[cfg(feature = "manifest")]
    #[test]
    fn extraction_rejects_duplicate_and_truncated_sections() {
        let mut bytes = b"\0asm\x01\0\0\0".to_vec();
        let text = b"ducktape.view.manifest.v2\nSized\n\n\n640.5,480.25\n1\n1";
        let mut section = vec![
            0,
            (1 + MANIFEST_SECTION.len() + text.len()) as u8,
            MANIFEST_SECTION.len() as u8,
        ];
        section.extend_from_slice(MANIFEST_SECTION.as_bytes());
        section.extend_from_slice(text);
        bytes.extend_from_slice(&section);
        assert_eq!(
            read_manifest(&bytes)
                .unwrap()
                .preferred_size
                .unwrap()
                .dimensions(),
            [640.5, 480.25]
        );
        let mut duplicate = bytes.clone();
        duplicate.extend_from_slice(&section);
        assert!(
            read_manifest(&duplicate).is_none(),
            "duplicate manifest accepted"
        );
        bytes.extend_from_slice(&[0, 127]);
        assert!(
            read_manifest(&bytes).is_none(),
            "truncated section accepted"
        );
        assert!(
            read_manifest(b"\0asm\x0d\0\x01\0").is_none(),
            "component accepted"
        );
    }

    // Claim: untrusted module metadata has one strict current format and a
    // finite positive bounded preferred size. Dropping those guards is Red.
    #[test]
    fn manifest_format_and_preferred_size_are_strict() {
        let good =
            "ducktape.view.manifest.v2\nSized\nDescription\nclock,store,\n640.5,480.25\n1\n1";
        let parsed = Manifest::parse(good).unwrap();
        assert_eq!(parsed.preferred_size.unwrap().dimensions(), [640.5, 480.25]);
        assert_eq!(parsed.capabilities, [Capability::Clock, Capability::Store]);
        assert!(
            Manifest::parse("ducktape.view.manifest.v2\nDefault\n\n\nnone\n1\n1")
                .unwrap()
                .preferred_size
                .is_none()
        );
        for invalid in [
            "Sized\nDescription\nclock,", // no header
            "ducktape.view.manifest.v1\nSized\nDescription\nclock,\nnone\n1", // no older format
            "ducktape.view.manifest.v2\nSized\nDescription\n\nnone",
            "ducktape.view.manifest.v2\nSized\nDescription\n\nnone\n1\n1\nextra",
            "ducktape.view.manifest.v2\nSized\nDescription\nclock\nnone\n1\n1",
            "ducktape.view.manifest.v2\nSized\nDescription\nclock,,\nnone\n1\n1",
            "ducktape.view.manifest.v2\nSized\nDescription\nclock,storage,\nnone\n1\n1",
        ] {
            assert!(
                Manifest::parse(invalid).is_none(),
                "accepted malformed manifest: {invalid}"
            );
        }
        for invalid in [
            "NaN,500",
            "inf,500",
            "-inf,500",
            "0,500",
            "-0,500",
            "-1,500",
            "8192.01,500",
            "1e40,500",
            "1e-50,500",
            "500,0",
            "1,2,3",
        ] {
            assert!(
                Manifest::parse(&format!(
                    "ducktape.view.manifest.v2\nSized\nDescription\n\n{invalid}\n1\n1"
                ))
                .is_none(),
                "accepted {invalid}"
            );
        }
        assert!(PreferredSize::new(8192.0, f32::MIN_POSITIVE).is_some());
        assert!(PreferredSize::new(f32::from_bits(1), 1.0).is_some());
        use std::hash::{Hash, Hasher};
        let value = PreferredSize::new(640.5, 480.25).unwrap();
        let mut hashes = [
            std::collections::hash_map::DefaultHasher::new(),
            std::collections::hash_map::DefaultHasher::new(),
        ];
        value.hash(&mut hashes[0]);
        parsed.preferred_size.unwrap().hash(&mut hashes[1]);
        assert_eq!(hashes[0].finish(), hashes[1].finish());
    }

    #[test]
    fn a_manifest_names_its_methods_revision_canonically() {
        let manifest = Manifest::parse("ducktape.view.manifest.v2\nApp\n\n\nnone\n2\n7").unwrap();
        assert_eq!((manifest.wire_epoch, manifest.methods), (2, 7));
        for invalid in [
            "ducktape.view.manifest.v2\nApp\n\n\nnone\n2",
            "ducktape.view.manifest.v2\nApp\n\n\nnone\n2\n0",
            "ducktape.view.manifest.v2\nApp\n\n\nnone\n2\n07",
        ] {
            assert!(Manifest::parse(invalid).is_none(), "accepted {invalid:?}");
        }
    }
}

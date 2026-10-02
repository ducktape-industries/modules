//! Static metadata for a ducktape view. Parsing never instantiates or runs a guest.

use crate::methods::Capability;

pub const MANIFEST_SECTION: &str = "ducktape.view.manifest";

/// The manifest's lines, in order: what `export_view!` writes and
/// [`Manifest::parse`] reads, listed in `tests/golden/schema.txt` so a
/// line added or moved moves [`crate::WIRE_ID`] like any other shape.
pub const LINES: [&str; 7] = [
    "ducktape.view.manifest",
    "name",
    "description",
    "capabilities",
    "min_width",
    "wire_id",
    "targets",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    /// the [`crate::WIRE_ID`] the view was built against
    pub wire_id: String,
    pub name: String,
    pub description: String,
    pub capabilities: Vec<Capability>,
    /// The view's `MIN_WINDOW_WIDTH`: the narrowest it is laid out, in
    /// logical px, `1..=8192`.
    pub min_width: u32,
    /// The programs the view may address with `op.submit`, `module.query`
    /// and `module.changes`; a view with the `op` or `module` capability
    /// names at least one.
    pub targets: Vec<String>,
}

/// What a manifest may say about itself. The catalog is read before anything
/// is installed, and the store shapes every field of every entry on every
/// relayout — outside the sandbox, with no fuel and no memory limit — so a
/// module whose manifest is a megabyte of capability names is left out of the
/// catalog rather than laid out.
const MAX_NAME_BYTES: usize = 64;
const MAX_DESCRIPTION_BYTES: usize = 256;
const MAX_CAPABILITIES: usize = 16;
const MAX_WIRE_ID_BYTES: usize = 16;
const MAX_TARGETS: usize = 16;

/// Whether a view declaring `capabilities` must name targets: it may
/// address a program only through `op` or `module`.
pub const fn needs_targets(capabilities: &[Capability]) -> bool {
    let mut i = 0;
    while i < capabilities.len() {
        if matches!(capabilities[i], Capability::Op | Capability::Module) {
            return true;
        }
        i += 1;
    }
    false
}

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
    /// Parses the strict seven-line ([`LINES`]) `ducktape.view.manifest`
    /// text and its bounds. A capability this host does not know refuses
    /// the whole manifest: a grant is never silently narrowed; so does a
    /// view that could address a program (`op`, `module`) and names no
    /// target.
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() > 1024 || text.chars().any(|c| c.is_control() && c != '\n') {
            return None;
        }
        let mut lines = text.split('\n');
        if lines.next()? != "ducktape.view.manifest" {
            return None;
        }
        let name = lines.next()?.to_owned();
        let description = lines.next()?.to_owned();
        let capabilities = list(lines.next()?)?
            .into_iter()
            .map(Capability::parse)
            .collect::<Option<_>>()?;
        // canonical decimal only: no sign, no leading zero, no spaces
        let text = lines.next()?;
        let min_width = text
            .parse::<u32>()
            .ok()
            .filter(|n| n.to_string() == text && (1..=crate::MAX_PIXELS as u32).contains(n))?;
        let wire_id = lines.next()?.to_owned();
        let targets: Vec<String> = list(lines.next()?)?
            .into_iter()
            .map(str::to_owned)
            .collect();
        if lines.next().is_some() {
            return None;
        }
        let manifest = Self {
            wire_id,
            name,
            description,
            capabilities,
            min_width,
            targets,
        };
        manifest.within_bounds().then_some(manifest)
    }

    fn within_bounds(&self) -> bool {
        !self.name.is_empty()
            && self.name.len() <= MAX_NAME_BYTES
            && self.description.len() <= MAX_DESCRIPTION_BYTES
            && self.capabilities.len() <= MAX_CAPABILITIES
            && !self.wire_id.is_empty()
            && self.wire_id.len() <= MAX_WIRE_ID_BYTES
            && self
                .wire_id
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
            && self.targets.len() <= MAX_TARGETS
            && self.targets.iter().all(|target| program::is_name(target))
            && (!self.targets.is_empty() || !needs_targets(&self.capabilities))
    }
}

/// A comma-terminated list line (`a,b,`): its items, none for an empty
/// line; `None` for a line that is not one.
fn list(line: &str) -> Option<Vec<&str>> {
    match line.is_empty() {
        true => Some(Vec::new()),
        false => Some(line.strip_suffix(',')?.split(',').collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "manifest")]
    #[test]
    fn extraction_rejects_duplicate_and_truncated_sections() {
        let mut bytes = b"\0asm\x01\0\0\0".to_vec();
        let text = b"ducktape.view.manifest\nSized\n\n\n640\n0123abcd\n";
        let mut section = vec![
            0,
            (1 + MANIFEST_SECTION.len() + text.len()) as u8,
            MANIFEST_SECTION.len() as u8,
        ];
        section.extend_from_slice(MANIFEST_SECTION.as_bytes());
        section.extend_from_slice(text);
        bytes.extend_from_slice(&section);
        assert_eq!(read_manifest(&bytes).unwrap().min_width, 640);
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

    // Claim: untrusted module metadata has one strict format. Dropping
    // those guards is Red.
    #[test]
    fn the_manifest_format_is_strict() {
        let good = "ducktape.view.manifest\nSized\nDescription\nclock,store,\n480\n0123abcd\n";
        let parsed = Manifest::parse(good).unwrap();
        assert_eq!(parsed.capabilities, [Capability::Clock, Capability::Store]);
        assert_eq!(
            (&*parsed.name, &*parsed.description),
            ("Sized", "Description")
        );
        assert!(parsed.targets.is_empty());
        for invalid in [
            "Sized\nDescription\nclock,", // no header
            "ducktape.view\nSized\nDescription\nclock,\n480\n0123abcd\n", // another header
            "ducktape.view.manifest\nSized\nDescription\n\n480",
            "ducktape.view.manifest\nSized\nDescription\n\n480\n0123abcd", // six lines: the shape before targets
            "ducktape.view.manifest\nSized\nDescription\n\n480\n0123abcd\n\nextra",
            "ducktape.view.manifest\nSized\nDescription\nclock\n480\n0123abcd\n",
            "ducktape.view.manifest\nSized\nDescription\nclock,,\n480\n0123abcd\n",
            "ducktape.view.manifest\nSized\nDescription\nclock,storage,\n480\n0123abcd\n",
        ] {
            assert!(
                Manifest::parse(invalid).is_none(),
                "accepted malformed manifest: {invalid}"
            );
        }
    }

    // Claim: line 5 is the min width, canonical decimal in 1..=8192, and a
    // manifest of the shape before it (a preferred size there, or one line
    // more) is refused rather than read with a guessed width.
    #[test]
    fn a_manifest_carries_its_min_width() {
        let parse = |width: &str| {
            Manifest::parse(&format!(
                "ducktape.view.manifest\nApp\n\n\n{width}\n0123abcd\n"
            ))
        };
        assert_eq!(parse("480").unwrap().min_width, 480);
        assert_eq!(parse("1").unwrap().min_width, 1);
        assert_eq!(parse("8192").unwrap().min_width, 8192);
        for invalid in [
            "",
            "0",
            "01",
            "+480",
            "-1",
            "480.0",
            " 480",
            "8193",
            "4294967296",
            "none",
            "1180,760",
        ] {
            assert!(parse(invalid).is_none(), "accepted {invalid:?}");
        }
        // every line valid on its own (`480` is hex, so a wire id too): only
        // the eighth line refuses it
        assert!(
            Manifest::parse("ducktape.view.manifest\nApp\n\n\n480\n480\n0123abcd\n").is_none(),
            "an eighth line accepted"
        );
    }

    // Claim: line 7 names the programs the view may address, in the list
    // grammar of line 4; a view that could address one (`op`, `module`)
    // and names none is refused, so no view signs for "any program".
    #[test]
    fn a_manifest_names_its_targets_when_it_can_address_a_program() {
        let parse = |caps: &str, targets: &str| {
            Manifest::parse(&format!(
                "ducktape.view.manifest\nApp\n\n{caps}\n480\n0123abcd\n{targets}"
            ))
        };
        assert_eq!(
            parse("op,module,", "chat,identity,").unwrap().targets,
            ["chat", "identity"]
        );
        assert_eq!(parse("module,", "a-b_c9,").unwrap().targets, ["a-b_c9"]);
        assert!(parse("clock,", "").unwrap().targets.is_empty());
        assert!(
            parse("", "chat,").is_some(),
            "a target without op is only unused"
        );
        for (caps, targets) in [
            ("op,", ""),       // could submit, names no program
            ("module,", ""),   // could query, names no program
            ("op,", "chat"),   // no trailing comma
            ("op,", "chat,,"), // an empty name
            ("op,", "a/b,"),   // not a program name
            ("op,", &format!("{},", "x".repeat(65))),
            ("op,", &"chat,".repeat(MAX_TARGETS + 1)),
        ] {
            assert!(
                parse(caps, targets).is_none(),
                "accepted {caps:?} {targets:?}"
            );
        }
    }

    // Claim: the wire id is short lowercase hex, so a host can show it in a
    // refusal; a different id still parses, since refusing it is the host's.
    #[test]
    fn the_wire_id_is_bounded_lowercase_hex() {
        let parse =
            |id: &str| Manifest::parse(&format!("ducktape.view.manifest\nApp\n\n\n480\n{id}\n"));
        assert_eq!(
            parse("0123456789abcdef").unwrap().wire_id,
            "0123456789abcdef"
        );
        assert!(parse(crate::WIRE_ID).is_some());
        for invalid in ["", "0123456789abcdef0", "ABCDEF", "xyz", " 0a", "0a "] {
            assert!(parse(invalid).is_none(), "accepted {invalid:?}");
        }
    }
}

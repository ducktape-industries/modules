//! Packs what `make wasm-modules` builds into a founding: each program with its describe module
//! and any view embedded, forge's and halo's params, and a founding file `ducktape init` reads.

use std::borrow::Cow;
use std::fs;
use std::path::Path;

use abi::role::halo::{Class, Genesis};
use wasm_encoder::{CustomSection, Encode, Section};
use wasmparser::{Encoding, Parser, Payload, Validator};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub const VIEW_SECTION: &str = "ducktape.view";
pub const DESCRIBE_SECTION: &str = "ducktape.describe";
pub const FOUNDING: &str = include_str!("../founding.toml");

pub const PROGRAMS: [(&str, &str); 5] = [
    ("programs", "settings_view"),
    ("valset", "node_view"),
    ("identity", "members_view"),
    ("chat", "chat_view"),
    ("forge", "forge_view"),
];
pub const UNVIEWED: [&str; 8] = [
    "blobs",
    "awaits",
    "halo",
    "activity",
    "files",
    "provider",
    "attribution",
    "harness",
];
pub const VIEWS: [&str; 1] = ["explorer_view"];

pub fn forge_bounds() -> forge::Bounds {
    forge::Bounds {
        max_objects: 1_000_000,
        max_delta_depth: 64,
        max_object_size: 256 << 20,
        push_walk: 10_000,
        fetch_walk: 1_000_000,
        merge_cost: 1024,
        page_size: 128,
        log_walk: 10_000,
        tree_walk: 1024,
        diff_bytes: 32 << 20,
        blob_bytes: 256 << 10,
        record_bytes: 64 << 10,
    }
}

/// The lanes halo serves from the first block, given ids in order from 0. blobs' lane is
/// `Critical`: a node that lags fetches the blobs a block names at that class before it can
/// apply the block.
pub fn halo_lanes() -> Genesis {
    Genesis {
        lanes: vec![("ducktape/blobs".into(), Class::Critical)],
    }
}

pub fn bundle(release: &Path, out: &Path) -> Result<()> {
    fs::create_dir_all(out)?;
    let read = |name: &str| {
        let path = release.join(format!("{name}.wasm"));
        fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))
    };
    for (program, view) in PROGRAMS {
        let packed = embed(&read(program)?, &read(view)?)?;
        let packed = embed_describe(&packed, &read(&format!("{program}.describe"))?)?;
        fs::write(out.join(format!("{program}.wasm")), packed)?;
    }
    for program in UNVIEWED {
        let packed = embed_describe(&read(program)?, &read(&format!("{program}.describe"))?)?;
        fs::write(out.join(format!("{program}.wasm")), packed)?;
    }
    for view in VIEWS {
        let bytes = read(view)?;
        validate(&bytes)?;
        fs::write(out.join(format!("{view}.wasm")), bytes)?;
    }
    fs::write(out.join("forge.params"), abi::encode(&forge_bounds()))?;
    fs::write(out.join("halo.params"), abi::encode(&halo_lanes()))?;
    fs::write(out.join("founding.toml"), FOUNDING)?;
    Ok(())
}

fn validate(bytes: &[u8]) -> Result<()> {
    let module = matches!(
        Parser::new(0).parse_all(bytes).next(),
        Some(Ok(Payload::Version {
            encoding: Encoding::Module,
            ..
        }))
    );
    if !module {
        return Err("expected a core WebAssembly module".into());
    }
    Validator::new().validate_all(bytes)?;
    Ok(())
}

pub fn strip_section(program: &[u8], name: &str) -> Result<Vec<u8>> {
    validate(program)?;
    let mut output = Vec::with_capacity(program.len());
    let mut keep = 0;
    let mut section_start = 8;
    for payload in Parser::new(0).parse_all(program) {
        let payload = payload?;
        let Some((_, range)) = payload.as_section() else {
            continue;
        };
        let named = matches!(&payload, Payload::CustomSection(s) if s.name() == name);
        if named {
            output.extend_from_slice(&program[keep..section_start]);
            keep = range.end;
        }
        section_start = range.end;
    }
    output.extend_from_slice(&program[keep..]);
    Ok(output)
}

pub fn embed(program: &[u8], view: &[u8]) -> Result<Vec<u8>> {
    validate(view)?;
    embed_section(program, VIEW_SECTION, view)
}

pub fn embed_describe(program: &[u8], module: &[u8]) -> Result<Vec<u8>> {
    validate(module)?;
    for payload in Parser::new(0).parse_all(module) {
        if let Payload::ImportSection(imports) = payload?
            && imports.count() > 0
        {
            return Err("a describe module imports nothing".into());
        }
    }
    embed_section(program, DESCRIBE_SECTION, module)
}

fn embed_section(program: &[u8], name: &str, payload: &[u8]) -> Result<Vec<u8>> {
    let mut output = strip_section(program, name)?;
    let section = CustomSection {
        name: Cow::Borrowed(name),
        data: Cow::Borrowed(payload),
    };
    output.push(section.id());
    section.encode(&mut output);
    Ok(output)
}

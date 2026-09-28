//! `WIRE_ID`: FNV-1a 64 over the committed golden files `tests/golden.rs`
//! writes under `WIRE_GOLDEN_WRITE=1`: the bytes of every node, event and
//! method the fixtures sample, and the shape of every type that crosses
//! (`schema.txt`). Regenerating them is the one way the id moves.

const GOLDEN: [&str; 3] = [
    "tests/golden/frame.bin",
    "tests/golden/methods.bin",
    "tests/golden/schema.txt",
];

fn main() {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for path in GOLDEN {
        println!("cargo:rerun-if-changed={path}");
        let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{path}: {error}"));
        for byte in bytes {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("wire_id.rs");
    std::fs::write(out, format!("\"{hash:016x}\"")).unwrap();
}

//! `founding RELEASE OUT`: writes the founding bundle `make wasm-publish` releases.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [release, out] = args.as_slice() else {
        eprintln!("usage: founding RELEASE_DIR OUT_DIR");
        return ExitCode::FAILURE;
    };
    match founding::bundle(release, out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("founding: {error}");
            ExitCode::FAILURE
        }
    }
}

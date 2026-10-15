use std::fs;
use std::path::Path;

use wasm_encoder::{CustomSection, Module};
use wasmparser::{Parser, Payload};

fn section(bytes: &[u8], name: &str) -> Vec<Vec<u8>> {
    Parser::new(0)
        .parse_all(bytes)
        .filter_map(|payload| match payload.unwrap() {
            Payload::CustomSection(section) if section.name() == name => {
                Some(section.data().to_vec())
            }
            _ => None,
        })
        .collect()
}

fn module(tag: &str) -> Vec<u8> {
    let mut module = Module::new();
    module.section(&CustomSection {
        name: "tag".into(),
        data: tag.as_bytes().into(),
    });
    module.finish()
}

#[test]
fn embedding_replaces_and_stripping_restores_the_program() {
    let program = module("program");
    let view = Module::new().finish();
    let packed = founding::embed(&program, &view).unwrap();
    assert_eq!(
        section(&packed, founding::VIEW_SECTION),
        std::slice::from_ref(&view)
    );
    assert_eq!(founding::embed(&packed, &view).unwrap(), packed);
    assert_eq!(
        founding::strip_section(&packed, founding::VIEW_SECTION).unwrap(),
        program
    );
    let replaced = founding::embed(&packed, &program).unwrap();
    assert_eq!(
        section(&replaced, founding::VIEW_SECTION),
        std::slice::from_ref(&program)
    );
    let mut duplicate = packed.clone();
    duplicate.extend_from_slice(&packed[program.len()..]);
    assert_eq!(founding::embed(&duplicate, &view).unwrap(), packed);
    assert!(founding::embed(b"broken", &view).is_err());
    assert!(founding::embed(&program, b"broken").is_err());
    assert!(founding::embed(&program, b"\0asm\x0d\0\x01\0").is_err());
}

#[test]
fn a_describe_module_rides_beside_the_view_and_imports_nothing() {
    let program = Module::new().finish();
    let view = module("view");
    let describe = module("describe");
    let packed = founding::embed(&program, &view).unwrap();
    let both = founding::embed_describe(&packed, &describe).unwrap();
    assert_eq!(section(&both, founding::VIEW_SECTION), [view]);
    assert_eq!(
        section(&both, founding::DESCRIBE_SECTION),
        std::slice::from_ref(&describe)
    );
    assert_eq!(founding::embed_describe(&both, &describe).unwrap(), both);
    assert_eq!(
        founding::strip_section(&both, founding::DESCRIBE_SECTION).unwrap(),
        packed
    );
    let mut importing = Module::new();
    let mut types = wasm_encoder::TypeSection::new();
    types.ty().function([], []);
    importing.section(&types);
    let mut imports = wasm_encoder::ImportSection::new();
    imports.import("env", "f", wasm_encoder::EntityType::Function(0));
    importing.section(&imports);
    assert!(founding::embed_describe(&program, &importing.finish()).is_err());
}

fn release(dir: &Path) {
    for (program, view) in founding::PROGRAMS {
        fs::write(dir.join(format!("{program}.wasm")), module(program)).unwrap();
        fs::write(dir.join(format!("{view}.wasm")), module(view)).unwrap();
        let describe = format!("{program}.describe");
        fs::write(dir.join(format!("{describe}.wasm")), module(&describe)).unwrap();
    }
    for program in founding::UNVIEWED {
        fs::write(dir.join(format!("{program}.wasm")), module(program)).unwrap();
        let describe = format!("{program}.describe");
        fs::write(dir.join(format!("{describe}.wasm")), module(&describe)).unwrap();
    }
    for view in founding::VIEWS {
        fs::write(dir.join(format!("{view}.wasm")), module(view)).unwrap();
    }
}

#[test]
fn the_bundle_names_every_file_it_holds_and_packs_each_program_with_its_own_view() {
    let built = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    release(built.path());
    founding::bundle(built.path(), out.path()).unwrap();

    for (program, view) in founding::PROGRAMS {
        let packed = fs::read(out.path().join(format!("{program}.wasm"))).unwrap();
        assert_eq!(section(&packed, "tag"), [program.as_bytes().to_vec()]);
        assert_eq!(section(&packed, founding::VIEW_SECTION), [module(view)]);
        let describe = module(&format!("{program}.describe"));
        assert_eq!(section(&packed, founding::DESCRIBE_SECTION), [describe]);
    }
    for program in founding::UNVIEWED {
        let packed = fs::read(out.path().join(format!("{program}.wasm"))).unwrap();
        assert_eq!(section(&packed, "tag"), [program.as_bytes().to_vec()]);
        assert!(section(&packed, founding::VIEW_SECTION).is_empty());
        let describe = module(&format!("{program}.describe"));
        assert_eq!(section(&packed, founding::DESCRIBE_SECTION), [describe]);
    }
    let params = fs::read(out.path().join("forge.params")).unwrap();
    assert_eq!(
        abi::decode::<forge::Bounds>(&params).unwrap(),
        founding::forge_bounds()
    );
    let params = fs::read(out.path().join("halo.params")).unwrap();
    assert_eq!(
        abi::decode::<abi::role::halo::Genesis>(&params).unwrap(),
        founding::halo_lanes()
    );

    let text = fs::read_to_string(out.path().join("founding.toml")).unwrap();
    let file: toml::Table = toml::from_str(&text).unwrap();
    let mut named: Vec<String> = ["programs", "views"]
        .iter()
        .flat_map(|list| file[*list].as_array().unwrap())
        .flat_map(|entry| ["code", "params"].map(|key| entry.get(key)))
        .flatten()
        .map(|path| path.as_str().unwrap().to_string())
        .collect();
    named.push("founding.toml".into());
    let founded: Vec<&str> = file["programs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|program| program["id"].as_str().unwrap())
        .collect();
    for hook in file["hooks"].as_array().unwrap() {
        let hook = hook.as_str().unwrap();
        assert!(founded.contains(&hook), "{hook} hooks but is not founded");
    }
    named.sort();
    let mut held: Vec<String> = fs::read_dir(out.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    held.sort();
    assert_eq!(named, held);
}

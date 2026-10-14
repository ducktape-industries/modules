//! A program's code blob as genesis packs it: a core wasm module that is
//! nothing but custom sections, the view in `ducktape.view` and the
//! describe module in `ducktape.describe`, which the app reads out by name
//! (`backend::views::view_section`, `describe::host::section`).

/// A core module holding `sections` and nothing else.
pub fn wrapper(sections: &[(&str, &[u8])]) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    for (name, data) in sections {
        let mut body = Vec::new();
        leb128(&mut body, name.len() as u64);
        body.extend_from_slice(name.as_bytes());
        body.extend_from_slice(data);
        // custom section id, then its size
        module.push(0);
        leb128(&mut module, body.len() as u64);
        module.extend_from_slice(&body);
    }
    module
}

fn leb128(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one layout the app's own test pins (`the_view_section_is_read_out_of_a_core_module`).
    #[test]
    fn a_wrapper_is_a_module_of_named_custom_sections() {
        let module = wrapper(&[("ducktape.view", b"view bytes")]);
        let mut expected = b"\0asm\x01\0\0\0".to_vec();
        let name = b"ducktape.view";
        let mut section = vec![name.len() as u8];
        section.extend_from_slice(name);
        section.extend_from_slice(b"view bytes");
        expected.push(0);
        expected.push(section.len() as u8);
        expected.extend_from_slice(&section);
        assert_eq!(module, expected);
    }

    #[test]
    fn a_long_section_takes_a_two_byte_size() {
        let data = vec![7u8; 200];
        let module = wrapper(&[("d", &data)]);
        // 1 (name len) + 1 (name) + 200 = 202 = 0xca: LEB128 `ca 01`
        assert_eq!(&module[8..12], &[0, 0xca, 0x01, 1]);
        assert_eq!(module.len(), 8 + 3 + 202);
    }
}

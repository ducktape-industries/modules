//! Explorer's pages as short `duck://explorer/<path>` links: the one
//! spelling every view opens a block, a transaction or an account by.
//! Explorer's own `Route::path` writes the same paths through these.

/// `block/<height>`
pub fn block_path(height: u64) -> String {
    format!("block/{height}")
}

/// `tx/<hash hex>`
pub fn tx_path(hash: &[u8]) -> String {
    let hex: String = hash.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("tx/{hex}")
}

/// `account/<number>`
pub fn account_path(number: u64) -> String {
    format!("account/{number}")
}

/// `duck://explorer/<path>`: the host opens Explorer at `path`.
pub fn link(path: &str) -> String {
    format!("duck://explorer/{path}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn explorer_links_spell_explorers_paths() {
        use super::*;
        assert_eq!(link(&block_path(30)), "duck://explorer/block/30");
        assert_eq!(link(&tx_path(&[0xab, 0x01])), "duck://explorer/tx/ab01");
        assert_eq!(link(&account_path(7)), "duck://explorer/account/7");
    }
}

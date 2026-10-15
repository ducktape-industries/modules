//! What a view shows of a frame in a list: how it ran in a word, and the name its row is read by.
//! Explorer's frame lists and members' recent activity both draw frames with it.

pub fn verdict(applied: bool) -> &'static str {
    match applied {
        true => "Accepted",
        false => "Rejected",
    }
}

pub fn label(target: &str, applied: bool, height: &str, position: u32) -> String {
    let word = verdict(applied);
    format!("{target}, {word}, block {height} · {position}")
}

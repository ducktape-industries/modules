//! Which host-window control an element stands in for: the drag region or a
//! caption button. No native window ID crosses.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowControlArea {
    Drag,
    Close,
    Max,
    Min,
}

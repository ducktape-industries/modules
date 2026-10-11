//! Property tests for the wire's hostile-input contract: a random tree either
//! comes back out of `decode` refused for a reason the budget actually names,
//! or `sanitize` pulls it inside every bound `sanitize_node` promises; bytes a
//! hostile guest could have written never make `decode` panic; and a
//! hand-crafted length-prefix bomb is refused before it is walked.
//!
//! No new dependency: the generator is a splitmix64 PRNG seeded by a fixed
//! constant, so a failure prints its seed and the run reproduces exactly.

use std::collections::HashSet;

use view_wire::*;

mod common;
use common::{PLAIN, plain, sanitize};

mod hostile {
    use super::*;

    mod rng;
    use rng::*;
    mod table;
    use table::*;
    mod leaves;
    use leaves::*;
    mod generation;
    use generation::*;
    mod checks;
    use checks::*;
    mod styles;
    use styles::*;
    mod aria;
    mod cases;
}

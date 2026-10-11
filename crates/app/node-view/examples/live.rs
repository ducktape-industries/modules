//! `cargo run -p node-view --example live`: Nodes in the real app over a
//! fake node whose one validator is the dev key, plus a resident.
use ducktape_view_guest::live;
use valset::{Motion, Op};

fn main() {
    live::run(valset::MODULE, |net| {
        let ada = net.person("ada");
        net.submit(
            net.me(),
            valset::MODULE,
            &Op::Vote(Motion::Door { open: true }),
        )?;
        net.submit(
            ada,
            valset::MODULE,
            &Op::Enroll {
                address: Some("10.0.0.2:4000".into()),
                invite: None,
            },
        )
        .map(drop)
    })
}

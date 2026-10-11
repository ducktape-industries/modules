//! `cargo run -p node-view --example live`: Nodes in the real app over a
//! fake node whose one validator is the dev key, plus a resident.
use ducktape_view_guest::live;
use valset::{Membership, Op, Role};

fn main() {
    live::run(valset::MODULE, |net| {
        let ada = net.person("ada");
        net.system(
            valset::MODULE,
            &Op::Set(Membership {
                key: net.key_of(ada),
                address: "10.0.0.2:4000".into(),
                role: Role::Resident,
            }),
        )
        .map(drop)
    })
}

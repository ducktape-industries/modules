//! `cargo run -p members-view --example live`: Members in the real app
//! over a fake node with a few people, an agent and a resident node.
use ducktape_view_guest::live;
use valset::{Membership, Op, Role};

fn main() {
    live::run(identity::MODULE, |net| {
        let ada = net.person("ada");
        net.person("bob");
        net.submit(
            net.me(),
            identity::MODULE,
            &identity::Op::CreateAgent {
                name: "reviewer".into(),
            },
        )?;
        net.system(
            valset::MODULE,
            &Op::Set(Membership {
                key: net.key_of(ada),
                address: "10.0.0.2:4000".into(),
                role: Role::Resident,
            }),
        )?;
        Ok(())
    })
}

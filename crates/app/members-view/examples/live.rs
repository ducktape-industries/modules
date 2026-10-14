//! `cargo run -p members-view --example live`: Members in the real app
//! over a fake node with a few people, an agent and a resident node.
use ducktape_view_guest::rig;
use valset::{Motion, Op};

fn main() {
    rig::run(identity::MODULE, |net| {
        let ada = net.person("ada");
        net.person("bob");
        net.submit(
            net.me(),
            identity::MODULE,
            &identity::Op::CreateAgent {
                name: "reviewer".into(),
            },
        )?;
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
        )?;
        Ok(())
    })
}

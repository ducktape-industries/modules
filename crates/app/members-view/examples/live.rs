//! `cargo run -p members-view --example live`: Members in the real app
//! over a fake node with a few people, an agent and a resident node.
use valset::{Motion, Op};
use view::live;

fn main() {
    live::run(identity::PROGRAM, |net| {
        let ada = net.person("ada");
        net.person("bob");
        net.submit(
            net.me(),
            identity::PROGRAM,
            &identity::Op::CreateAgent {
                name: "reviewer".into(),
            },
        )?;
        net.submit(
            net.me(),
            valset::PROGRAM,
            &Op::Vote(Motion::Door { open: true }),
        )?;
        net.submit(
            ada,
            valset::PROGRAM,
            &Op::Enroll {
                address: Some("10.0.0.2:4000".into()),
                invite: None,
            },
        )?;
        Ok(())
    })
}

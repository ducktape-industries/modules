//! `cargo run -p settings-view --example live`: Account in the real app
//! over a fake node where the dev account manages an agent.
use view::rig;

fn main() {
    rig::run("programs", |net| {
        net.person("ada");
        net.submit(
            net.me(),
            identity::PROGRAM,
            &identity::Op::CreateAgent {
                name: "deploy-bot".into(),
            },
        )
        .map(drop)
    })
}

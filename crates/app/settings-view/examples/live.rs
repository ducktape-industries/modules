//! `cargo run -p settings-view --example live`: Account in the real app
//! over a fake node where the dev account manages an agent.
use ducktape_view_guest::live;

fn main() {
    live::run("module-registry", |net| {
        net.person("ada");
        net.submit(
            net.me(),
            identity::MODULE,
            &identity::Op::CreateAgent {
                name: "deploy-bot".into(),
            },
        )
        .map(drop)
    })
}

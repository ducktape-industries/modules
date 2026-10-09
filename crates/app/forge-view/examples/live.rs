//! `cargo run -p forge-view --example live`: Forge in the real app over a
//! fake node with a repository and the people who may push to it.
use ducktape_view_guest::live;
use forge::{Bounds, Op};

fn main() {
    live::run("forge", |net| {
        net.seat::<chat::Chat>(chat::MODULE);
        net.seat::<forge::Forge>(forge::MODULE);
        net.init(
            forge::MODULE,
            &Bounds {
                max_objects: 10_000,
                max_delta_depth: 64,
                max_object_size: 1 << 20,
                push_walk: 1000,
                fetch_walk: 1000,
                merge_cost: 100_000,
                page_size: 128,
                log_walk: 1000,
                tree_walk: 256,
                diff_bytes: 4 << 20,
                blob_bytes: 1 << 20,
                record_bytes: 64 << 10,
            },
        )?;
        let ada = net.person("ada");
        for repo in ["web", "docs"] {
            net.submit(
                ada,
                forge::MODULE,
                &Op::Create {
                    repo: repo.into(),
                    hash: abi::HashKind::Sha256,
                },
            )?;
            net.submit(
                ada,
                forge::MODULE,
                &Op::Grant {
                    repo: repo.into(),
                    principal: forge::Principal::Account(net.me()),
                },
            )?;
        }
        Ok(())
    })
}

//! `cargo run -p explorer-view --example live`: Explorer in the real app
//! over a fake node whose blocks carry a few people and chat posts.
use chat::{Block, Op, PostPolicy};
use ducktape_view_guest::live;

fn main() {
    live::run("explorer", |net| {
        net.seat::<chat::Chat>(chat::MODULE);
        let ada = net.person("ada");
        let bob = net.person("bob");
        net.submit(
            ada,
            chat::MODULE,
            &Op::CreateChannel {
                channel_id: "general".into(),
                name: "General".into(),
                post_policy: PostPolicy::Open,
            },
        )?;
        for (seq, (who, text)) in [(ada, "first"), (bob, "second"), (ada, "third")]
            .into_iter()
            .enumerate()
        {
            net.submit(
                who,
                chat::MODULE,
                &Op::PostMessage {
                    channel_id: "general".into(),
                    message_id: format!("m{seq}"),
                    blocks: vec![Block::paragraph(text)],
                    thread: None,
                },
            )?;
        }
        Ok(())
    })
}

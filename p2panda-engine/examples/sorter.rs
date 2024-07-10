use futures::channel::mpsc;
use futures::StreamExt;
use p2panda_core::{Body, Hash, Header, Operation, PrivateKey};
use p2panda_engine::{EngineBuilder, Router, Sorter, SorterRoute, StreamEvent};
use tokio::task::LocalSet;

const SORTER_CHANNEL_CAPACITY: usize = 1000;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // TODO: The sorter spawns a new task inside a local task, and I couldn't
    // get this to work using futures::executor::LocalPool. Might have been doing 
    // something wrong, but it seemed like the sub-task was never starting. It just 
    // worked with tokio::task::LocalSet....
    let local = LocalSet::new();

    let (sender, receiver) = mpsc::channel(SORTER_CHANNEL_CAPACITY);
    let sorter = Sorter::new(sender);
    let sorter_route = SorterRoute::new(receiver, sorter);

    let router = Router::<()>::new().route(sorter_route);
    let engine = EngineBuilder::new().router(router).build();

    {
        let mut engine = engine.clone();
        local.spawn_local(async move {
            let private_key = PrivateKey::new();
            let mut backlink: Option<Hash> = None;
            let mut seq_num = 0;

            loop {
                let body: Body = Body::new(&[1, 2, 3]);
                let mut header = Header::<()> {
                    version: 1,
                    public_key: private_key.public_key(),
                    signature: None,
                    payload_size: body.size(),
                    payload_hash: Some(body.hash()),
                    timestamp: 0,
                    seq_num,
                    backlink,
                    previous: vec![],
                    extensions: None,
                };
                header.sign(&private_key);

                backlink = Some(header.hash());
                seq_num += 1;

                let operation = Operation {
                    hash: header.hash(),
                    header,
                    body: Some(body),
                };

                engine.ingest(operation).await;
            }
        });
    }

    {
        let mut engine = engine.clone();
        local.spawn_local(async move {
            while let Some(Ok(event)) = engine.next().await {
                match event {
                    StreamEvent::Commit(operation) => {
                        println!(
                            "commit: {:?}",
                            (operation.header.seq_num, operation.hash.to_hex())
                        )
                    }
                    StreamEvent::Replay(operations) => println!(
                        "replay: {:?}",
                        operations
                            .iter()
                            .map(|operation| (operation.header.seq_num, operation.hash.to_hex()))
                            .collect::<Vec<(u64, String)>>()
                    ),
                    StreamEvent::None => (),
                }
            }
        });
    }

    local.await;
}

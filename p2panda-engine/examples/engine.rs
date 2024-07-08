use futures::executor::LocalPool;
use futures::stream::StreamExt;
use futures::task::LocalSpawnExt;
use p2panda_core::{Body, Hash, Operation, PrivateKey, UnsignedHeader};
use p2panda_engine::engine::EngineBuilder;
use p2panda_engine::ingest::IngestResult;
use p2panda_engine::{Ingest, Layer, Router, StreamEvent};

fn main() {
    let mut pool = LocalPool::new();
    let spawner = pool.spawner();

    #[derive(Clone)]
    struct TestRoute {}

    #[derive(Clone)]
    struct TestRouteService<M> {
        inner: Option<M>,
    }

    impl TestRouteService<()> {
        pub fn new() -> Self {
            Self { inner: None }
        }
    }

    impl Ingest<()> for TestRoute {
        async fn ingest(
            &mut self,
            _context: p2panda_engine::Context,
            operation: &Operation<()>,
        ) -> IngestResult<()> {
            Ok(StreamEvent::Commit(operation.clone()))
        }
    }

    impl<M> Layer<M> for TestRoute {
        type Middleware = TestRouteService<M>;

        fn layer(&self, inner: M) -> Self::Middleware {
            TestRouteService { inner: Some(inner) }
        }
    }

    let test_route = TestRoute {};

    let router = Router::<()>::new().route(test_route);
    let engine = EngineBuilder::new().router(router).build();

    {
        let mut engine = engine.clone();
        spawner
            .spawn_local(async move {
                let private_key = PrivateKey::new();
                let mut backlink: Option<Hash> = None;
                let mut seq_num = 0;

                loop {
                    let body: Body = Body::new(&[1, 2, 3]);
                    let header = UnsignedHeader::<()> {
                        version: 1,
                        public_key: private_key.public_key(),
                        payload_size: body.size(),
                        payload_hash: Some(body.hash()),
                        timestamp: 0,
                        seq_num,
                        backlink,
                        previous: vec![],
                        extension: None,
                    };

                    let header = header.sign(&private_key);
                    backlink = Some(header.hash());
                    seq_num += 1;

                    let operation = Operation {
                        hash: header.hash(),
                        header,
                        body: Some(body),
                    };

                    engine.ingest(operation).await;
                }
            })
            .unwrap();
    }

    {
        let mut engine = engine.clone();
        spawner
            .spawn_local(async move {
                while let Some(Ok(event)) = engine.next().await {
                    println!("{:?}", event);
                }
            })
            .unwrap();
    }

    pool.run();
}

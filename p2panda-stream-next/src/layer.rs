// SPDX-License-Identifier: MIT OR Apache-2.0

pub trait Layer<Input> {
    type Output;

    type Error;

    fn process(&self, input: Input) -> impl Future<Output = Result<Self::Output, Self::Error>>;
}

pub trait LayerExt<Input>: Layer<Input> + Sized {
    fn and_then<L>(self, next: L) -> impl Layer<Input, Output = L::Output, Error = Self::Error>
    where
        L: Layer<Self::Output, Error = Self::Error>,
    {
        Chain::new(self, next)
    }
}

impl<L, Input> LayerExt<Input> for L where L: Layer<Input> {}

pub struct Chain<L1, L2> {
    first: L1,
    second: L2,
}

impl<L1, L2> Chain<L1, L2> {
    pub fn new(first: L1, second: L2) -> Self {
        Self { first, second }
    }
}

impl<L1, L2, Input> Layer<Input> for Chain<L1, L2>
where
    L1: Layer<Input>,
    L2: Layer<L1::Output, Error = L1::Error>,
{
    type Output = L2::Output;

    type Error = L1::Error;

    async fn process(&self, input: Input) -> Result<Self::Output, Self::Error> {
        let intermediate = self.first.process(input).await?;
        self.second.process(intermediate).await
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{Layer, LayerExt};

    #[derive(Clone, Debug, PartialEq)]
    struct Message {
        payload: Vec<u8>,
    }

    // Timestamp layer example.

    struct TimestampLayer;

    #[allow(dead_code)]
    struct WithTimestamp<T> {
        pub item: T,
        pub timestamp: u64,
    }

    impl<T> Layer<T> for TimestampLayer {
        type Output = WithTimestamp<T>;

        type Error = Infallible;

        async fn process(&self, input: T) -> Result<Self::Output, Self::Error> {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            Ok(WithTimestamp {
                item: input,
                timestamp,
            })
        }
    }

    // Coloring layer example.

    #[derive(Clone)]
    enum Color {
        Red,
        Green,
        #[allow(dead_code)]
        Blue,
    }

    struct ColoringLayer {
        color: Color,
    }

    impl ColoringLayer {
        fn new(color: Color) -> Self {
            Self { color }
        }
    }

    #[allow(dead_code)]
    struct WithColor<T> {
        pub item: T,
        pub color: Color,
    }

    impl<T> Layer<T> for ColoringLayer {
        type Output = WithColor<T>;

        type Error = Infallible;

        async fn process(&self, input: T) -> Result<Self::Output, Self::Error> {
            Ok(WithColor {
                item: input,
                color: self.color.clone(),
            })
        }
    }

    #[tokio::test]
    async fn simple_layer_impl() {
        let timestamp = TimestampLayer;

        let message = Message {
            payload: vec![1, 2, 3],
        };

        let _result = timestamp.process(message.clone()).await.unwrap();
    }

    #[tokio::test]
    async fn layer_chaining() {
        // Can chain together different layers for processing.
        let timestamp = TimestampLayer;
        let color = ColoringLayer::new(Color::Red);

        let message = Message {
            payload: vec![1, 2, 3],
        };

        let chain = timestamp.and_then(color);
        let _result = chain.process(message.clone()).await.unwrap();

        // Can change the order of layers.
        let timestamp_2 = TimestampLayer;
        let color_2 = ColoringLayer::new(Color::Green);

        let different_order_chain = color_2.and_then(timestamp_2);
        let _result = different_order_chain.process(message).await.unwrap();
    }
}

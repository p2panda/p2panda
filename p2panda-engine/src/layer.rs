// SPDX-License-Identifier: AGPL-3.0-or-later

pub trait Layer<M> {
    type Middleware;

    fn layer(&self, inner: M) -> Self::Middleware;
}

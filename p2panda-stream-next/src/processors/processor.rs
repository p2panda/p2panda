// SPDX-License-Identifier: MIT OR Apache-2.0

/// Interface for implementing data processors.
///
/// Processors take in items (input) and yield "processed" or "enriched" versions of them at _some
/// point_ (output). Please note that these items do _not_ necessarily need to come out in the same
/// order as they came in. Processors can keep items around until it's internal logic decided
/// they're ready to go, and this can happen in any order required.
///
/// Users of processors will use the `process` method to insert "input" items and call `next` to
/// find out if there's ready processed "output" items to take out.
///
/// The `process` method might apply back-pressure, for example when the processor is busy and
/// can't take in more work.
///
/// Processors _never_ terminate. The future returned by `next` will stay in pending state whenever
/// there's no work to do or the processor can't continue because of it's internal logic.
///
/// Users can decide to drop a processor or escalate to a higher-level whenever an error occurs.
pub trait Processor<T> {
    type Output;

    type Error;

    /// Consumes an item for further processing.
    ///
    /// Might apply back-pressure when being too busy.
    fn process(&self, input: T) -> impl Future<Output = Result<(), Self::Error>>;

    /// Returns future with processed output whenever ready.
    ///
    /// Please note that processors never terminate and rather return pending state (Poll::Pending)
    /// in the future when `next` is called and no ready items are available.
    fn next(&self) -> impl Future<Output = Result<Self::Output, Self::Error>>;
}

// SPDX-License-Identifier: MIT OR Apache-2.0

use tokio::sync::broadcast;

pub type EventsReceiver = broadcast::Receiver<NetworkEvent>;

pub type EventsSender = broadcast::Sender<NetworkEvent>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkEvent {}

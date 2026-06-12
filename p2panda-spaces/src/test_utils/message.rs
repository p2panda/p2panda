// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::message::SpacesMessage;
use crate::test_utils::{TestConditions, TestSpaceId};

pub type SeqNum = u64;

pub type TestMessage = SpacesMessage<TestSpaceId, TestConditions>;

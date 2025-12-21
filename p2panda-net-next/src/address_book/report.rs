// SPDX-License-Identifier: MIT OR Apache-2.0

#[derive(Debug)]
pub enum ConnectionOutcome {
    Successful,
    Failed,
}

impl ConnectionOutcome {
    pub fn is_failed(&self) -> bool {
        match self {
            ConnectionOutcome::Successful => false,
            ConnectionOutcome::Failed => true,
        }
    }
}

#[derive(Debug)]
pub enum ConnectionRole {
    Connect {
        #[allow(unused)]
        remote_address: iroh::EndpointAddr,
    },
    Accept,
}

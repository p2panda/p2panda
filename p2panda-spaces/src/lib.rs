use std::marker::PhantomData;

/// Create and manage spaces and groups.
///
/// Takes care of ingesting operations, updating spaces, groups and member key-material. Has
/// access to the operation and group stores, orderer, key-registry and key-manager.
///
/// Routes operations to the correct space(s), group(s) or member.
///
/// Only one instance of `Spaces` per app user.
///
/// Operations are created and published within the spaces service, reacting to arriving
/// operations, due to api calls (create group, create space), or triggered by key-bundles
/// expiring.
///
/// Users of spaces can subscribe to events which inform about member, group or space state
/// changes, application data being decrypted, pre-key bundles being published, we were added or
/// removed from a space.
///
/// Is agnostic to current p2panda-streams, networking layer, data type?
pub struct Manager<F, M> {
    forge: F,
    _phantom: PhantomData<M>
}

impl<F, M> Manager<F, M>
where
    F: Forge<M>,
{
    pub fn space(&self) -> Space<F, M> {
        todo!()
    }

    pub fn create_space(&mut self) -> Space<F, M> {
        todo!()
    }

    pub fn group(&self) -> Group {
        todo!()
    }

    pub fn create_group(&mut self) -> Group {
        todo!()
    }

    pub fn receive(&mut self, message: &M) -> Vec<Event<F, M>> {
        todo!()
    }
}

enum Message {
    App,
    Auth,
    Encryption,
}

/// Encrypted data context with authorization boundary.
///
/// Only members with suitable access to the space can read and write to it.
pub struct Space<F, M> {
    manager: Manager<F, M>,
}

impl<F, M> Space<F, M> {
    pub fn publish(bytes: &[u8]) {
        todo!()
    }
}

pub struct Group {}

trait Forge<M> {
    type Error;

    fn forge(&self, args: ForgeArgs) -> Result<M, Self::Error>;
}

struct ForgeArgs {}

pub enum Event<F, M> {
    JoinedSpace(Space<F, M>),
    JoinedGroup(Group),
    Message(M)
}

// Sketch for "nested" approach to generic dependency definitions using trait.
//
// trait Dependencies<ID> {
//     fn get() -> Vec<ID>;
// }
//
// struct Operation {
//     header: ControlMessage,
//     body: AppMessage,
// }
//
// struct ControlMessage {
//     auth_message: (),
//     encryption_message: (),
// }
//
// struct AuthMessage {}
// struct EncryptionMessage {}
// struct AppMessage {}
//
// impl<ID> Dependencies<ID> for AppMessage {
//     fn get() -> Vec<ID> {
//         todo!()
//     }
// }
//
// impl<ID> Dependencies<ID> for AuthMessage {
//     fn get() -> Vec<ID> {
//         todo!()
//     }
// }
//
// impl<ID> Dependencies<ID> for EncryptionMessage {
//     fn get() -> Vec<ID> {
//         todo!()
//     }
// }
//
// impl<ID> Dependencies<ID> for Operation {
//     fn get() -> Vec<ID> {
//         let mut dependencies = Vec::new();
//
//         // merge all sub-dependencies both for header extensions and body
//
//         dependencies
//     }
// }

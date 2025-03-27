// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt::Debug;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use crate::crypto::x25519::PublicKey;
use crate::traits::{IdentityHandle, IdentityRegistry, KeyBundle, PreKeyRegistry};
use crate::{LongTermKeyBundle, OneTimeKeyBundle};

pub struct KeyRegistry<ID> {
    _marker: PhantomData<ID>,
}

/// Serializable state of key manager (for persistance).
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(Clone))]
pub struct KeyRegistryState<ID>
where
    ID: IdentityHandle,
{
    identities: HashMap<ID, PublicKey>,
    onetime_bundles: HashMap<ID, Vec<OneTimeKeyBundle>>,
    longterm_bundles: HashMap<ID, Vec<LongTermKeyBundle>>,
}

impl<ID> KeyRegistry<ID>
where
    ID: IdentityHandle + Serialize + for<'a> Deserialize<'a>,
{
    pub fn init() -> KeyRegistryState<ID> {
        KeyRegistryState {
            identities: HashMap::new(),
            onetime_bundles: HashMap::new(),
            longterm_bundles: HashMap::new(),
        }
    }

    pub fn register_longterm(
        mut y: KeyRegistryState<ID>,
        id: ID,
        key_bundle: LongTermKeyBundle,
    ) -> KeyRegistryState<ID> {
        let existing = y.identities.insert(id, *key_bundle.identity_key());
        if let Some(existing) = existing {
            // Sanity check.
            assert_eq!(&existing, key_bundle.identity_key());
        }
        y.longterm_bundles
            .entry(id)
            .and_modify(|bundles| bundles.push(key_bundle.clone()))
            .or_insert(vec![key_bundle]);
        y
    }

    pub fn register_onetime(
        mut y: KeyRegistryState<ID>,
        id: ID,
        key_bundle: OneTimeKeyBundle,
    ) -> KeyRegistryState<ID> {
        let existing = y.identities.insert(id, *key_bundle.identity_key());
        if let Some(existing) = existing {
            // Sanity check.
            assert_eq!(&existing, key_bundle.identity_key());
        }
        y.onetime_bundles
            .entry(id)
            .and_modify(|bundles| bundles.push(key_bundle.clone()))
            .or_insert(vec![key_bundle]);
        y
    }
}

impl<ID> PreKeyRegistry<ID, OneTimeKeyBundle> for KeyRegistry<ID>
where
    ID: IdentityHandle + Serialize + for<'a> Deserialize<'a>,
{
    type State = KeyRegistryState<ID>;

    type Error = Infallible;

    fn key_bundle(
        mut y: Self::State,
        id: &ID,
    ) -> Result<(Self::State, Option<OneTimeKeyBundle>), Self::Error> {
        let bundle = y
            .onetime_bundles
            .get_mut(id)
            .and_then(|bundles| bundles.pop());
        Ok((y, bundle))
    }
}

impl<ID> PreKeyRegistry<ID, LongTermKeyBundle> for KeyRegistry<ID>
where
    ID: IdentityHandle + Serialize + for<'a> Deserialize<'a>,
{
    type State = KeyRegistryState<ID>;

    type Error = Infallible;

    fn key_bundle(
        mut y: Self::State,
        id: &ID,
    ) -> Result<(Self::State, Option<LongTermKeyBundle>), Self::Error> {
        let bundle = y
            .longterm_bundles
            .get_mut(id)
            .and_then(|bundles| bundles.pop());
        Ok((y, bundle))
    }
}

impl<ID> IdentityRegistry<ID, KeyRegistryState<ID>> for KeyRegistry<ID>
where
    ID: IdentityHandle + Serialize + for<'a> Deserialize<'a>,
{
    type Error = Infallible;

    fn identity_key(y: &KeyRegistryState<ID>, id: &ID) -> Result<Option<PublicKey>, Self::Error> {
        let key = y.identities.get(id).cloned();
        Ok(key)
    }
}

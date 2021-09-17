// SPDX-License-Identifier: AGPL-3.0-or-later

import debug from 'debug';

import { Session } from '~/index';
import { EntryRecord, Fields, FieldsTagged, InstanceRecord } from '~/types';
import { marshallRequestFields } from '~/utils';

import { P2Panda } from '~/wasm';
import { KeyPair, MessageFields } from 'wasm-web';

export type Context = {
  keyPair: KeyPair;
  schema: string;
  session: Session;
};

const log = debug('p2panda-js:entry');

/**
 * Sign and publish an entry given a prepared `Message`, `KeyPair` and
 * `Session`.
 */
const signPublishEntry = async (
  messageEncoded: string,
  { keyPair, schema, session }: Context,
) => {
  const { signEncodeEntry } = (await session.loadWasm()) as P2Panda;

  const entryArgs = await session.getNextEntryArgs(keyPair.publicKey(), schema);

  const { entryEncoded } = signEncodeEntry(
    keyPair,
    messageEncoded,
    entryArgs.entryHashSkiplink,
    entryArgs.entryHashBacklink,
    entryArgs.seqNum,
    entryArgs.logId,
  );

  const nextEntryArgs = await session.publishEntry(
    entryEncoded,
    messageEncoded,
  );

  // Cache next entry args for next publish
  session.setNextEntryArgs(keyPair.publicKey(), schema, nextEntryArgs);
};

// SPDX-License-Identifier: AGPL-3.0-or-later

import debug from 'debug';

import wasm from '~/wasm';
import { Context } from '~/session';

const log = debug('p2panda-js:entry');
/**
 * Sign and publish an entry given a prepared `Message`, `KeyPair` and
 * `Session`.
 *
 * Sets next entry args on the supplied session's entry args cache.
 *
 * Returns the encoded entry.
 */
export const signPublishEntry = async (
  messageEncoded: string,
  { keyPair, schema, session }: Context,
): Promise<string> => {
  const { signEncodeEntry } = await wasm;

  log('Signing and publishing entry');

  const entryArgs = await session.getNextEntryArgs(keyPair.publicKey(), schema);

  log('Retrieved next entry args for', {
    keyPair: keyPair.publicKey(),
    schema,
    entryArgs,
  });

  const { entryEncoded } = signEncodeEntry(
    keyPair,
    messageEncoded,
    entryArgs.entryHashSkiplink,
    entryArgs.entryHashBacklink,
    entryArgs.seqNum,
    entryArgs.logId,
  );
  log('Signed and encoded entry');

  const nextEntryArgs = await session.publishEntry(
    entryEncoded,
    messageEncoded,
  );
  log('Published entry');

  // Cache next entry args for next publish
  session.setNextEntryArgs(keyPair.publicKey(), schema, nextEntryArgs);
  log('Cached next entry args');

  return entryEncoded;
};

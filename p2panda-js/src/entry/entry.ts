// SPDX-License-Identifier: AGPL-3.0-or-later

import debug from 'debug';

import wasm from '~/wasm';
import { Context } from '~/session';

const log = debug('p2panda-js:entry');

/**
 * Sign and publish an entry given a prepared `Operation`, `KeyPair` and
 * `Session`.
 *
 * Sets next entry args on the supplied session's entry args cache.
 *
 * Returns the encoded entry.
 */
export const signPublishEntry = async (
  operationEncoded: string,
  { keyPair, session }: Context,
  documentId?: string,
): Promise<string> => {
  const { signEncodeEntry } = await wasm;
  const publicKey = keyPair.publicKey();

  log('Signing and publishing entry');
  const nextArgs = await session.getNextArgs(publicKey, documentId);

  log('Retrieved next args for', {
    publicKey,
    documentId,
    nextArgs,
  });

  const { entryEncoded, entryHash } = signEncodeEntry(
    keyPair,
    operationEncoded,
    nextArgs.skiplink,
    nextArgs.backlink,
    BigInt(nextArgs.seqNum),
    BigInt(nextArgs.logId),
  );
  log('Signed and encoded entry');

  const publishNextArgs = await session.publish(entryEncoded, operationEncoded);
  log('Published entry');

  // Cache next entry args for next publish. Use the entry hash as the document
  // id for CREATE operations.
  session.setNextArgs(publicKey, documentId || entryHash, publishNextArgs);
  log('Cached next arguments');

  return entryEncoded;
};

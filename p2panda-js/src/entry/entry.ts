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

  log('Signing and publishing entry');

  const entryArgs = await session.getNextEntryArgs(
    keyPair.publicKey(),
    documentId,
  );

  log('Retrieved next entry args for', {
    keyPair: keyPair.publicKey(),
    documentId,
    entryArgs,
  });

  const { entryEncoded, entryHash } = signEncodeEntry(
    keyPair,
    operationEncoded,
    entryArgs.entryHashSkiplink,
    entryArgs.entryHashBacklink,
    BigInt(entryArgs.seqNum),
    BigInt(entryArgs.logId),
  );
  log('Signed and encoded entry');

  const nextEntryArgs = await session.publishEntry(
    entryEncoded,
    operationEncoded,
  );
  log('Published entry');

  // Cache next entry args for next publish. Use the entry hash as the document
  // id for CREATE operations.
  session.setNextEntryArgs(
    keyPair.publicKey(),
    documentId || entryHash,
    nextEntryArgs,
  );
  log('Cached next entry args');

  return entryEncoded;
};

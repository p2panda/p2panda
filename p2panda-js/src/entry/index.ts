// SPDX-License-Identifier: AGPL-3.0-or-later

import { Session } from '~/index';

import { P2Panda } from '~/wasm';
import { KeyPair } from 'wasm-web';

export type Context = {
  keyPair: KeyPair;
  schema: string;
  session: Session;
};

/**
 * Sign and publish an entry given a prepared `Message`, `KeyPair` and
 * `Session`.
 */
export const signPublishEntry = async (
  messageEncoded: string,
  { keyPair, schema, session }: Context,
): Promise<void> => {
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

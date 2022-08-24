// SPDX-License-Identifier: AGPL-3.0-or-later

import * as wasm from '../wasm';
import { KeyPair } from '../identity';
import { toBigInt } from '../utils';
import { validate } from '../validate';

/**
 * Arguments to create an Bamboo entry.
 */
export type EntryArgs = {
  /** Log id of entry, starting at 0 */
  logId?: bigint | number | string;

  /** Sequence number of entry, starting at 1 */
  seqNum?: bigint | number | string;

  /** Skiplink hash */
  skiplink?: string;

  /** Backlink hash, omitted when first entry in log */
  backlink?: string;

  /** Operation payload this entry points at */
  operation: string;
};

/**
 * Signs and encodes an Bamboo entry for the given payload and key pair.
 * @param {EntryArgs} entry - Arguments to create the entry
 * @param {KeyPair} keyPair - Key pair to sign the entry with
 * @returns Hexadecimal encoded entry
 * @example
 * ```
 * import { KeyPair, signAndEncodeEntry, encodeOperation } from 'p2panda-js';
 *
 * const keyPair = new KeyPair();
 *
 * const operation = encodeOperation({
 *   schemaId: 'venues_0020c9db3376fa753b041e199ebfe1c0e6dfb50ca7924c7eedfdd35f141ac8d1207c',
 *   fields: {
 *     name: 'Klangkeller',
 *   },
 * });
 *
 * const entry = signAndEncodeEntry({
 *   logId: 2,
 *   operation,
 * }, keyPair);
 * ```
 */
export function signAndEncodeEntry(entry: EntryArgs, keyPair: KeyPair): string {
  validate(
    { entry, keyPair },
    {
      entry: { type: 'object' },
      keyPair: { type: 'object' },
    },
  );

  const { skiplink = undefined, backlink = undefined, operation } = entry;

  // Convert arguments always to BigInt, set defaults if undefined
  const logId = toBigInt(entry.logId, BigInt(0));
  const seqNum = toBigInt(entry.seqNum, BigInt(1));

  validate(
    {
      logId,
      seqNum,
      skiplink,
      backlink,
      operation,
    },
    {
      logId: {
        type: 'bigint',
        min: 0,
      },
      seqNum: {
        type: 'bigint',
        min: 1,
      },
      skiplink: {
        length: 68,
        optional: true,
        validHex: true,
      },
      backlink: {
        length: 68,
        optional: true,
        validHex: true,
      },
      operation: {
        validHex: true,
      },
    },
  );

  try {
    return wasm.signAndEncodeEntry(
      logId,
      seqNum,
      skiplink,
      backlink,
      operation,
      keyPair.__internal,
    );
  } catch (error) {
    throw new Error(
      `Could not sign and encode entry: ${(error as Error).message}`,
    );
  }
}

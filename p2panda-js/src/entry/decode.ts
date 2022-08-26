// SPDX-License-Identifier: AGPL-3.0-or-later

import * as wasm from '../wasm';
import { validate } from '../validate';

/**
 * Signed Bamboo entry.
 */
export type Entry = {
  /** Public key of the entry author */
  publicKey: string;

  /** Log id of entry, starting at 0 */
  logId?: bigint;

  /** Sequence number of entry, starting at 1 */
  seqNum?: bigint;

  /** Skiplink hash */
  skiplink?: string;

  /** Backlink hash, omitted when first entry in log */
  backlink?: string;

  /** Size of the payload in bytes */
  payloadSize: bigint;

  /** Hash of the payload */
  payloadHash: string;

  /** Ed25519 signature */
  signature: string;
};

/**
 * Decodes an signed Bamboo entry.
 * @param {string} encodedEntry - Hexadecimal string of an encoded entry
 * @returns {Entry} Bamboo Entry
 * @example
 * ```
 * import { decodeEntry } from 'p2panda-js';
 *
 * const encodedEntry =
 *   '00' +
 *   'b443811e711fdbfcbeccaf655e0ebe7a1c83490cc28d1516c5920178bef416c5' +
 *   '0f' +
 *   '08' +
 *   '002034441bd15ac6c01ba5bb9f22b9a6d51d56e280cb3abcdb65216d45ddff74ae4b' +
 *   '0020d5c17b82ad475e2c9ec2d77b08737f7db967cd5f7d481bb4e15443a7d03b5327' +
 *   '03' +
 *   '00203ec19c37eaa225b9b811d0c30aa3f6994b05c78f630249d574c8824f191001d6' +
 *   '36ea3d6f735e388e4c257a3689030a28f60958c8bdb29e4039ed0bb0a3ede4c0' +
 *   'd3aed2095b1eb9a37ef065f20a2df90af0583da6081339a2689bc734dff0da04';
 *
 * const result = decodeEntry(encodedEntry);
 * console.log(result.publicKey); // "b443811e..."
 * ```
 */
export function decodeEntry(encodedEntry: string): Entry {
  validate(
    {
      encodedEntry,
    },
    {
      encodedEntry: {
        validHex: true,
      },
    },
  );

  try {
    return wasm.decodeEntry(encodedEntry);
  } catch (error) {
    throw new Error(`Could not decode entry: ${(error as Error).message}`);
  }
}

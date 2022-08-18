// SPDX-License-Identifier: AGPL-3.0-or-later

import wasm from '../wasm';
import { validate } from '../validate';

/**
 * Signed Bamboo entry.
 */
type Entry = {
  // Public key of the entry author
  publicKey: string;

  // Log id of entry, starting at 0
  logId?: bigint;

  // Sequence number of entry, starting at 1
  seqNum?: bigint;

  // Skiplink hash
  skiplink?: string;

  // Backlink hash, omitted when first entry in log
  backlink?: string;

  // Size of the payload in bytes
  payloadSize: bigint;

  // Hash of the payload
  payloadHash: string;

  // Ed25519 signature
  signature: string;
};

/**
 * Decodes an signed Bamboo entry.
 * @param {string} encodedEntry - Hexadecimal string of an encoded entry
 * @returns {Entry} Bamboo Entry
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

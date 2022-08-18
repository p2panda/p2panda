// SPDX-License-Identifier: AGPL-3.0-or-later

import * as wasm from '../wasm';
import { validate } from '../validate';

/**
 * Generates a hash (BLAKE3 wrapped in YASMF container) from any value.
 * @param {string} value - Data encoded as hexadecimal string
 * @returns {string} Generated hash, encoded as hexadecimal string
 */
export function generateHash(value: string): string {
  validate(
    {
      value,
    },
    {
      value: {
        validHex: true,
      },
    },
  );

  return wasm.generateHash(value);
}

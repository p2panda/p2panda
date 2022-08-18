// SPDX-License-Identifier: AGPL-3.0-or-later

import { validate } from '../validate';

/**
 * Converts byte sequence to hexadecimal string.
 * @param {Uint8Array} bytes - Any byte sequence
 * @returns {string} Hexadecimal encoded string
 */
export function bytesToHex(bytes: Uint8Array): string {
  const hex = [];

  for (let i = 0; i < bytes.length; i++) {
    const current = bytes[i] < 0 ? bytes[i] + 256 : bytes[i];
    hex.push((current >>> 4).toString(16));
    hex.push((current & 0xf).toString(16));
  }

  return hex.join('');
}

/**
 * Converts any hexadecimal string to byte sequence.
 * @param {string} hex - Hexadecimal encoded string
 * @returns {Uint8Array} Byte sequence
 */
export function hexToBytes(hex: string): Uint8Array {
  validate(
    {
      hex,
    },
    {
      hex: {
        validHex: true,
      },
    },
  );

  const bytes = [];

  for (let c = 0; c < hex.length; c += 2) {
    bytes.push(parseInt(hex.substring(c, c + 2), 16));
  }

  return new Uint8Array(bytes);
}

// SPDX-License-Identifier: AGPL-3.0-or-later

import * as wasm from '../wasm';
import { validate } from '../validate';

/**
 * Ed25519 key pair to sign Bamboo entries with.
 * @example
 * ```
 * import { KeyPair } from 'p2panda-js';
 *
 * const keyPair = new KeyPair();
 * console.log(keyPair.publicKey());
 * ```
 */
export class KeyPair {
  /**
   * @internal
   */
  readonly __internal: wasm.KeyPair;

  /**
   * Creates a new instance of `KeyPair`.
   *
   * When no `privateKey` value is passed, the constructor generates a new
   * Ed25519 key pair using the systems random number generator (CSPRNG) as a
   * seed.
   *
   * Warning: Depending on the context this does not guarantee the random
   * number generator to be cryptographically secure (eg. broken / hijacked
   * browser or system implementations), so make sure to only run this in
   * trusted environments.
   *
   * @param {string?} privateKey - Hexadecimal encoded private key. Warning:
   * Absolutely no validation is done on the key. If you give this function
   * bytes which do not represent a valid point, or which do not represent
   * corresponding parts of the key, then your KeyPair will be broken and it
   * will be your fault.
   * @returns KeyPair instance
   * @example
   * ```
   * import { KeyPair } from 'p2panda-js';
   *
   * const keyPair = new KeyPair('1f9e81007da0c007314a151be11be392de4cdc76888fbc5a8c62aa03c6730c6a');
   * ```
   */
  constructor(privateKey?: string) {
    validate(
      {
        privateKey,
      },
      {
        privateKey: {
          optional: true,
          validHex: true,
          length: 64,
        },
      },
    );

    let keyPair: wasm.KeyPair;
    if (privateKey) {
      // Recover key pair from private key when given
      try {
        keyPair = wasm.KeyPair.fromPrivateKey(privateKey);
      } catch (error) {
        throw new Error(
          `Could not recreate key pair from private key: ${
            (error as Error).message
          }`,
        );
      }
    } else {
      // Generate new key pair otherwise
      try {
        keyPair = new wasm.KeyPair();
      } catch (error) {
        throw new Error(
          `Could not generate new key pair: ${(error as Error).message}`,
        );
      }
    }

    this.__internal = keyPair;
  }

  /**
   * Returns public key as a hexadecimal string.
   * @returns {string} Hexadecimal encoded public key
   */
  publicKey(): string {
    return this.__internal.publicKey();
  }

  /**
   * Returns private key as a hexadecimal string.
   * @returns {string} Hexadecimal encoded private key.
   */
  privateKey(): string {
    return this.__internal.privateKey();
  }

  /**
   * Signs any data using this key pair and returns signature.
   * @param {string?} bytes - Any byte sequence encoded as a hexadecimal string
   * @returns {string} Hexadecimal encoded signature
   */
  sign(bytes: string): string {
    validate(
      {
        bytes,
      },
      {
        bytes: {
          validHex: true,
        },
      },
    );

    return this.__internal.sign(bytes);
  }
}

/**
 * Returns true if signed data could be verified against a public key.
 * @param {string} publicKey - Ed25519 public key string
 * @param {string} bytes - Any byte sequence encoded as a hexadecimal string
 * @param {string} signature - Ed25519 signature string
 * @returns {boolean} True if claimed signature is correct
 * @example
 * ```
 * import { KeyPair, verifySignature } from 'p2panda-js';
 *
 * const keyPair = new KeyPair();
 * const signature = keyPair.sign('aabbcc');
 * verifySignature(keyPair.publicKey(), 'aabbcc', signature); // true
 * ```
 */
export function verifySignature(
  publicKey: string,
  bytes: string,
  signature: string,
): boolean {
  validate(
    {
      publicKey,
      bytes,
      signature,
    },
    {
      publicKey: {
        validHex: true,
        length: 64,
      },
      bytes: {
        validHex: true,
      },
      signature: {
        validHex: true,
        length: 128,
      },
    },
  );

  return wasm.verifySignature(publicKey, bytes, signature);
}

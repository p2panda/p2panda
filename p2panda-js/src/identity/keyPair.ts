// SPDX-License-Identifier: AGPL-3.0-or-later

import { KeyPair } from '~/wasm';

/**
 * Generates a new key pair.
 *
 * @returns KeyPair instance
 */
export function createKeyPair() {
  const keyPair = new KeyPair();
  return keyPair;
}

/**
 * Load a p2panda key pair from its private key.
 *
 * @param privateKey string representation of a private key
 * @returns KeyPair instance
 */
export function recoverKeyPair(privateKey: string) {
  const keyPair = KeyPair.fromPrivateKey(privateKey);
  return keyPair;
}

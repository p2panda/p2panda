// SPDX-License-Identifier: AGPL-3.0-or-later

import wasm from '~/wasm';

/**
 * Returns a new p2panda key pair.
 */
// Remove this eslint rule once we have proper TypeScript types
// eslint-disable-next-line @typescript-eslint/explicit-module-boundary-types
export const createKeyPair = async () => {
  const { KeyPair } = await wasm;
  const keyPair = new KeyPair();
  return keyPair;
};

/**
 * Load a p2panda key pair from its private key.
 *
 * @param privateKey string representation of a private key
 * @returns KeyPair instance
 */
// Remove this eslint rule once we have proper TypeScript types
// eslint-disable-next-line @typescript-eslint/explicit-module-boundary-types
export const recoverKeyPair = async (privateKey: string) => {
  const { KeyPair } = await wasm;
  const keyPair = KeyPair.fromPrivateKey(privateKey);
  return keyPair;
};

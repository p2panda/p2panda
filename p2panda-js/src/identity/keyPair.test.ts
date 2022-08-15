// SPDX-License-Identifier: AGPL-3.0-or-later

import { createKeyPair, recoverKeyPair } from './';

describe('Key pair utils', () => {
  it('createKeyPair creates a key pair', () => {
    const keyPair = createKeyPair();
    expect(keyPair.privateKey()).toHaveLength(64);
  });

  it('recoverKeyPair recovers a key pair', () => {
    const keyPair = createKeyPair();
    const keyPair2 = recoverKeyPair(keyPair.privateKey());
    expect(keyPair.publicKey()).toBe(keyPair2.publicKey());
  });

  it('recoverKeyPair throws when recovering an invalid key pair', () => {
    expect(() => {
      recoverKeyPair('invalid');
    }).toThrow();
  });
});

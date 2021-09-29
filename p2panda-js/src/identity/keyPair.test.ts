// SPDX-License-Identifier: AGPL-3.0-or-later

import { createKeyPair, recoverKeyPair } from '~/identity';

describe('Key pair utils', () => {
  it('createKeyPair creates a key pair', async () => {
    const keyPair = await createKeyPair();
    expect(keyPair.privateKey()).toHaveLength(64);
  });

  it('recoverKeyPair recovers a key pair', async () => {
    const keyPair = await createKeyPair();
    const keyPair2 = await recoverKeyPair(keyPair.privateKey());
    expect(keyPair.publicKey()).toBe(keyPair2.publicKey());
  });

  it('recoverKeyPair throws when recovering an invalid key pair', async () => {
    await expect(recoverKeyPair('invalid')).rejects.toThrow();
  });
});

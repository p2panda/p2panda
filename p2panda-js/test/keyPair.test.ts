// SPDX-License-Identifier: AGPL-3.0-or-later

import chai, { expect, assert } from 'chai';
import chaiAsPromised from 'chai-as-promised';

// @ts-expect-error bundle import has no type
import { createKeyPair, recoverKeyPair } from '../lib';

chai.use(chaiAsPromised);

describe('Key pair utils', () => {
  it('createKeyPair creates a key pair', async () => {
    const keyPair = await createKeyPair();
    expect(keyPair.privateKey()).to.have.length(64);
  });

  it('recoverKeyPair recovers a key pair', async () => {
    const keyPair = await createKeyPair();
    const keyPair2 = await recoverKeyPair(keyPair.privateKey());
    expect(keyPair.publicKey()).to.equal(keyPair2.publicKey());
  });

  it('recoverKeyPair throws when recovering an invalid key pair', async () => {
    assert.isRejected(recoverKeyPair('invalid'));
  });
});

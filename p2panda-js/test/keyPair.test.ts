import chai, { expect, assert } from 'chai';
import chaiAsPromised from 'chai-as-promised';

import { createKeyPair, recoverKeyPair } from '../lib';

chai.use(chaiAsPromised);

describe('key pair utils', () => {
  it('creates a key pair', async () => {
    const keyPair = await createKeyPair();
    expect(keyPair.privateKey()).to.have.length(64);
  });

  it('recovers a key pair', async () => {
    const keyPair = await createKeyPair();
    const keyPair2 = await recoverKeyPair(keyPair.privateKey());
    expect(keyPair.publicKey()).to.equal(keyPair2.publicKey());
  });

  it('throws when recovering an invalid key pair', async () => {
    assert.isRejected(recoverKeyPair('invalid'));
  });
});

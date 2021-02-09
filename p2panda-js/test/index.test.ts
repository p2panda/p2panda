import { expect } from 'chai';

import p2panda from '../lib';

describe('KeyPair', () => {
  it('creates a key pair', async () => {
    const { KeyPair } = await p2panda;
    const keyPair = new KeyPair();
    expect(keyPair.privateKey().length).to.eq(64);
  });

  it('restores a key pair', async () => {
    const { KeyPair } = await p2panda;
    const keyPair = new KeyPair();
    const keyPairSecond = KeyPair.fromPrivateKey(keyPair.privateKey());
    expect(keyPair.publicKey()).to.eq(keyPairSecond.publicKey());
  });
});

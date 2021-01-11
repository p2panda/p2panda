import { expect } from 'chai';

import p2panda from '../lib';

describe('KeyPair', () => {
  it('creates a key pair', async () => {
    const { KeyPair } = await p2panda;
    const kp = new KeyPair();
    expect(kp.privateKey().length).to.eq(64);
  });
});

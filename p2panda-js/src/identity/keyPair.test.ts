// SPDX-License-Identifier: AGPL-3.0-or-later

import { KeyPair, verifySignature } from './';

describe('KeyPair', () => {
  it('generates a new key pair', () => {
    const keyPair = new KeyPair();
    expect(keyPair.privateKey()).toHaveLength(64);
  });

  it('returns public key string', () => {
    const keyPair = new KeyPair();
    expect(keyPair.publicKey()).toHaveLength(64);
  });

  it('returns private key string', () => {
    const keyPair = new KeyPair();
    expect(keyPair.privateKey()).toHaveLength(64);
  });

  it('recovers a key pair from private key string', () => {
    const keyPair = new KeyPair();
    const keyPair2 = new KeyPair(keyPair.privateKey());
    expect(keyPair.publicKey()).toBe(keyPair2.publicKey());
  });

  it('throws when recovering from an invalid private key', () => {
    expect(() => {
      new KeyPair('invalid');
    }).toThrow();

    expect(() => {
      new KeyPair('002233');
    }).toThrow();
  });

  it('signs any sort of data', () => {
    expect(() => {
      new KeyPair('invalid');
    }).toThrow();
  });

  it('throws when trying to sign invalid data string', () => {
    expect(() => {
      const keyPair = new KeyPair();
      keyPair.sign('112');
    }).toThrow();
  });
});

describe('verifySignature', () => {
  it('checks the signature', () => {
    const keyPair = new KeyPair();
    const data = '1345';
    const signature = keyPair.sign(data);

    const result = verifySignature(keyPair.publicKey(), data, signature);
    expect(result).toBe(true);

    // Wrong public key
    const keyPair2 = new KeyPair();
    const wrong1 = verifySignature(keyPair2.publicKey(), data, signature);
    expect(wrong1).toBe(false);

    // Wrong signature
    const signature2 = keyPair2.sign(data);
    const wrong2 = verifySignature(keyPair.publicKey(), data, signature2);
    expect(wrong2).toBe(false);

    // Wrong data
    const wrong3 = verifySignature(keyPair.publicKey(), '6789', signature);
    expect(wrong3).toBe(false);
  });

  it('throws when giving wrong parameters', () => {
    expect(() => {
      verifySignature('02', '03', '01');
    }).toThrow();
  });
});

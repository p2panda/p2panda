import { expect } from 'chai';

describe('KeyPair', () => {
  it('creates a key pair', (done) => {
    import('wasm').then(({ KeyPair }) => {
      console.log('imported');
      const kp = new KeyPair();
      expect(kp.privateKey().length).to.eq(32);
      console.log(kp.privateKey());
      done();
    });
  });
});

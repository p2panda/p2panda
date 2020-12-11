import { expect } from 'chai';

describe('KeyPair', () => {
  it('creates a key pair', (done) => {
    import('../wasm')
      .then(({ KeyPair }) => {
        const kp = new KeyPair();
        expect(kp.privateKey().length).to.eq(64);
        done();
      })
      .catch((err) => {
        console.error(err);
        throw err;
      });
  });
});

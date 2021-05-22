import { expect } from 'chai';

import { Session } from '../lib';

// const PRIVATE_KEY =
//   '4c21b14046f284f87f1ea4be4b973664221ad483079a68ed35a6812553b41176';

describe('Session', () => {
  it('can query entries', async () => {
    const session = new Session('http://localhost:2020');
    const entries = await session.queryEntries(
      '00401d76566758a5b6bfc561f1c936d8fc86b5b42ea22ab1dabf40d249d27dd906401fde147e53f44c103dd02a254916be113e51de1077a946a3a0c1272b9b348437',
    );
    expect(entries.length).to.equal(2);
  });

  it('throws when querying without a schema');

  it('can publish entries');

  it('caches next entry args');

  it('throws when publishing without a key pair');

  it('throws without a configured endpoint');
});

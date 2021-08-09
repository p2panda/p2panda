import { expect } from 'chai';

import { Session } from '../lib';

// const PRIVATE_KEY =
//   '4c21b14046f284f87f1ea4be4b973664221ad483079a68ed35a6812553b41176';

const PUBLIC_KEY =
  '2deee1fac104aaac753377bbc2ff70fa5d5154abdac4b4577392db88de6a9a49';

const SCHEMA =
  '00401d76566758a5b6bfc561f1c936d8fc86b5b42ea22ab1dabf40d249d27dd906401fde147e53f44c103dd02a254916be113e51de1077a946a3a0c1272b9b348437';

describe('Session', () => {
  it('can query entries', async () => {
    const session = new Session('http://localhost:2020');
    const entries = await session.queryEntries(SCHEMA);
    console.log(entries);
    expect(entries.length).to.equal(2);
  });

  it('throws when querying without a schema', async () => {
    const session = new Session('http://localhost:2020');
    let error;
    try {
      await session.queryEntries();
    } catch (e) {
      error = e;
    }
    expect(error.message).to.equal('Schema must be provided');
  });

  it('gets next entry args', async () => {
    const session = new Session('http://localhost:2020');
    const nextEntryArgs = await session.getNextEntryArgs(PUBLIC_KEY, SCHEMA);
    expect(nextEntryArgs.entryHashSkiplink).to.equal('SKIPLINK_HASH');
    expect(nextEntryArgs.entryHashBacklink).to.equal('BACKLINK_HASH');
    expect(nextEntryArgs.seqNum).to.equal(3);
    expect(nextEntryArgs.logId).to.equal(1);
  });

  it('can publish entries');

  it('caches next entry args');

  it('throws when publishing without a key pair');

  it('throws without a configured endpoint');
});

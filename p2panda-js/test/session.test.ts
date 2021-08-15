import chai, { assert, expect } from 'chai';
import sinon from 'sinon';
import chaiAsPromised from 'chai-as-promised';

import { Session } from '../lib';
import SAMPLE_VALUES from './sample-values';

const { SCHEMA, PUBLIC_KEY, ENTRY_ENCODED, MESSAGE_ENCODED } = SAMPLE_VALUES;

chai.use(chaiAsPromised);

describe('Session', () => {
  it('can query entries', async () => {
    const session = new Session('http://localhost:2020');
    const entries = await session._queryEntries(SCHEMA);
    expect(entries.length).to.equal(2);
  });

  it('throws when querying without a schema', async () => {
    const session = new Session('http://localhost:2020');
    let error;
    try {
      await session._queryEntries();
    } catch (e) {
      error = e;
    }
    expect(error.message).to.equal('Schema must be provided');
  });

  it('gets next entry args', async () => {
    const session = new Session('http://localhost:2020');
    const nextEntryArgs = await session._getNextEntryArgs(PUBLIC_KEY, SCHEMA);
    expect(nextEntryArgs.entryHashSkiplink).to.equal('SKIPLINK_HASH');
    expect(nextEntryArgs.entryHashBacklink).to.equal('BACKLINK_HASH');
    expect(nextEntryArgs.seqNum).to.equal(3);
    expect(nextEntryArgs.logId).to.equal(1);
  });

  it('can publish entries', async () => {
    const session = new Session('http://localhost:2020');
    const nextEntryArgs = await session._publishEntry(
      ENTRY_ENCODED,
      MESSAGE_ENCODED,
    );
    expect(nextEntryArgs.entryHashSkiplink).to.equal('SKIPLINK_HASH');
  });

  it('caches next entry args', async () => {
    const session = new Session('http://localhost:2020');
    // add a spy to check whether the value is really retrieved from the cache
    // and not requested
    session.client.request = sinon.replace(
      session.client,
      'request',
      sinon.fake(),
    );

    const nextEntryArgs = { test: true };
    session._setNextEntryArgs(PUBLIC_KEY, SCHEMA, nextEntryArgs);

    const cacheResponse = await session._getNextEntryArgs(PUBLIC_KEY, SCHEMA);
    expect(cacheResponse.test).to.equal(nextEntryArgs.test);
    expect(session.client.request.notCalled).to.be.true;
  });

  it('throws when missing parameters', async () => {
    const session = new Session('http://localhost:2020');
    assert.isRejected(session._publishEntry(null, MESSAGE_ENCODED));
    assert.isRejected(session._publishEntry(ENTRY_ENCODED, null));
  });

  it('throws without a configured endpoint', () => {
    const session = new Session();
    assert.isRejected(session._publishEntry(ENTRY_ENCODED, MESSAGE_ENCODED));
  });
});

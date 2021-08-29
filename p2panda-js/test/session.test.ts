import chai, { assert, expect } from 'chai';
import sinon from 'sinon';
import chaiAsPromised from 'chai-as-promised';

// @ts-expect-error bundle import has no type
import { Session, createKeyPair } from '../lib';
import SAMPLE_VALUES from './sample-values';

const {
  BACKLINK_HASH,
  ENTRY_ENCODED,
  MESSAGE_ENCODED,
  PUBLIC_KEY,
  SCHEMA,
} = SAMPLE_VALUES;

chai.use(chaiAsPromised);

/**
 * Test the `Session` class
 *
 * These tests expect the mock rpc server to be running, which can be started
 * with `npm run test:mock-node`.
 */
describe('Session', () => {
  it('requires an endpoint parameter', () => {
    expect(() => {
      new Session();
    }).to.throw('Missing `endpoint` parameter for creating a session');
    expect(() => {
      new Session('');
    }).to.throw('Missing `endpoint` parameter for creating a session');
  });

  it('has a string representation', async () => {
    const session = new Session('http://localhost:2020');
    expect(`${session}`).to.equal('<Session http://localhost:2020>');

    session.setKeyPair(await createKeyPair());
    expect(`${session}`).to.match(
      /<Session http:\/\/localhost:2020 key pair \w{8}>/,
    );

    session.setSchema(SCHEMA);
    expect(`${session}`).to.match(
      /<Session http:\/\/localhost:2020 key pair \w{8} schema \w{8}>/,
    );
  });

  describe('queryEntries', () => {
    it('can query entries', async () => {
      const session = new Session('http://localhost:2020');
      const entries = await session.queryEntries(SCHEMA);
      expect(entries.length).to.equal(1);
    });

    it('throws when querying without a schema', async () => {
      const session = new Session('http://localhost:2020');
      assert.isRejected(session.queryEntries(), 'Schema must be provided');
    });
  });

  describe('publishEntry', () => {
    it('can publish entries', async () => {
      const session = new Session('http://localhost:2020');
      const nextEntryArgs = await session.publishEntry(
        ENTRY_ENCODED,
        MESSAGE_ENCODED,
      );
      expect(nextEntryArgs.entryHashBacklink).to.equal(
        BACKLINK_HASH,
        JSON.stringify(nextEntryArgs, null, 2),
      );
    });

    it('throws when publishing without all required parameters', async () => {
      const session = new Session('http://localhost:2020');
      assert.isRejected(session.publishEntry(null, MESSAGE_ENCODED));
      assert.isRejected(session.publishEntry(ENTRY_ENCODED, null));
    });
  });

  describe('get/setNextEntryArgs', () => {
    it('returns next entry args from node', async () => {
      const session = new Session('http://localhost:2020');
      const nextEntryArgs = await session.getNextEntryArgs(PUBLIC_KEY, SCHEMA);
      expect(nextEntryArgs.entryHashSkiplink).to.equal(null);
      expect(nextEntryArgs.entryHashBacklink).to.equal(BACKLINK_HASH);
      expect(nextEntryArgs.seqNum).to.equal(2);
      expect(nextEntryArgs.logId).to.equal(1);
    });

    it('returns next entry args from cache', async () => {
      const session = new Session('http://localhost:2020');
      // add a spy to check whether the value is really retrieved from the cache
      // and not requested
      session.client.request = sinon.replace(
        session.client,
        'request',
        sinon.fake(),
      );

      const nextEntryArgs = {
        entryHashBacklink: BACKLINK_HASH,
        entryHashSkiplink: undefined,
        logId: 1,
        lastSeqNum: 0,
      };
      session.setNextEntryArgs(PUBLIC_KEY, SCHEMA, nextEntryArgs);

      const cacheResponse = await session.getNextEntryArgs(PUBLIC_KEY, SCHEMA);
      expect(cacheResponse.logId).to.equal(nextEntryArgs.logId);
      expect(session.client.request.notCalled).to.be.true;
    });
  });
});

// SPDX-License-Identifier: AGPL-3.0-or-later

import chai, { assert, expect } from 'chai';
import sinon from 'sinon';
import chaiAsPromised from 'chai-as-promised';

// @ts-expect-error bundle import has no type
import { Session, createKeyPair } from '../lib';

import TEST_DATA from './test-data.json';

const ENTRIES = TEST_DATA.entries;
const PUBLIC_KEY = TEST_DATA.meta.keyPair.publicKey;
const SCHEMA = TEST_DATA.meta.schema;
const ENTRY_ARGS = TEST_DATA.nextEntryArgs;

const NODE_ADDRESS = 'http://localhost:2020';

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
    const session = new Session(NODE_ADDRESS);
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
      const session = new Session(NODE_ADDRESS);
      const entries = await session.queryEntries(SCHEMA);
      expect(entries.length).to.equal(ENTRIES.length);
    });

    it('throws when querying without a schema', async () => {
      const session = new Session(NODE_ADDRESS);
      assert.isRejected(session.queryEntries(), 'Schema must be provided');
    });
  });

  describe('query', () => {
    it('can materialize instances', async () => {
      const session = new Session(NODE_ADDRESS);
      const instances = await session.query({ schema: SCHEMA });
      expect(instances).to.have.lengthOf(1);
      expect(instances[0].description).to.equal('for playing chess');
    });
  });

  describe('publishEntry', () => {
    it('can publish entries', async () => {
      const session = new Session(NODE_ADDRESS);
      const nextEntryArgs = await session.publishEntry(
        ENTRIES[0].entryBytes,
        ENTRIES[0].payloadBytes,
      );
      expect(nextEntryArgs.entryHashBacklink).to.equal(
        ENTRY_ARGS.entryHashBacklink,
        JSON.stringify(
          nextEntryArgs,
          ENTRY_ARGS.entryHashSkiplink,
          ENTRY_ARGS.seqNum,
        ),
      );
    });

    it('throws when publishing without all required parameters', async () => {
      const session = new Session(NODE_ADDRESS);
      assert.isRejected(session.publishEntry(null, ENTRIES[0].payloadBytes));
      assert.isRejected(session.publishEntry(ENTRIES[0].entryBytes, null));
    });
  });

  describe('get/setNextEntryArgs', () => {
    it('returns next entry args from node', async () => {
      const session = new Session(NODE_ADDRESS);
      const nextEntryArgs = await session.getNextEntryArgs(PUBLIC_KEY, SCHEMA);
      expect(nextEntryArgs.entryHashSkiplink).to.equal(
        ENTRY_ARGS.entryHashSkiplink,
      );
      expect(nextEntryArgs.entryHashBacklink).to.equal(
        ENTRY_ARGS.entryHashBacklink,
      );
      expect(nextEntryArgs.seqNum).to.equal(ENTRY_ARGS.seqNum);
      expect(nextEntryArgs.logId).to.equal(ENTRY_ARGS.logId);
    });

    it('returns next entry args from cache', async () => {
      const session = new Session(NODE_ADDRESS);
      // add a spy to check whether the value is really retrieved from the cache
      // and not requested
      session.client.request = sinon.replace(
        session.client,
        'request',
        sinon.fake(),
      );

      const nextEntryArgs = {
        entryHashBacklink: ENTRY_ARGS.entryHashBacklink,
        entryHashSkiplink: ENTRY_ARGS.entryHashSkiplink,
        logId: ENTRY_ARGS.logId,
        lastSeqNum: 0,
      };
      session.setNextEntryArgs(PUBLIC_KEY, SCHEMA, nextEntryArgs);

      const cacheResponse = await session.getNextEntryArgs(PUBLIC_KEY, SCHEMA);
      expect(cacheResponse.logId).to.equal(nextEntryArgs.logId);
      expect(session.client.request.notCalled).to.be.true;
    });
  });
});

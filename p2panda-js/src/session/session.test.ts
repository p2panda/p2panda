// SPDX-License-Identifier: AGPL-3.0-or-later

/* eslint-disable @typescript-eslint/ban-ts-comment */

import { createKeyPair } from '~/identity';
import { Session } from '~/session';

import TEST_DATA from '~/../test/test-data.json';

const ENTRIES = TEST_DATA.entries;
const PUBLIC_KEY = TEST_DATA.meta.keyPair.publicKey;
const SCHEMA = TEST_DATA.meta.schema;
const ENTRY_ARGS = TEST_DATA.nextEntryArgs;

const NODE_ADDRESS = 'http://localhost:2020';

/**
 * Test the `Session` class
 *
 * These tests expect the mock rpc server to be running, which can be started
 * with `npm run test:mock-node`.
 */
describe('Session', () => {
  it('requires an endpoint parameter', () => {
    expect(() => {
      // @ts-ignore: We deliberately use the API wrong here
      new Session();
    }).toThrow('Missing `endpoint` parameter for creating a session');
    expect(() => {
      new Session('');
    }).toThrow('Missing `endpoint` parameter for creating a session');
  });

  it('has a string representation', async () => {
    const session = new Session(NODE_ADDRESS);
    expect(`${session}`).toEqual('<Session http://localhost:2020>');

    session.setKeyPair(await createKeyPair());
    expect(`${session}`).toMatch(
      /<Session http:\/\/localhost:2020 key pair \w{8}>/,
    );

    session.setSchema(SCHEMA);
    expect(`${session}`).toMatch(
      /<Session http:\/\/localhost:2020 key pair \w{8} schema \w{8}>/,
    );
  });

  describe('queryEntries', () => {
    it('can query entries', async () => {
      const session = new Session(NODE_ADDRESS);
      const entries = await session.queryEntries(SCHEMA);
      expect(entries.length).toBe(ENTRIES.length);
    });

    it('throws when querying without a schema', async () => {
      const session = new Session(NODE_ADDRESS);
      // @ts-ignore: We deliberately use the API wrong here
      await expect(session.queryEntries()).rejects.toThrow(
        /Schema must be provided/,
      );
    });
  });

  describe('query', () => {
    it('can materialize instances', async () => {
      const session = new Session(NODE_ADDRESS);
      const instances = await session.query({ schema: SCHEMA });
      expect(instances).toHaveLength(1);
      expect(instances[0].description).toEqual('for playing chess');
    });
  });

  describe('publishEntry', () => {
    it('can publish entries', async () => {
      const session = new Session(NODE_ADDRESS);
      const nextEntryArgs = await session.publishEntry(
        ENTRIES[0].entryBytes,
        ENTRIES[0].payloadBytes,
      );
      expect(nextEntryArgs.entryHashBacklink).toEqual(
        ENTRY_ARGS.entryHashBacklink,
      );
    });

    it('throws when publishing without all required parameters', async () => {
      const session = new Session(NODE_ADDRESS);
      await expect(
        // @ts-ignore: We deliberately use the API wrong here
        session.publishEntry(null, ENTRIES[0].payloadBytes),
      ).rejects.toThrow();
      await expect(
        // @ts-ignore: We deliberately use the API wrong here
        session.publishEntry(ENTRIES[0].entryBytes, null),
      ).rejects.toThrow();
    });
  });

  describe('get/setNextEntryArgs', () => {
    it('returns next entry args from node', async () => {
      const session = new Session(NODE_ADDRESS);
      const nextEntryArgs = await session.getNextEntryArgs(PUBLIC_KEY, SCHEMA);
      expect(nextEntryArgs.entryHashSkiplink).toEqual(
        ENTRY_ARGS.entryHashSkiplink,
      );
      expect(nextEntryArgs.entryHashBacklink).toEqual(
        ENTRY_ARGS.entryHashBacklink,
      );
      expect(nextEntryArgs.seqNum).toEqual(ENTRY_ARGS.seqNum);
      expect(nextEntryArgs.logId).toEqual(ENTRY_ARGS.logId);
    });

    it('returns next entry args from cache', async () => {
      const session = new Session(NODE_ADDRESS);
      // Add a spy to check whether the value is really retrieved from the
      // cache and not requested
      const mockedFn = jest.fn(async () => true);
      session.client.request = mockedFn;

      const nextEntryArgs = {
        entryHashBacklink: ENTRY_ARGS.entryHashBacklink,
        entryHashSkiplink: undefined,
        logId: ENTRY_ARGS.logId,
        seqNum: 1,
      };
      session.setNextEntryArgs(PUBLIC_KEY, SCHEMA, nextEntryArgs);

      const cacheResponse = await session.getNextEntryArgs(PUBLIC_KEY, SCHEMA);
      expect(cacheResponse.logId).toEqual(nextEntryArgs.logId);
      expect(mockedFn.mock.calls.length).toBe(0);
    });
  });
});

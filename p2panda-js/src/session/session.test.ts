// SPDX-License-Identifier: AGPL-3.0-or-later

/* eslint-disable @typescript-eslint/ban-ts-comment */

import { createKeyPair, recoverKeyPair } from '~/identity';
import { Session } from '~/session';
import { Fields } from '~/types';
import {
  authorFixture,
  entryFixture,
  encodedEntryFixture,
  entryArgsFixture,
  schemaFixture,
} from '../../test/fixtures';

const MOCK_SERVER_URL = 'http://localhost:2020';

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
    const session = new Session(MOCK_SERVER_URL);
    expect(`${session}`).toEqual('<Session http://localhost:2020>');

    session.setKeyPair(await createKeyPair());
    expect(`${session}`).toMatch(
      /<Session http:\/\/localhost:2020 key pair \w{8}>/,
    );

    session.setSchema(schemaFixture());
    expect(`${session}`).toMatch(
      /<Session http:\/\/localhost:2020 key pair \w{8} schema \w{8}>/,
    );
  });

  describe('queryEntries', () => {
    it('can query entries', async () => {
      const session = new Session(MOCK_SERVER_URL);
      const entries = await session.queryEntries(schemaFixture());
      expect(entries.length).toBe(4);
    });

    it('throws when querying without a schema', async () => {
      const session = new Session(MOCK_SERVER_URL);
      // @ts-ignore: We deliberately use the API wrong here
      await expect(session.queryEntries()).rejects.toThrow(
        /Schema must be provided/,
      );
    });
  });

  describe('query', () => {
    it('can materialize instances', async () => {
      const session = new Session(MOCK_SERVER_URL);
      const instances = await session.query({
        schema: schemaFixture(),
      });
      expect(instances.length).toEqual(2);
      expect(instances[0]._meta.deleted).toEqual(true);
      expect(instances[1].message).toEqual(
        entryFixture(4).message?.fields?.message,
      );
    });
  });

  describe('publishEntry', () => {
    it('can publish entries', async () => {
      const session = new Session(MOCK_SERVER_URL);
      const nextEntryArgs = await session.publishEntry(
        encodedEntryFixture(4).entryBytes,
        encodedEntryFixture(4).payloadBytes,
      );
      expect(nextEntryArgs.entryHashBacklink).toEqual(
        entryArgsFixture(5).entryHashBacklink,
      );
    });

    it('throws when publishing without all required parameters', async () => {
      const session = new Session(MOCK_SERVER_URL);
      await expect(
        // @ts-ignore: We deliberately use the API wrong here
        session.publishEntry(null, encodedEntryFixture(1).payloadBytes),
      ).rejects.toThrow();
      await expect(
        // @ts-ignore: We deliberately use the API wrong here
        session.publishEntry(encodedEntryFixture(1).entryBytes, null),
      ).rejects.toThrow();
    });
  });

  describe('get/setNextEntryArgs', () => {
    it('returns next entry args from node', async () => {
      const session = new Session(MOCK_SERVER_URL);
      const nextEntryArgs = await session.getNextEntryArgs(
        authorFixture().publicKey,
        schemaFixture(),
      );
      expect(nextEntryArgs.entryHashSkiplink).toEqual(
        entryArgsFixture(5).entryHashSkiplink as string | undefined,
      );
      expect(nextEntryArgs.entryHashBacklink).toEqual(
        entryArgsFixture(5).entryHashBacklink,
      );
      expect(nextEntryArgs.seqNum).toEqual(entryArgsFixture(5).seqNum);
      expect(nextEntryArgs.logId).toEqual(entryArgsFixture(5).logId);
    });

    it('returns next entry args from cache', async () => {
      const session = new Session(MOCK_SERVER_URL);
      // Add a spy to check whether the value is really retrieved from the
      // cache and not requested
      const mockedFn = jest.fn(async () => true);
      session.client.request = mockedFn;

      const nextEntryArgs = {
        // convert json null into undefined
        entryHashBacklink: entryArgsFixture(5).entryHashBacklink as
          | string
          | undefined,
        entryHashSkiplink: entryArgsFixture(5).entryHashSkiplink as
          | string
          | undefined,
        logId: entryArgsFixture(5).logId,
        seqNum: entryArgsFixture(5).seqNum,
      };
      session.setNextEntryArgs(
        authorFixture().publicKey,
        schemaFixture(),
        nextEntryArgs,
      );

      const cacheResponse = await session.getNextEntryArgs(
        authorFixture().publicKey,
        schemaFixture(),
      );
      expect(cacheResponse.logId).toEqual(nextEntryArgs.logId);
      expect(mockedFn.mock.calls.length).toBe(0);
    });
  });
  describe('create', () => {
    it('creates an instance', async () => {
      const keyPair = await recoverKeyPair(authorFixture().privateKey);
      const session: Session = new Session(MOCK_SERVER_URL);

      const asyncFunctionMock = jest
        .fn()
        .mockResolvedValue(entryArgsFixture(1));
      jest
        .spyOn(session, 'getNextEntryArgs')
        .mockImplementation(asyncFunctionMock);

      const fields = entryFixture(1).message?.fields as Fields;

      expect(
        await session.setKeyPair(keyPair).create(fields, {
          schema: schemaFixture(),
        }),
      ).resolves;
    });
  });
  describe('updateInstance', () => {
    it('updates an instance', async () => {
      const keyPair = await recoverKeyPair(authorFixture().privateKey);
      const session = new Session(MOCK_SERVER_URL);

      const asyncFunctionMock = jest
        .fn()
        .mockResolvedValue(entryArgsFixture(2));
      jest
        .spyOn(session, 'getNextEntryArgs')
        .mockImplementation(asyncFunctionMock);

      // These are the fields for an update message
      const fields = entryFixture(2).message?.fields as Fields;

      // This is the instance id
      const id = entryFixture(2).message?.id as string;

      expect(
        await session.setKeyPair(keyPair).update(id, fields, {
          schema: schemaFixture(),
        }),
      ).resolves;
    });
  });
  describe('deleteInstance', () => {
    it('deletes an instance', async () => {
      const keyPair = await recoverKeyPair(authorFixture().privateKey);
      const session = new Session(MOCK_SERVER_URL);

      const asyncFunctionMock = jest
        .fn()
        .mockResolvedValue(entryArgsFixture(3));
      jest
        .spyOn(session, 'getNextEntryArgs')
        .mockImplementation(asyncFunctionMock);

      // This is the instance id
      const id = entryFixture(3).message?.id as string;

      expect(
        session.setKeyPair(keyPair).delete(id, {
          schema: schemaFixture(),
        }),
      ).resolves;
    });
  });
});

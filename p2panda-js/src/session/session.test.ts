// SPDX-License-Identifier: AGPL-3.0-or-later

/* eslint-disable @typescript-eslint/ban-ts-comment */

import { KeyPair } from 'wasm';
import { recoverKeyPair } from '~/identity';
import { Session } from '~/session';
import { Fields } from '~/types';
import {
  authorFixture,
  entryFixture,
  encodedEntryFixture,
  entryArgsFixture,
  schemaFixture,
  documentId
} from '../../test/fixtures';

const MOCK_SERVER_URL = 'http://localhost:2020';

/**
 * Test the `Session` class
 *
 * These tests expect the mock rpc server to be running, which can be started
 * with `npm run test:mock-node`.
 */
describe('Session', () => {
  let keyPair: KeyPair;
  beforeAll(async () => {
    keyPair = await recoverKeyPair(authorFixture().privateKey);
  });

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

    session.setKeyPair(keyPair);
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
    let session: Session;

    beforeEach(() => {
      session = new Session(MOCK_SERVER_URL).setKeyPair(keyPair);
    });

    it('handles valid arguments', async () => {
      expect(session.query({ schema: schemaFixture() })).resolves;
      expect(session.setSchema(schemaFixture()).query()).resolves;
    });

    it('can materialize instances', async () => {
      const instances = await session.query({
        schema: schemaFixture(),
      });
      expect(instances.length).toEqual(1);
      expect(instances[0]._meta.deleted).toEqual(true);
    });

    it('throws when missing a required parameter', async () => {
      await expect(session.query()).rejects.toThrow();
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
        documentId(),
      );
      expect(nextEntryArgs.entryHashSkiplink).toEqual(
        entryArgsFixture(5).entryHashSkiplink,
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
        documentId(),
        nextEntryArgs,
      );

      const cacheResponse = await session.getNextEntryArgs(
        authorFixture().publicKey,
        documentId(),
      );
      expect(cacheResponse.logId).toEqual(nextEntryArgs.logId);
      expect(mockedFn.mock.calls.length).toBe(0);
    });
  });

  describe('create', () => {
    let session: Session;

    // Fields for instance to be created
    const fields = entryFixture(1).operation?.fields as Fields;

    beforeEach(async () => {
      session = new Session(MOCK_SERVER_URL).setKeyPair(keyPair);
    });

    it('handles valid arguments', async () => {
      jest
        .spyOn(session, 'getNextEntryArgs')
        .mockResolvedValue(entryArgsFixture(1));

      expect(
        await session.create(fields, {
          schema: schemaFixture(),
        }),
      ).resolves;
      expect(await session.setSchema(schemaFixture()).create(fields)).resolves;
    });

    it('throws when missing a required parameter', async () => {
      await expect(
        // @ts-ignore: We deliberately use the API wrong here
        session.setKeyPair(keyPair).create(),
      ).rejects.toThrow();
    });
  });

  describe('update', () => {
    let session: Session;

    // These are the fields for an update operation
    const fields = entryFixture(2).operation?.fields as Fields;

    // This is the instance id
    const id = entryFixture(2).operation?.id as string;

    beforeEach(async () => {
      session = new Session(MOCK_SERVER_URL).setKeyPair(keyPair);
      jest
        .spyOn(session, 'getNextEntryArgs')
        .mockResolvedValue(entryArgsFixture(2));
    });

    it('handles valid arguments', async () => {
      expect(
        await session.update(id, fields, {
          schema: schemaFixture(),
        }),
      ).resolves;

      expect(await session.setSchema(schemaFixture()).update(id, fields))
        .resolves;
    });

    it('throws when missing a required parameter', async () => {
      await expect(
        // @ts-ignore: We deliberately use the API wrong here
        session.update(null, fields, { schema: schemaFixture() }),
      ).rejects.toThrow();
      await expect(
        // @ts-ignore: We deliberately use the API wrong here
        session.update(id, null, { schema: schemaFixture() }),
      ).rejects.toThrow();
      await expect(
        // @ts-ignore: We deliberately use the API wrong here
        session.update(id, fields),
      ).rejects.toThrow();
    });
  });

  describe('delete', () => {
    let session: Session;

    // This is the instance id that can be deleted
    const instanceId = entryFixture(3).operation?.id as string;

    beforeEach(async () => {
      session = new Session(MOCK_SERVER_URL).setKeyPair(keyPair);
      jest
        .spyOn(session, 'getNextEntryArgs')
        .mockResolvedValue(entryArgsFixture(3));
    });

    it('handles valid arguments', async () => {
      expect(session.delete(instanceId, { schema: schemaFixture() })).resolves;
      expect(session.setSchema(schemaFixture()).delete(instanceId)).resolves;
    });

    it('throws when missing a required parameter', async () => {
      expect(
        // @ts-ignore: We deliberately use the API wrong here
        session.delete(null, { schema: schemaFixture() }),
      ).rejects.toThrow();

      expect(
        // @ts-ignore: We deliberately use the API wrong here
        session.delete(instanceId),
      ).rejects.toThrow();
    });
  });
});

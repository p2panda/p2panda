// SPDX-License-Identifier: AGPL-3.0-or-later

/* eslint-disable @typescript-eslint/ban-ts-comment */

import { createMockClient } from 'mock-apollo-client';

import { GQL_NEXT_ARGS, GQL_PUBLISH } from './session';
import { KeyPair } from '../wasm';
import { Session } from './';
import { recoverKeyPair } from '../identity';

import type { Fields } from '../types';

import {
  authorFixture,
  documentIdFixture,
  encodedEntryFixture,
  entryArgsFixture,
  entryFixture,
  schemaFixture,
} from '../../test/fixtures';

/**
 * Simple mock p2panda session.
 *
 * Will respond to:
 * - query `nextEntryArgs`: always returns entry args for sequence number 6
 * - mutation `publishEntry` always returns a response as if sequence number 5
 *  had been published.
 *
 * @returns Session
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const createMockSession = (): Session => {
  const session = new Session('http://localhost:2020');
  const mockClient = createMockClient();

  // Register nextEntryArgs handler
  mockClient.setRequestHandler(GQL_NEXT_ARGS, () =>
    Promise.resolve({
      data: {
        nextEntryArgs: entryArgsFixture(5),
      },
    }),
  );

  // Register publishEntry handler
  mockClient.setRequestHandler(GQL_PUBLISH, () =>
    Promise.resolve({
      data: {
        publishEntry: entryArgsFixture(5),
      },
    }),
  );

  session.client = mockClient;
  return session;
};

/**
 * Test the `Session` class.
 *
 * These tests expect the mock rpc server to be running, which can be started
 * with `npm run test:mock-node`.
 */
describe('Session', () => {
  let keyPair: KeyPair;

  beforeAll(() => {
    keyPair = recoverKeyPair(authorFixture().privateKey);
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
    const session = createMockSession();
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

  describe('publish', () => {
    it('can publish entries', async () => {
      const session = createMockSession();

      try {
        const nextArgs = await session.publish(
          encodedEntryFixture(4).entryBytes,
          encodedEntryFixture(4).payloadBytes,
        );
        expect(nextArgs.backlink).toEqual(entryArgsFixture(5).backlink);
      } catch (err) {
        console.error(err);
        throw err;
      }
    });

    it('throws when publishing without all required parameters', async () => {
      const session = createMockSession();
      await expect(
        // @ts-ignore: We deliberately use the API wrong here
        session.publish(null, encodedEntryFixture(1).payloadBytes),
      ).rejects.toThrow();
      await expect(
        // @ts-ignore: We deliberately use the API wrong here
        session.publish(encodedEntryFixture(1).entryBytes, null),
      ).rejects.toThrow();
    });
  });

  describe('get/setNextEntryArgs', () => {
    it('returns next entry args from node', async () => {
      const session = createMockSession();

      const nextArgs = await session.getNextArgs(
        authorFixture().publicKey,
        documentIdFixture(),
      );
      expect(nextArgs.skiplink).toEqual(entryArgsFixture(5).skiplink);
      expect(nextArgs.backlink).toEqual(entryArgsFixture(5).backlink);
      expect(nextArgs.seqNum).toEqual(entryArgsFixture(5).seqNum);
      expect(nextArgs.logId).toEqual(entryArgsFixture(5).logId);
    });

    it('returns next entry args from cache', async () => {
      const session = createMockSession();
      // Add a spy to check whether the value is really retrieved from the
      // cache and not requested
      const mockedFn = jest.fn(async () => true);
      // @ts-ignore Yes, Typescript, a mock is not the same as the original.
      session.client.query = mockedFn;

      const nextArgs = {
        // Treat json `null` as undefined
        backlink: entryArgsFixture(5).backlink as string | undefined,
        skiplink: entryArgsFixture(5).skiplink as string | undefined,
        logId: entryArgsFixture(5).logId,
        seqNum: entryArgsFixture(5).seqNum,
      };
      session.setNextArgs(
        authorFixture().publicKey,
        documentIdFixture(),
        nextArgs,
      );

      const cacheResponse = await session.getNextArgs(
        authorFixture().publicKey,
        documentIdFixture(),
      );
      expect(cacheResponse.logId).toEqual(nextArgs.logId);
      expect(mockedFn.mock.calls.length).toBe(0);
    });
  });

  describe('create', () => {
    let session: Session;

    // Fields for document to be created
    const fields = entryFixture(1).operation?.fields as Fields;

    beforeEach(async () => {
      session = createMockSession();
      session.setKeyPair(keyPair);
    });

    it('handles valid arguments', async () => {
      jest.spyOn(session, 'getNextArgs').mockResolvedValue(entryArgsFixture(1));

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

    // This is the document id
    const documentId = documentIdFixture();

    // These are the previous operations
    const previousOperations = entryFixture(2).operation
      ?.previous_operations as string[];

    beforeEach(async () => {
      session = createMockSession();
      session.setKeyPair(keyPair);
      jest.spyOn(session, 'getNextArgs').mockResolvedValue(entryArgsFixture(2));
    });

    it('handles valid arguments', async () => {
      expect(
        await session.update(documentId, fields, previousOperations, {
          schema: schemaFixture(),
        }),
      ).resolves;

      expect(
        await session
          .setSchema(schemaFixture())
          .update(documentId, fields, previousOperations),
      ).resolves;
    });

    it('throws when missing a required parameter', async () => {
      await expect(
        // @ts-ignore: We deliberately use the API wrong here
        session.update(null, fields, { schema: schemaFixture() }),
      ).rejects.toThrow();
      await expect(
        // @ts-ignore: We deliberately use the API wrong here
        session.update(documentId, null, { schema: schemaFixture() }),
      ).rejects.toThrow();
      await expect(
        // @ts-ignore: We deliberately use the API wrong here
        session.update(documentId, fields),
      ).rejects.toThrow();
    });
  });

  describe('delete', () => {
    let session: Session;

    // This is the document id that can be deleted
    const documentId = documentIdFixture();

    // These are the previous operations
    const previousOperations = entryFixture(2).operation
      ?.previous_operations as string[];

    beforeEach(async () => {
      session = createMockSession();
      session.setKeyPair(keyPair);
      jest.spyOn(session, 'getNextArgs').mockResolvedValue(entryArgsFixture(3));
    });

    it('handles valid arguments', async () => {
      expect(
        session.delete(documentId, previousOperations, {
          schema: schemaFixture(),
        }),
      ).resolves;
      expect(
        session
          .setSchema(schemaFixture())
          .delete(documentId, previousOperations),
      ).resolves;
    });

    it('throws when missing a required parameter', async () => {
      expect(
        // @ts-ignore: We deliberately use the API wrong here
        session.delete(null, { schema: schemaFixture() }),
      ).rejects.toThrow();

      expect(
        // @ts-ignore: We deliberately use the API wrong here
        session.delete(documentId),
      ).rejects.toThrow();
    });
  });
});

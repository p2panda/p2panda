// SPDX-License-Identifier: AGPL-3.0-or-later

import { recoverKeyPair } from '~/identity';
import { Session } from '~/session';

import type { Fields } from '~/types';

import { createDocument, deleteDocument, updateDocument } from '.';

import {
  authorFixture,
  entryFixture,
  encodedEntryFixture,
  entryArgsFixture,
  schemaFixture,
} from '../../test/fixtures';

jest.mock('~/session');

const MOCK_SERVER_URL = 'http://localhost:2020';

describe('document', () => {
  describe('createDocument', () => {
    it('creates a document', async () => {
      const keyPair = await recoverKeyPair(authorFixture().privateKey);
      const session: Session = new Session(MOCK_SERVER_URL);
      session.setKeyPair(keyPair);

      const asyncFunctionMock = jest
        .fn()
        .mockResolvedValue(entryArgsFixture(1));
      jest
        .spyOn(session, 'getNextEntryArgs')
        .mockImplementation(asyncFunctionMock);

      const fields = entryFixture(1).operation?.fields as Fields;

      const entryEncoded = await createDocument(fields, {
        keyPair,
        schema: schemaFixture(),
        session,
      });

      expect(entryEncoded).toEqual(encodedEntryFixture(1).entryBytes);
    });
  });

  describe('updateDocument', () => {
    it('updates a document', async () => {
      const keyPair = await recoverKeyPair(authorFixture().privateKey);
      const session = new Session(MOCK_SERVER_URL);
      session.setKeyPair(keyPair);

      const asyncFunctionMock = jest
        .fn()
        .mockResolvedValue(entryArgsFixture(2));
      jest
        .spyOn(session, 'getNextEntryArgs')
        .mockImplementation(asyncFunctionMock);

      // These are the fields for an update operation
      const fields = entryFixture(2).operation?.fields as Fields;

      // This is the document id
      const id = entryFixture(2).operation?.id as string;

      const previousOperations = entryFixture(2).operation?.previousOperations as string[];

      const entryEncoded = await updateDocument(id, fields, previousOperations, {
        keyPair,
        schema: schemaFixture(),
        session,
      });

      expect(entryEncoded).toEqual(encodedEntryFixture(2).entryBytes);
    });
  });

  describe('deleteDocument', () => {
    it('deletes a document', async () => {
      const keyPair = await recoverKeyPair(authorFixture().privateKey);
      const session = new Session(MOCK_SERVER_URL);
      session.setKeyPair(keyPair);

      const asyncFunctionMock = jest
        .fn()
        .mockResolvedValue(entryArgsFixture(4));
      jest
        .spyOn(session, 'getNextEntryArgs')
        .mockImplementation(asyncFunctionMock);

      // This is the document id
      const id = entryFixture(3).operation?.id as string;

      const previousOperations = entryFixture(3).operation?.previousOperations as string[];

      const entryEncoded = await deleteDocument(id, previousOperations, {
        keyPair,
        schema: schemaFixture(),
        session,
      });

      expect(entryEncoded).toEqual(encodedEntryFixture(4).entryBytes);
    });
  });
});

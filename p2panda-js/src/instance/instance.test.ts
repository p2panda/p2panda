// SPDX-License-Identifier: AGPL-3.0-or-later

import { recoverKeyPair } from '~/identity';
import { Session } from '~/session';
import { Fields } from '~/types';

import { createInstance, deleteInstance, updateInstance } from '.';

import {
  authorFixture,
  entryFixture,
  encodedEntryFixture,
  entryArgsFixture,
  schemaFixture,
} from '../../test/fixtures';
jest.mock('~/session');

const MOCK_SERVER_URL = 'http://localhost:2020';

describe('instance', () => {
  describe('createInstance', () => {
    it('creates an instance', async () => {
      const keyPair = await recoverKeyPair(authorFixture().privateKey);
      const session: Session = new Session(MOCK_SERVER_URL);
      session.setKeyPair(keyPair);

      const asyncFunctionMock = jest
        .fn()
        .mockResolvedValue(entryArgsFixture(1));
      jest
        .spyOn(session, 'getNextEntryArgs')
        .mockImplementation(asyncFunctionMock);

      const fields = entryFixture(1).message?.fields as Fields;

      const entryEncoded = await createInstance(fields, {
        keyPair,
        schema: schemaFixture(),
        session,
      });

      expect(entryEncoded).toEqual(encodedEntryFixture(1).entryBytes);
    });
  });
  describe('updateInstance', () => {
    it('updates an instance', async () => {
      const keyPair = await recoverKeyPair(authorFixture().privateKey);
      const session = new Session(MOCK_SERVER_URL);
      session.setKeyPair(keyPair);

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

      const entryEncoded = await updateInstance(id, fields, {
        keyPair,
        schema: schemaFixture(),
        session,
      });

      expect(entryEncoded).toEqual(encodedEntryFixture(2).entryBytes);
    });
  });
  describe('deleteInstance', () => {
    it('deletes an instance', async () => {
      const keyPair = await recoverKeyPair(authorFixture().privateKey);
      const session = new Session(MOCK_SERVER_URL);
      session.setKeyPair(keyPair);

      const asyncFunctionMock = jest
        .fn()
        .mockResolvedValue(entryArgsFixture(3));
      jest
        .spyOn(session, 'getNextEntryArgs')
        .mockImplementation(asyncFunctionMock);

      // This is the instance id
      const id = entryFixture(3).message?.id as string;

      const entryEncoded = await deleteInstance(id, {
        keyPair,
        schema: schemaFixture(),
        session,
      });

      expect(entryEncoded).toEqual(encodedEntryFixture(3).entryBytes);
    });
  });
});

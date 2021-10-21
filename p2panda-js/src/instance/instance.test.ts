// SPDX-License-Identifier: AGPL-3.0-or-later

import { recoverKeyPair } from '~/identity';
import { Session } from '~/session';
import { createInstance, updateInstance } from '.';
import { FieldsTagged } from '~/types';
jest.mock('~/session');

import TEST_DATA from '~/../test/test-data.json';
import { marshallResponseFields } from '~/utils';

const PANDA_LOG = TEST_DATA.panda.logs[0];
const SCHEMA = PANDA_LOG.decodedMessages[0].schema;

const MOCK_SERVER_URL = 'http://localhost:2020';

describe('instance', () => {
  describe('createInstance', () => {
    it('creates an instance', async () => {
      const keyPair = await recoverKeyPair(TEST_DATA.panda.privateKey);
      const session: Session = new Session(MOCK_SERVER_URL);
      session.setKeyPair(keyPair);

      const asyncFunctionMock = jest
        .fn()
        .mockResolvedValue(PANDA_LOG.nextEntryArgs[0]);
      jest
        .spyOn(session, 'getNextEntryArgs')
        .mockImplementation(asyncFunctionMock);

      const fieldsTagged = PANDA_LOG.decodedMessages[0].fields as FieldsTagged;

      const fields = marshallResponseFields(fieldsTagged);

      const entryEncoded = await createInstance(fields, {
        keyPair,
        schema: SCHEMA,
        session,
      });

      expect(entryEncoded).toEqual(PANDA_LOG.encodedEntries[0].entryBytes);
    });
  });
  describe('updateInstance', () => {
    it('updates an instance', async () => {
      const keyPair = await recoverKeyPair(TEST_DATA.panda.privateKey);
      const session = new Session(MOCK_SERVER_URL);
      session.setKeyPair(keyPair);

      const asyncFunctionMock = jest
        .fn()
        .mockResolvedValue(PANDA_LOG.nextEntryArgs[1]);
      jest
        .spyOn(session, 'getNextEntryArgs')
        .mockImplementation(asyncFunctionMock);

      // These are the fields for an update message
      const fieldsTagged = PANDA_LOG.decodedMessages[1].fields as FieldsTagged;

      const fields = marshallResponseFields(fieldsTagged);
      // This is the instance id
      const id = PANDA_LOG.decodedMessages[1].id as string;

      const entryEncoded = await updateInstance(id, fields, {
        keyPair,
        schema: SCHEMA,
        session,
      });

      expect(entryEncoded).toEqual(PANDA_LOG.encodedEntries[1].entryBytes);
    });
  });
});

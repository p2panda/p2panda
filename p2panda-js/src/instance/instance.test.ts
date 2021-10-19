// SPDX-License-Identifier: AGPL-3.0-or-later

import wasm from '~/wasm';
import { recoverKeyPair } from '~/identity';
import { Session } from '~/session';
import { createInstance } from '.';
import { Fields } from '~/types';

import TEST_DATA from '~/../test/test-data.json';

const PANDA_LOG = TEST_DATA.panda.logs[0];
const SCHEMA = PANDA_LOG.decodedMessages[0].schema;

const MOCK_SERVER_URL = 'http://localhost:2020';

describe('instance', () => {
  describe('createInstance', () => {
    it('creates an instance', async () => {
      const { decodeEntry } = await wasm;

      const keyPair = await recoverKeyPair(TEST_DATA.panda.privateKey);
      const session = new Session(MOCK_SERVER_URL);
      session.setKeyPair(keyPair);

      const fields: Fields = {
        message: 'A shiny new message!',
      };

      const entryEncoded = await createInstance(fields, {
        keyPair,
        schema: SCHEMA,
        session,
      });

      const entry = decodeEntry(entryEncoded);

      // Although we don't have a fixture value for the encoded entry (because it doesn't exist
      // in our test log), we can still compare the decoded values with what we would expect to see.
      expect(entry.logId).toEqual(PANDA_LOG.nextEntryArgs.logId);
      expect(entry.seqNum).toEqual(PANDA_LOG.nextEntryArgs.seqNum);
      expect(entry.entryHashBacklink).toEqual(
        PANDA_LOG.nextEntryArgs.entryHashBacklink,
      );
      expect(entry.entryHashSkiplink).toEqual(
        PANDA_LOG.nextEntryArgs.entryHashSkiplink,
      );
    });
  });
});

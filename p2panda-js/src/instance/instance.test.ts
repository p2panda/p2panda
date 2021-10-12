// SPDX-License-Identifier: AGPL-3.0-or-later

import { recoverKeyPair } from '~/identity';
import { Session } from '~/session';
import { createInstance } from '.';

import TEST_DATA from '~/../test/test-data.json';
import { marshallResponseFields } from '~/utils';
import { FieldsTagged } from '~/types';

// const ENTRIES = TEST_DATA.entries;
// const PUBLIC_KEY = TEST_DATA.meta.keyPair.publicKey;
// const SCHEMA = TEST_DATA.meta.schema;
// const ENTRY_ARGS = TEST_DATA.nextEntryArgs;

const MOCK_SERVER_URL = 'http://localhost:2020';

describe('instance', () => {
  describe('createInstance', () => {
    it('creates an instance', async () => {
      const keyPair = await recoverKeyPair(TEST_DATA.meta.keyPair.privateKey);
      const session = new Session(MOCK_SERVER_URL);
      session.setKeyPair(keyPair);

      const fields = marshallResponseFields(
        TEST_DATA.decodedEntries[0].message.fields as FieldsTagged,
      );
      const entryEncoded = await createInstance(fields, {
        keyPair,
        schema: TEST_DATA.meta.schema,
        session,
      });

      expect(entryEncoded).toEqual(TEST_DATA.entries[0].entryBytes);
    });
  });
});

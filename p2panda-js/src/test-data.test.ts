// SPDX-License-Identifier: AGPL-3.0-or-later

import wasm from '~/wasm';
import { recoverKeyPair } from './identity';

import TEST_DATA from '../test/test-data.json';

describe('test data', () => {
  it('contains correct keyPair values', async () => {
    const keyPair = await recoverKeyPair(TEST_DATA.meta.keyPair.privateKey);

    expect(keyPair.publicKey()).toEqual(TEST_DATA.meta.keyPair.publicKey);
  });

  it('contains correct values for signing and encoding first entry in log', async () => {
    const { signEncodeEntry } = await wasm;
    const keyPair = await recoverKeyPair(TEST_DATA.meta.keyPair.privateKey);

    const { entryEncoded } = signEncodeEntry(
      keyPair,
      TEST_DATA.entries[0].payloadBytes,
      undefined,
      undefined,
      1,
      1,
    );
    expect(entryEncoded).toEqual(TEST_DATA.entries[0].entryBytes);
  });

  it('contains correct message fields', async () => {
    const { MessageFields } = await wasm;
    const { encodeCreateMessage } = await wasm;

    const messageFields = new MessageFields();
    const fields = TEST_DATA.decodedEntries[0].message.fields;
    messageFields.add(
      'description',
      fields['description']['type'],
      fields['description']['value'],
    );
    messageFields.add('name', fields['name']['type'], fields['name']['value']);

    const encodedMessage = encodeCreateMessage(
      TEST_DATA.meta.schema,
      messageFields,
    );

    expect(encodedMessage).toEqual(TEST_DATA.entries[0].payloadBytes);
  });
});

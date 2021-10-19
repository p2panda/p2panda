// SPDX-License-Identifier: AGPL-3.0-or-later

import wasm from '~/wasm';
import { recoverKeyPair } from './identity';

import TEST_DATA from '~/../test/test-data.json';

const PANDA_LOG = TEST_DATA.panda.logs[0];
const SCHEMA = PANDA_LOG.decodedMessages[0].schema;

describe('test data', () => {
  it('contains correct keyPair values', async () => {
    const keyPair = await recoverKeyPair(TEST_DATA.panda.privateKey);

    expect(keyPair.publicKey()).toEqual(TEST_DATA.panda.publicKey);
  });

  it('contains correct values for signing and encoding first entry in log', async () => {
    const { signEncodeEntry } = await wasm;
    const keyPair = await recoverKeyPair(TEST_DATA.panda.privateKey);

    const { entryEncoded } = signEncodeEntry(
      keyPair,
      PANDA_LOG.encodedEntries[0].payloadBytes,
      undefined,
      undefined,
      1,
      1,
    );
    expect(entryEncoded).toEqual(PANDA_LOG.encodedEntries[0].entryBytes);
  });

  it('contains correct message fields', async () => {
    const { MessageFields } = await wasm;
    const { encodeCreateMessage } = await wasm;

    const messageFields = new MessageFields();
    const fields = PANDA_LOG.decodedMessages[0].fields;
    messageFields.add(
      'message',
      fields['message']['type'],
      fields['message']['value'],
    );
    const encodedMessage = encodeCreateMessage(SCHEMA, messageFields);

    expect(encodedMessage).toEqual(PANDA_LOG.encodedEntries[0].payloadBytes);
  });
});

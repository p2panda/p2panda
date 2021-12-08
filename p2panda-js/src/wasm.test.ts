// SPDX-License-Identifier: AGPL-3.0-or-later

import wasm from '~/wasm';

import TEST_DATA from '../test/test-data.json';

const TEST_SCHEMA = TEST_DATA.panda.logs[0].decodedMessages[0].schema;

describe('Web assembly interface', () => {
  describe('KeyPair', () => {
    it('creates a key pair', async () => {
      const { KeyPair } = await wasm;
      const keyPair = new KeyPair();
      expect(keyPair.privateKey().length).toBe(64);
    });

    it('restores a key pair', async () => {
      const { KeyPair } = await wasm;
      const keyPair = new KeyPair();
      const keyPairSecond = KeyPair.fromPrivateKey(keyPair.privateKey());
      expect(keyPair.publicKey()).toBe(keyPairSecond.publicKey());
    });

    it('signs and validates', async () => {
      const { KeyPair, verifySignature } = await wasm;
      const keyPair = new KeyPair();
      const publicKey = keyPair.publicKey();
      const message = 'Hello, Signature!';
      const signature = keyPair.sign(message);
      expect(verifySignature(publicKey, message, signature)).toBeTruthy();
      expect(
        verifySignature(publicKey, 'Wrong Message!', signature),
      ).toBeFalsy();
    });
  });

  describe('MessageFields', () => {
    it('stores and returns the right fields', async () => {
      const { MessageFields } = await wasm;
      const fields = new MessageFields();

      // Set fields of all possible types
      fields.add('description', 'str', 'Hello, Panda');
      fields.add('temperature', 'int', 23);
      fields.add('isCute', 'bool', true);
      fields.add('degree', 'float', 12.322);
      fields.add('username', 'relation', TEST_SCHEMA);

      // Returns the correct fields
      expect(fields.get('description')).toBe('Hello, Panda');
      expect(fields.get('temperature')).toBe(23);
      expect(fields.get('isCute')).toBe(true);
      expect(fields.get('degree')).toBe(12.322);
      expect(fields.get('username')).toBe(TEST_SCHEMA);

      // Return nothing when field does not exist
      expect(fields.get('message')).toBe(null);
    });

    it('returns the correct length', async () => {
      const { MessageFields } = await wasm;
      const fields = new MessageFields();
      expect(fields.length()).toBe(0);
      fields.add('message', 'str', 'Good morning');
      expect(fields.length()).toBe(1);
      fields.remove('message');
      expect(fields.length()).toBe(0);
    });

    it('throws when trying to set a field twice', async () => {
      const { MessageFields } = await wasm;
      const fields = new MessageFields();
      fields.add('description', 'str', 'Good morning, Panda');
      expect(() =>
        fields.add('description', 'str', 'Good night, Panda'),
      ).toThrow('field already exists');
    });

    it('throws when using invalid types or values', async () => {
      const { MessageFields } = await wasm;
      const fields = new MessageFields();

      // Throw when type is invalid
      expect(() => fields.add('test', 'lulu', true)).toThrow(
        'Unknown type value',
      );
      expect(() => fields.add('test', 'int', true)).toThrow(
        'Invalid integer value',
      );

      // Throw when relation is an invalid hash
      expect(() => fields.add('contact', 'relation', 'test')).toThrow(
        'invalid hex encoding in hash string',
      );
    });

    it('throws when removing an inexistent field', async () => {
      const { MessageFields } = await wasm;
      const fields = new MessageFields();
      expect(() => fields.remove('test')).toThrow();
    });
  });

  describe('Entries', () => {
    it('creates, signs and decodes an entry', async () => {
      const LOG_ID = 5;
      const SEQ_NUM = 1;

      const {
        KeyPair,
        MessageFields,
        decodeEntry,
        encodeCreateMessage,
        signEncodeEntry,
      } = await wasm;

      // Generate new key pair
      const keyPair = new KeyPair();

      // Create message
      const fields = new MessageFields();
      fields.add('description', 'str', 'Hello, Panda');
      expect(fields.get('description')).toBe('Hello, Panda');

      const messageEncoded = encodeCreateMessage(TEST_SCHEMA, fields);

      // Sign and encode entry
      const { entryEncoded, entryHash } = signEncodeEntry(
        keyPair,
        messageEncoded,
        undefined,
        undefined,
        SEQ_NUM,
        LOG_ID,
      );

      expect(entryHash.length).toBe(68);

      // Decode entry and return as JSON
      const decodedEntry = decodeEntry(entryEncoded, messageEncoded);

      expect(decodedEntry.entryHashBacklink).toBeNull();
      expect(decodedEntry.entryHashSkiplink).toBeNull();
      expect(decodedEntry.logId).toBe(LOG_ID);
      expect(decodedEntry.message.action).toBe('create');
      expect(decodedEntry.message.schema).toBe(TEST_SCHEMA);
      expect(decodedEntry.message.fields.description.value).toBe(
        'Hello, Panda',
      );
      expect(decodedEntry.message.fields.description.type).toBe('str');

      // Test decoding entry without message
      expect(() => decodeEntry(entryEncoded)).not.toThrow();
    });
  });
});

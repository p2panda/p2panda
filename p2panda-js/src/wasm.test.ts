// SPDX-License-Identifier: AGPL-3.0-or-later

import wasm from '~/wasm';

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
      const { KeyPair } = await wasm;
      const keyPair = new KeyPair();
      const message = new Uint8Array([1, 2, 3]);
      const sig = keyPair.sign(message);
      expect(keyPair.verify(message, sig)).toBeTruthy();
      expect(keyPair.verify(new Uint8Array([3, 4, 5]), sig)).toBeFalsy();
    });
  });

  describe('MessageFields', () => {
    it('stores and returns the right fields', async () => {
      const TEST_SCHEMA =
        '0040cf94f6d605657e90c543b0c919070cdaaf7209c5e1ea58acb8f3568fa2114268dc9ac3bafe12af277d286fce7dc59b7c0c348973c4e9dacbe79485e56ac2a702';

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
      const TEST_SCHEMA =
        '0040cf94f6d605657e90c543b0c919070cdaaf7209c5e1ea58acb8f3568fa2114268dc9ac3bafe12af277d286fce7dc59b7c0c348973c4e9dacbe79485e56ac2a702';
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

      expect(entryHash.length).toBe(132);

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

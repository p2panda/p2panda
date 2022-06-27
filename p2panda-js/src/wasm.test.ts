// SPDX-License-Identifier: AGPL-3.0-or-later

import wasm from '~/wasm';

const TEST_HASH =
  '0020ddc99aca776df0ca9d1b5871ba39d4edacc752a0a3426b12c3958971b6c847ac';
const TEST_SCHEMA = `test_${TEST_HASH}`;

describe('WebAssembly interface', () => {
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
        verifySignature(publicKey, 'Wrong Operation!', signature),
      ).toBeFalsy();
    });
  });

  describe('OperationFields', () => {
    it('stores and returns the right fields', async () => {
      const { OperationFields } = await wasm;
      const fields = new OperationFields();

      // Set fields of all possible types
      fields.add('description', 'str', 'Hello, Panda');
      fields.add('temperature', 'int', '32');
      fields.add('isCute', 'bool', true);
      fields.add('degree', 'float', 12.322);
      fields.add('username', 'relation', TEST_HASH);
      fields.add('locations', 'relation_list', [TEST_HASH]);
      fields.add('that_one_funny_comment_i_made', 'pinned_relation', [
        TEST_HASH,
      ]);
      fields.add('those_many_funny_comments_i_made', 'pinned_relation_list', [
        [TEST_HASH],
      ]);

      // Returns the correct fields
      expect(fields.get('description')).toBe('Hello, Panda');
      expect(fields.get('temperature')).toEqual(BigInt(32));
      expect(fields.get('isCute')).toBe(true);
      expect(fields.get('degree')).toBe(12.322);
      expect(fields.get('username')).toEqual(TEST_HASH);
      expect(fields.get('locations')).toEqual([TEST_HASH]);
      expect(fields.get('that_one_funny_comment_i_made')).toEqual([TEST_HASH]);
      expect(fields.get('those_many_funny_comments_i_made')).toEqual([
        [TEST_HASH],
      ]);

      // Return nothing when field does not exist
      expect(fields.get('message')).toBe(null);
    });

    it('returns the correct length', async () => {
      const { OperationFields } = await wasm;
      const fields = new OperationFields();
      expect(fields.length()).toBe(0);
      fields.add('message', 'str', 'Good morning');
      expect(fields.length()).toBe(1);
      fields.remove('message');
      expect(fields.length()).toBe(0);
    });

    it('throws when trying to set a field twice', async () => {
      const { OperationFields } = await wasm;
      const fields = new OperationFields();
      fields.add('description', 'str', 'Good morning, Panda');
      expect(() =>
        fields.add('description', 'str', 'Good night, Panda'),
      ).toThrow('field already exists');
    });

    it('throws when using invalid types or values', async () => {
      const { OperationFields } = await wasm;
      const fields = new OperationFields();

      // Throw when type is invalid
      expect(() => fields.add('test', 'lulu', true)).toThrow(
        'Unknown value type',
      );
      expect(() => fields.add('test', 'int', 'notanumber')).toThrow(
        'Invalid integer value',
      );

      expect(() => fields.add('contact', 'relation', [TEST_HASH])).toThrow(
        'Expected an operation id value for field of type relation',
      );

      expect(() => fields.add('contact', 'relation_list', TEST_HASH)).toThrow(
        'Expected an array of operation ids for field of type relation list',
      );

      expect(() => fields.add('contact', 'pinned_relation', TEST_HASH)).toThrow(
        'Expected an array of operation ids for field of type pinned relation list',
      );

      expect(() =>
        fields.add('contact', 'pinned_relation_list', [TEST_HASH]),
      ).toThrow(
        'Expected a nested array of operation ids for field of type pinned relation list',
      );

      // Throw when relation is an invalid hash
      expect(() =>
        fields.add('contact', 'relation', 'this is not a hash'),
      ).toThrow('invalid hex encoding in hash string');

      expect(() =>
        fields.add('contact', 'relation_list', ['this is not a hash']),
      ).toThrow('invalid hex encoding in hash string');
    });

    it('throws when removing an inexistent field', async () => {
      const { OperationFields } = await wasm;
      const fields = new OperationFields();
      expect(() => fields.remove('test')).toThrow();
    });
  });

  describe('Entries', () => {
    it('creates, signs and decodes an entry', async () => {
      const LOG_ID = 5;
      const SEQ_NUM = 1;

      const {
        KeyPair,
        OperationFields,
        decodeEntry,
        encodeCreateOperation,
        signEncodeEntry,
      } = await wasm;

      // Generate new key pair
      const keyPair = new KeyPair();

      // Create operation with fields
      const fields = new OperationFields();
      fields.add('description', 'str', 'Hello, Panda');
      expect(fields.get('description')).toBe('Hello, Panda');

      const operationEncoded = encodeCreateOperation(TEST_SCHEMA, fields);

      // Sign and encode entry
      const { entryEncoded, entryHash } = signEncodeEntry(
        keyPair,
        operationEncoded,
        undefined,
        undefined,
        BigInt(SEQ_NUM),
        BigInt(LOG_ID),
      );
      expect(entryHash.length).toBe(68);

      // Decode entry and return as JSON
      const decodedEntry = decodeEntry(entryEncoded, operationEncoded);
      expect(decodedEntry.logId).toEqual(BigInt(LOG_ID));
      expect(decodedEntry.seqNum).toEqual(BigInt(SEQ_NUM));
      expect(decodedEntry.backlink).toBeUndefined();
      expect(decodedEntry.skiplink).toBeUndefined();
      expect(decodedEntry.operation.action).toBe('create');
      expect(decodedEntry.operation.schema).toEqual(TEST_SCHEMA);

      // Test operation fields map
      const { fields: operationFields } = decodedEntry.operation;
      expect(operationFields.get('description').value).toBe('Hello, Panda');
      expect(operationFields.get('description').type).toBe('str');

      // Test decoding entry without operation
      expect(() => decodeEntry(entryEncoded)).not.toThrow();
    });

    it('encodes and decodes large integers correctly', async () => {
      // A couple of large operation field values representing large 64 bit
      // signed integer and float numbers
      const LARGE_I64 = '8932198321983219';
      const LARGE_I64_NEGATIVE = '-8932198321983219';
      const LARGE_F64 = Number.MAX_VALUE;
      const LARGE_F64_NEGATIVE = Number.MIN_VALUE;

      // Maximum unsigned u64 integer is 18446744073709551615
      const LARGE_LOG_ID = '12345678912345678912';
      const SEQ_NUM = 1;

      const {
        KeyPair,
        OperationFields,
        decodeEntry,
        encodeCreateOperation,
        signEncodeEntry,
      } = await wasm;

      const keyPair = new KeyPair();

      // Use large numbers as operation field values
      const fields = new OperationFields();
      fields.add('large_i64', 'int', LARGE_I64);
      fields.add('large_i64_negative', 'int', LARGE_I64_NEGATIVE);
      fields.add('large_f64', 'float', LARGE_F64);
      fields.add('large_f64_negative', 'float', LARGE_F64_NEGATIVE);

      const operationEncoded = encodeCreateOperation(TEST_SCHEMA, fields);

      // Sign and encode entry with a very high `log_id` value
      const { entryEncoded } = signEncodeEntry(
        keyPair,
        operationEncoded,
        undefined,
        undefined,
        BigInt(SEQ_NUM),
        BigInt(LARGE_LOG_ID),
      );

      const decodedEntry = decodeEntry(entryEncoded, operationEncoded);
      expect(decodedEntry.seqNum).toEqual(BigInt(SEQ_NUM));
      expect(decodedEntry.logId).toEqual(BigInt(LARGE_LOG_ID));

      const { fields: operationFields } = decodedEntry.operation;
      expect(operationFields.get('large_i64').value).toEqual(BigInt(LARGE_I64));
      expect(operationFields.get('large_i64_negative').value).toEqual(
        BigInt(LARGE_I64_NEGATIVE),
      );
      expect(operationFields.get('large_f64').value).toEqual(LARGE_F64);
      expect(operationFields.get('large_f64_negative').value).toEqual(
        LARGE_F64_NEGATIVE,
      );
    });
  });
});

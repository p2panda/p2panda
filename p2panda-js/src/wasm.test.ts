// SPDX-License-Identifier: AGPL-3.0-or-later

import {
  KeyPair,
  OperationFields,
  decodeEntry,
  decodeOperation,
  encodeOperation,
  signAndEncodeEntry,
  verifySignature,
} from './wasm';

const TEST_HASH =
  '0020ddc99aca776df0ca9d1b5871ba39d4edacc752a0a3426b12c3958971b6c847ac';
const TEST_SCHEMA_ID = `test_${TEST_HASH}`;

describe('WebAssembly interface', () => {
  describe('KeyPair', () => {
    it('creates a key pair', () => {
      const keyPair = new KeyPair();
      expect(keyPair.privateKey().length).toBe(64);
    });

    it('restores a key pair', () => {
      const keyPair = new KeyPair();
      const keyPairSecond = KeyPair.fromPrivateKey(keyPair.privateKey());
      expect(keyPair.publicKey()).toBe(keyPairSecond.publicKey());
    });

    it('signs and validates', () => {
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
    it('stores and returns the right fields', () => {
      const fields = new OperationFields();

      // Set fields of all possible types
      fields.insert('description', 'str', 'Hello, Panda');
      fields.insert('temperature', 'int', '32');
      fields.insert('isCute', 'bool', true);
      fields.insert('degree', 'float', 12.322);
      fields.insert('username', 'relation', TEST_HASH);
      fields.insert('locations', 'relation_list', [TEST_HASH]);
      fields.insert('that_one_funny_comment_i_made', 'pinned_relation', [
        TEST_HASH,
      ]);
      fields.insert(
        'those_many_funny_comments_i_made',
        'pinned_relation_list',
        [[TEST_HASH]],
      );

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

    it('returns the correct length', () => {
      const fields = new OperationFields();
      expect(fields.length()).toBe(0);
      fields.insert('message', 'str', 'Good morning');
      expect(fields.length()).toBe(1);
    });

    it('throws when trying to set a field twice', () => {
      const fields = new OperationFields();
      fields.insert('description', 'str', 'Good morning, Panda');
      expect(() =>
        fields.insert('description', 'str', 'Good night, Panda'),
      ).toThrow("field 'description' already exists");
    });

    it('throws when using invalid types or values', () => {
      const fields = new OperationFields();

      // Throw when type is invalid
      expect(() => fields.insert('test', 'lulu', true)).toThrow(
        'Unknown value type',
      );
      expect(() => fields.insert('test', 'int', 'notanumber')).toThrow(
        'Invalid integer value',
      );

      expect(() => fields.insert('contact', 'relation', [TEST_HASH])).toThrow(
        'Expected a document id string for field of type relation',
      );

      expect(() =>
        fields.insert('contact', 'relation_list', TEST_HASH),
      ).toThrow(
        'Expected an array of operation ids for field of type relation list',
      );

      expect(() =>
        fields.insert('contact', 'pinned_relation', TEST_HASH),
      ).toThrow(
        'Expected an array of operation ids for field of type pinned relation',
      );

      expect(() =>
        fields.insert('contact', 'pinned_relation_list', [TEST_HASH]),
      ).toThrow(
        'Expected a nested array of operation ids for field of type pinned relation list',
      );

      // Throw when relation is an invalid hash
      expect(() =>
        fields.insert('contact', 'relation', 'this is not a hash'),
      ).toThrow('Expected a document id string for field of type relation');

      expect(() =>
        fields.insert('contact', 'relation_list', ['this is not a hash']),
      ).toThrow(
        'Expected an array of operation ids for field of type relation list',
      );
    });
  });

  describe('Entries', () => {
    it('creates, signs and decodes an entry', () => {
      const LOG_ID = 5;
      const SEQ_NUM = 1;

      // Generate new key pair
      const keyPair = new KeyPair();

      // Create operation with fields
      const fields = new OperationFields();
      fields.insert('description', 'str', 'Hello, Panda');
      expect(fields.get('description')).toBe('Hello, Panda');

      const operationEncoded = encodeOperation(
        BigInt(0),
        TEST_SCHEMA_ID,
        undefined,
        fields,
      );

      // Sign and encode entry
      const entryEncoded = signAndEncodeEntry(
        BigInt(LOG_ID),
        BigInt(SEQ_NUM),
        undefined,
        undefined,
        operationEncoded,
        keyPair,
      );

      // Decode entry and return as JSON
      const decodedEntry = decodeEntry(entryEncoded);
      expect(decodedEntry.logId).toEqual(BigInt(LOG_ID));
      expect(decodedEntry.seqNum).toEqual(BigInt(SEQ_NUM));
      expect(decodedEntry.backlink).toBeUndefined();
      expect(decodedEntry.skiplink).toBeUndefined();

      // Decode operation
      const plainOperation = decodeOperation(operationEncoded);
      expect(plainOperation.action).toBe(BigInt(0));
      expect(plainOperation.schemaId).toEqual(TEST_SCHEMA_ID);

      // Test operation fields map
      const operationFields = plainOperation.fields;
      expect(operationFields.get('description')).toBe('Hello, Panda');

      // Test decoding entry without operation
      expect(() => decodeEntry(entryEncoded)).not.toThrow();
    });

    it('encodes and decodes large integers correctly', () => {
      // A couple of large operation field values representing large 64 bit
      // signed integer and float numbers
      const LARGE_I64 = '8932198321983219';
      const LARGE_I64_NEGATIVE = '-8932198321983219';
      const LARGE_F64 = Number.MAX_VALUE;
      const LARGE_F64_NEGATIVE = Number.MIN_VALUE;

      // Maximum unsigned u64 integer is 18446744073709551615
      const LARGE_LOG_ID = '12345678912345678912';
      const SEQ_NUM = 1;

      const keyPair = new KeyPair();

      // Use large numbers as operation field values
      const fields = new OperationFields();
      fields.insert('large_i64', 'int', LARGE_I64);
      fields.insert('large_i64_negative', 'int', LARGE_I64_NEGATIVE);
      fields.insert('large_f64', 'float', LARGE_F64);
      fields.insert('large_f64_negative', 'float', LARGE_F64_NEGATIVE);

      const operationEncoded = encodeOperation(
        BigInt(0),
        TEST_SCHEMA_ID,
        undefined,
        fields,
      );

      // Sign and encode entry with a very high `log_id` value
      const entryEncoded = signAndEncodeEntry(
        BigInt(LARGE_LOG_ID),
        BigInt(SEQ_NUM),
        undefined,
        undefined,
        operationEncoded,
        keyPair,
      );

      const decodedEntry = decodeEntry(entryEncoded);
      expect(decodedEntry.seqNum).toEqual(BigInt(SEQ_NUM));
      expect(decodedEntry.logId).toEqual(BigInt(LARGE_LOG_ID));

      const plainOperation = decodeOperation(operationEncoded);
      const { fields: operationFields } = plainOperation;
      expect(operationFields.get('large_i64')).toEqual(BigInt(LARGE_I64));
      expect(operationFields.get('large_i64_negative')).toEqual(
        BigInt(LARGE_I64_NEGATIVE),
      );
      expect(operationFields.get('large_f64')).toEqual(LARGE_F64);
      expect(operationFields.get('large_f64_negative')).toEqual(
        LARGE_F64_NEGATIVE,
      );
    });
  });
});

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
export const TEST_SCHEMA_ID = `test_${TEST_HASH}`;

export function stringToBytes(str: string): number[] {
  const utf8EncodeText = new TextEncoder();
  return Array.from(utf8EncodeText.encode(str));
}

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
      fields.insert('data', 'bytes', [0, 1, 2, 3]);
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
      expect(fields.get('data')).toEqual([0, 1, 2, 3]);
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
    });
  });

  describe('Operations', () => {
    it('encodes and decodes operations', () => {
      // Create operation with fields
      const hash1 =
        '0020da3590fbac19a90bda5618dbcd1799ce6e3bf6e3cd74b7cd41d5d4cb4077af55';
      const hash2 =
        '002020e572c9e4cb884754c047d3d6fec0ff9e700e446cb5f62167575475f7e31bd2';
      const hash3 =
        '00206ebac2127506d3855abc76316299534ee1f695c52e6ac3ae105004b3b968a341';
      const hash4 =
        '002049ed4a0a6cb7308ec13c029e1b559bb1cddccd5c40710cbead900d2fa2ee86c2';
      const hash5 =
        '00206ec876ee8e56acc1e2d5d0d3390f1b02cb4807ade58ca71e885c3943e5287b96';
      const hash6 =
        '0020cf369d24676ba5ae8e74f259f9682607e3e6d01047e31b2b53d3a1cf5f31722e';
      const hash7 =
        '0020fc29afb2f0620bf7417fda043dd13b8e2ef60a47b3f99f47bf8019f68c17411e';

      const fields = new OperationFields();
      fields.insert('a', 'str', 'Hello, Panda!');
      // Int values expected as string
      fields.insert('b', 'int', '123');
      fields.insert('c', 'float', 12.3);
      fields.insert('d', 'bool', true);
      fields.insert('e', 'bytes', new Uint8Array([0, 1, 2, 3]));
      fields.insert('f', 'relation', hash1);
      fields.insert('g', 'pinned_relation', [hash2, hash3]);
      fields.insert('h', 'relation_list', [hash4]);
      fields.insert('i', 'pinned_relation_list', [[hash5], [hash6, hash7]]);

      const operationEncoded = encodeOperation(
        BigInt(0),
        TEST_SCHEMA_ID,
        undefined,
        fields,
      );

      // Decode operation
      const plainOperation = decodeOperation(operationEncoded);
      expect(plainOperation.action).toBe(BigInt(0));
      expect(plainOperation.schemaId).toEqual(TEST_SCHEMA_ID);

      // Test operation fields map
      const operationFields = plainOperation.fields;

      /// String values get decoded as bytes
      expect(operationFields.get('a')).toEqual('Hello, Panda!');
      expect(operationFields.get('b')).toEqual(BigInt(123));
      expect(operationFields.get('c')).toEqual(12.3);
      expect(operationFields.get('d')).toEqual(true);
      expect(operationFields.get('e')).toEqual(new Uint8Array([0, 1, 2, 3]));
      expect(operationFields.get('f')).toEqual(hash1);
      expect(operationFields.get('g')).toEqual([hash2, hash3]);
      expect(operationFields.get('h')).toEqual([hash4]);
      expect(operationFields.get('i')).toEqual([[hash5], [hash6, hash7]]);
    });

    it('encodes and decodes large integers correctly', () => {
      // A couple of large operation field values representing large 64 bit
      // signed integer and float numbers
      const LARGE_I64 = '8932198321983219';
      const LARGE_I64_NEGATIVE = '-8932198321983219';
      const LARGE_F64 = Number.MAX_VALUE;
      const LARGE_F64_NEGATIVE = Number.MIN_VALUE;

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

      const plainOperation = decodeOperation(operationEncoded);
      const { fields: plainFields } = plainOperation;
      expect(plainFields.get('large_i64')).toEqual(BigInt(LARGE_I64));
      expect(plainFields.get('large_i64_negative')).toEqual(
        BigInt(LARGE_I64_NEGATIVE),
      );
      expect(plainFields.get('large_f64')).toEqual(LARGE_F64);
      expect(plainFields.get('large_f64_negative')).toEqual(LARGE_F64_NEGATIVE);
    });
  });
});

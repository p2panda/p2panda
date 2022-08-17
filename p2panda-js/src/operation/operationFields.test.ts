// SPDX-License-Identifier: AGPL-3.0-or-later

import { OperationFields } from './';

describe('OperationFields', () => {
  it('inserts and gets new fields', () => {
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
    fields.insert('a', 'str', 'Hello, World!');
    fields.insert('b', 'int', 123);
    fields.insert('c', 'float', 12.3);
    fields.insert('d', 'bool', true);
    fields.insert('e', 'relation', hash1);
    fields.insert('f', 'pinned_relation', [hash2, hash3]);
    fields.insert('g', 'relation_list', [hash4]);
    fields.insert('h', 'pinned_relation_list', [[hash5], [hash6, hash7]]);

    expect(fields.get('a')).toBe('Hello, World!');
    expect(fields.get('b')).toEqual(BigInt(123));
    expect(fields.get('c')).toEqual(12.3);
    expect(fields.get('d')).toEqual(true);
    expect(fields.get('e')).toEqual(hash1);
    expect(fields.get('f')).toEqual([hash2, hash3]);
    expect(fields.get('g')).toEqual([hash4]);
    expect(fields.get('h')).toEqual([[hash5], [hash6, hash7]]);
  });

  it('handles all sorts of numbers', () => {
    const fields = new OperationFields();
    fields.insert('a', 'int', 123);
    fields.insert('b', 'int', BigInt(123));
    fields.insert('c', 'int', '123');
    fields.insert('d', 'float', 123.5);
    fields.insert('e', 'float', -123.5);

    expect(fields.get('a')).toEqual(BigInt(123));
    expect(fields.get('b')).toEqual(BigInt(123));
    expect(fields.get('c')).toEqual(BigInt(123));
    expect(fields.get('d')).toEqual(123.5);
    expect(fields.get('e')).toEqual(-123.5);
  });

  it('throws when inserting a field twice', () => {
    const fields = new OperationFields();
    fields.insert('test', 'str', 'Hello, World!');

    expect(() => {
      fields.insert('test', 'str', 'Hello, World!');
    }).toThrow(
      "Could not insert new field: Error: field 'test' already exists",
    );
  });

  it('allows an easy way to create fields', () => {
    const fields = new OperationFields({
      a: 'Hallo, Welt!',
      b: 123,
      c: false,
      d: 21.3,
    });

    expect(fields.get('a')).toEqual('Hallo, Welt!');
    expect(fields.get('b')).toEqual(BigInt(123));
    expect(fields.get('c')).toEqual(false);
    expect(fields.get('d')).toEqual(21.3);
  });
});

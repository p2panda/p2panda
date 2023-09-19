// SPDX-License-Identifier: AGPL-3.0-or-later

import { hexToBytes } from '../utils';
import { TEST_SCHEMA_ID } from '../wasm.test';
import { decodeOperation } from './';

describe('decodeOperation', () => {
  it('decodes an encoded operation', () => {
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

    const encodedOperation =
      '8401007849746573745f30303230646463393961636137373664663063613964' +
      '3162353837316261333964346564616363373532613061333432366231326333' +
      '3935383937316236633834376163a961616d48656c6c6f2c20576f726c642161' +
      '62187b6163fb402899999999999a6164f561654400010203616658220020da35' +
      '90fbac19a90bda5618dbcd1799ce6e3bf6e3cd74b7cd41d5d4cb4077af556167' +
      '825822002020e572c9e4cb884754c047d3d6fec0ff9e700e446cb5f621675754' +
      '75f7e31bd2582200206ebac2127506d3855abc76316299534ee1f695c52e6ac3' +
      'ae105004b3b968a3416168815822002049ed4a0a6cb7308ec13c029e1b559bb1' +
      'cddccd5c40710cbead900d2fa2ee86c261698281582200206ec876ee8e56acc1' +
      'e2d5d0d3390f1b02cb4807ade58ca71e885c3943e5287b968258220020cf369d' +
      '24676ba5ae8e74f259f9682607e3e6d01047e31b2b53d3a1cf5f31722e582200' +
      '20fc29afb2f0620bf7417fda043dd13b8e2ef60a47b3f99f47bf8019f68c1741' +
      '1e';

    const result = decodeOperation(encodedOperation);

    expect(result.action).toBe('create');
    expect(result.version).toBe(1);
    expect(result.schemaId).toBe(TEST_SCHEMA_ID);
    expect(result.previous).toBeUndefined;
    expect(result.fields?.get('a')).toEqual('Hello, World!');
    expect(result.fields?.get('b')).toEqual(BigInt(123));
    expect(result.fields?.get('c')).toEqual(12.3);
    expect(result.fields?.get('d')).toEqual(true);
    expect(result.fields?.get('e')).toEqual(new Uint8Array([0, 1, 2, 3]));
    // The hash of a relation on plain operation field is encoded to bytes.
    expect(result.fields?.get('f')).toEqual(hexToBytes(hash1));
    expect(result.fields?.get('g')).toEqual([hash2, hash3]);
    expect(result.fields?.get('h')).toEqual([hash4]);
    expect(result.fields?.get('i')).toEqual([[hash5], [hash6, hash7]]);
  });

  it('throws when decoding an invalid operation', () => {
    // Invalid schema id
    expect(() => {
      decodeOperation(
        '84010063626C61A1646E616D6570416D616E697461206361657361726561',
      );
    }).toThrow(
      "Could not decode operation: malformed schema id `bla`: doesn't contain an underscore",
    );
  });
});

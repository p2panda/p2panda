// SPDX-License-Identifier: AGPL-3.0-or-later

import { TEST_SCHEMA_ID, stringToBytes } from '../wasm.test';
import { decodeOperation } from './';

describe('decodeOperation', () => {
  it('decodes an encoded operation', () => {
    const encodedOperation =
      '8401007849746573745f3030323064646339396163613737366466306361396431623538373162613339643465646163633735326130613334323662313263333935383937316236633834376163a2636167650c6b6465736372697074696f6e4c48656c6c6f2c2050616e6461';

    const result = decodeOperation(encodedOperation);

    expect(result.action).toBe('create');
    expect(result.version).toBe(1);
    expect(result.schemaId).toBe(TEST_SCHEMA_ID);
    expect(result.previous).toBeUndefined;
    expect(result.fields?.get('description')).toEqual(
      stringToBytes('Hello, Panda'),
    );
    expect(result.fields?.get('age')).toEqual(BigInt(12));
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

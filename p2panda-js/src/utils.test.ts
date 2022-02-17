// SPDX-License-Identifier: AGPL-3.0-or-later

/* eslint-disable @typescript-eslint/ban-ts-comment */

import { marshallRequestFields, marshallResponseFields } from '~/utils';
import { Fields, FieldsTagged } from '~/types';

const REQUEST: Fields = {
  channel: 5,
  operation: 'chin chin',
  serious: false,
  temperature: 19.5,
  address: {
    document:
      '0020221e18642f8f9d2ba451e2c4423e370d53e8cc5c6316a6edb4a5821c0c5fc738',
    document_view: [
      '0020632d5603b8f2657a00dd315a16b5ce5208e00afa18a0d22dd0381254e8d6c0e1',
      '0020577aa145b275f54d61c56a0b9dc9efae0730656aee5a3cdb0b981a44927af141',
    ],
  },
};

const RESPONSE: FieldsTagged = new Map();

RESPONSE.set('channel', {
  value: '5',
  type: 'int',
});

RESPONSE.set('operation', {
  value: 'chin chin',
  type: 'str',
});

RESPONSE.set('serious', {
  value: false,
  type: 'bool',
});

RESPONSE.set('temperature', {
  value: 19.5,
  type: 'float',
});

RESPONSE.set('address', {
  value: {
    document:
      '0020221e18642f8f9d2ba451e2c4423e370d53e8cc5c6316a6edb4a5821c0c5fc738',
    document_view: [
      '0020632d5603b8f2657a00dd315a16b5ce5208e00afa18a0d22dd0381254e8d6c0e1',
      '0020577aa145b275f54d61c56a0b9dc9efae0730656aee5a3cdb0b981a44927af141',
    ],
  },
  type: 'relation',
});

const LARGE_NUMBER_REQUEST: Fields = {
  largeNumber: BigInt('894328732428428423810'),
};

const LARGE_NUMBER_RESPONSE: FieldsTagged = new Map();

LARGE_NUMBER_RESPONSE.set('largeNumber', {
  value: '894328732428428423810',
  type: 'int',
});

describe('Utils', () => {
  describe('marshallRequestFields', () => {
    it("creates aquadoggo's expected request format", () => {
      expect(marshallRequestFields(REQUEST)).toEqual(RESPONSE);

      // Large number passed as 'BigInt' will be converted to 'int' type
      expect(marshallRequestFields(LARGE_NUMBER_REQUEST)).toEqual(
        LARGE_NUMBER_RESPONSE,
      );
    });

    it('fails when using wrong relation fields', () => {
      // Missing fields
      expect(() =>
        marshallRequestFields({
          invalid: {
            // @ts-ignore: We deliberately use the API wrong here
            wrong_field:
              '0020577aa145b275f54d61c56a0b9dc9efae0730656aee5a3cdb0b981a44927af141',
          },
        }),
      ).toThrow();

      // Missing `document_view` field
      expect(() =>
        marshallRequestFields({
          // @ts-ignore: We deliberately use the API wrong here
          invalid: {
            document:
              '0020577aa145b275f54d61c56a0b9dc9efae0730656aee5a3cdb0b981a44927af141',
          },
        }),
      ).toThrow();

      // Empty array
      expect(() =>
        marshallRequestFields({
          // @ts-ignore: We deliberately use the API wrong here
          invalid: [],
        }),
      ).toThrow();
    });
  });

  describe('marshallResponseFields', () => {
    it("handles aquadoggo's response format", () => {
      expect(marshallResponseFields(RESPONSE)).toEqual(REQUEST);

      // Large numbers will be returned as strings
      expect(marshallResponseFields(LARGE_NUMBER_RESPONSE)).toEqual({
        largeNumber: LARGE_NUMBER_REQUEST.largeNumber.toString(),
      });
    });
  });
});

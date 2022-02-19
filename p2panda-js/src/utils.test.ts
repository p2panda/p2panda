// SPDX-License-Identifier: AGPL-3.0-or-later

import { marshallRequestFields, marshallResponseFields } from '~/utils';
import { Fields, FieldsTagged } from '~/types';

const REQUEST: Fields = {
  channel: 5,
  operation: 'chin chin',
  serious: false,
  temperature: 19.5,
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

// SPDX-License-Identifier: AGPL-3.0-or-later

/* eslint-disable @typescript-eslint/ban-ts-comment */

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
  int: '5',
});

RESPONSE.set('operation', {
  str: 'chin chin',
});

RESPONSE.set('serious', {
  bool: false,
});

RESPONSE.set('temperature', {
  float: 19.5,
});

const LARGE_NUMBER_REQUEST: Fields = {
  largeNumber: BigInt('894328732428428423810'),
};

const LARGE_NUMBER_RESPONSE: FieldsTagged = new Map();

LARGE_NUMBER_RESPONSE.set('largeNumber', {
  int: '894328732428428423810',
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

    it('ignores undefined fields', () => {
      const request: Fields = {
        channel: 5,
        // @ts-ignore
        username: undefined,
      };

      const result = marshallRequestFields(request);
      expect(result.size).toBe(1);
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

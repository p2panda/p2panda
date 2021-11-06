// SPDX-License-Identifier: AGPL-3.0-or-later

import { marshallRequestFields, marshallResponseFields } from '~/utils';
import { Fields, FieldsTagged } from '~/types';

const REQUEST: Fields = {
  channel: 5,
  message: 'chin chin',
  serious: false,
};

const RESPONSE: FieldsTagged = {
  channel: {
    value: 5,
    type: 'int',
  },
  message: {
    value: 'chin chin',
    type: 'str',
  },
  serious: {
    value: false,
    type: 'bool',
  },
};

describe('Utils', () => {
  describe('marshallRequestFields', () => {
    it("creates aquadoggo's expected request format", () => {
      expect(marshallRequestFields(REQUEST)).toEqual(RESPONSE);
    });
  });
  describe('marshallResponseFields', () => {
    it("handles aquadoggo's response format", () => {
      expect(marshallResponseFields(RESPONSE)).toEqual(REQUEST);
    });
  });
});

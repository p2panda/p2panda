// SPDX-License-Identifier: AGPL-3.0-or-later

import { createKeyPair } from '~/identity';
import { getOperationFields } from '~/operation';
import { marshallRequestFields, marshallResponseFields } from '~/utils';
import wasm from '~/wasm';

describe('operation', () => {
  describe('getOperationFields', () => {
    it('creates a WebAssembly OperationField', async () => {
      const fields = marshallRequestFields({
        channel: 5,
        temperature: 12.921,
        message: 'chin chin',
        serious: false,
      });

      const operationFields = await getOperationFields(fields);

      const outputRepresentation =
        'OperationFields(OperationFields({"channel": Integer(5), "message": ' +
        'Text("chin chin"), "serious": Boolean(false), "temperature": Float(12.921)}))';
      expect(operationFields.toString()).toEqual(outputRepresentation);
    });
  });
});

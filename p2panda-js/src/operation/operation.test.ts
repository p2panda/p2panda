// SPDX-License-Identifier: AGPL-3.0-or-later

import { Session } from '~/session';
import { getOperationFields } from '~/operation';
import { marshallRequestFields } from '~/utils';

describe('message', () => {
  describe('getOperationFields', () => {
    it('creates a web assembly OperationField', async () => {
      const fields = marshallRequestFields({
        channel: 5,
        message: 'chin chin',
        serious: false,
      });
      const outputRepresentation =
        'OperationFields(OperationFields({"channel": Integer(5), "message": ' +
        'Text("chin chin"), "serious": Boolean(false)}))';

      const session = new Session('http://localhost:2020');
      const operationFields = await getOperationFields(session, fields);
      expect(operationFields.toString()).toEqual(outputRepresentation);
    });
  });
});

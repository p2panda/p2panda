// SPDX-License-Identifier: AGPL-3.0-or-later

import { Session } from '~/session';
import { getMessageFields } from '~/message';
import { marshallRequestFields } from '~/utils';

describe('message', () => {
  describe('getMessageFields', () => {
    it('creates a web assembly MessageField', async () => {
      const fields = marshallRequestFields({
        channel: 5,
        message: 'chin chin',
        serious: false,
      });
      const outputRepresentation =
        'MessageFields(MessageFields({"channel": Integer(5), "message": ' +
        'Text("chin chin"), "serious": Boolean(false)}))';

      const session = new Session('http://localhost:2020');
      const messageFields = await getMessageFields(session, fields);
      expect(messageFields.toString()).toEqual(outputRepresentation);
    });
  });
});

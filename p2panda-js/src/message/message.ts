// SPDX-License-Identifier: AGPL-3.0-or-later

import debug from 'debug';

import { Session } from '~/index';
import { FieldsTagged } from '~/types';

import { MessageFields } from 'wasm';

const log = debug('p2panda-js:message');

/**
 * Returns a message fields instance for the given field contents and schema.
 */
export const getMessageFields = async (
  session: Session,
  fields: FieldsTagged,
): Promise<MessageFields> => {
  const { MessageFields } = await session.loadWasm();

  const messageFields = new MessageFields();
  for (const k of Object.keys(fields)) {
    messageFields.add(k, fields[k]['type'], fields[k]['value']);
  }
  log('getMessageFields', messageFields.toString());
  return messageFields;
};

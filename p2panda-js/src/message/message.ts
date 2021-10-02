// SPDX-License-Identifier: AGPL-3.0-or-later

import debug from 'debug';

import wasm from '~/wasm';
import { FieldsTagged } from '~/types';
import { Session } from '~/index';

import type { MessageFields } from 'wasm';

const log = debug('p2panda-js:message');

/**
 * Returns a message fields instance for the given field contents and schema.
 */
export const getMessageFields = async (
  session: Session,
  fields: FieldsTagged,
): Promise<MessageFields> => {
  const { MessageFields } = await wasm;

  const messageFields = new MessageFields();
  for (const k of Object.keys(fields)) {
    messageFields.add(k, fields[k]['type'], fields[k]['value']);
  }
  log('getMessageFields', messageFields.toString());
  return messageFields;
};

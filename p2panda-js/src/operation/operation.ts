// SPDX-License-Identifier: AGPL-3.0-or-later

import debug from 'debug';

import { OperationFields } from '../wasm';

import type { FieldsTagged } from '../types';

const log = debug('p2panda-js:operation');

/**
 * Returns an operation fields instance for the given field contents and schema.
 */
export const getOperationFields = (fields: FieldsTagged): OperationFields => {
  const operationFields = new OperationFields();

  for (const [key, fieldValue] of fields.entries()) {
    const { type, value } = fieldValue;
    operationFields.insert(key, type, value);
  }

  log('getOperationFields', operationFields.toString());
  return operationFields;
};

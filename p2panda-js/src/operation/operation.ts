// SPDX-License-Identifier: AGPL-3.0-or-later

import debug from 'debug';

import wasm from '~/wasm';

import type { FieldsTagged } from '~/types';
import type { OperationFields } from 'wasm';

const log = debug('p2panda-js:operation');

/**
 * Returns an operation fields instance for the given field contents and schema.
 */
export const getOperationFields = async (
  fields: FieldsTagged,
): Promise<OperationFields> => {
  const { OperationFields } = await wasm;

  const operationFields = new OperationFields();
  for (const [key, fieldValue] of fields.entries()) {
    const type = Object.keys(fieldValue).pop();
    if (type == null) {
      throw new Error(`Missing value for field '${key}'`);
    }
    const value = fieldValue[type as keyof typeof fieldValue];
    operationFields.add(key, type, value);
  }

  log('getOperationFields', operationFields.toString());
  return operationFields;
};

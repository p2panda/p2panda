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
  for (const k of Object.keys(fields)) {
    operationFields.add(k, fields[k]['type'], fields[k]['value']);
  }
  log('getOperationFields', operationFields.toString());
  return operationFields;
};

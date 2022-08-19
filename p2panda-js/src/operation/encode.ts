// SPDX-License-Identifier: AGPL-3.0-or-later

import * as wasm from '../wasm';
import { OPERATION_ACTIONS } from './constants';
import { OperationFields } from './operationFields';
import { validate } from '../validate';

import type { EasyValues, OperationAction, OperationMeta } from './';

/**
 * Arguments to create an operation.
 */
export type OperationArgs = OperationMeta & {
  /** Operation action, default is CREATE */
  action?: OperationAction;

  /** Fields with user data, needs to be set for CREATE and UPDATE operations */
  fields?: OperationFields | EasyValues;
};

/**
 * Creates and encodes an p2panda operation.
 * @param {OperationArgs} operation - Arguments to create the operation
 * @returns Hexadecimal encoded operation
 */
export function encodeOperation(operation: OperationArgs): string {
  validate({ operation }, { operation: { type: 'object' } });

  const {
    action = 'create',
    schemaId,
    previousOperations = undefined,
    fields = undefined,
  } = operation;

  validate(
    {
      action,
      schemaId,
    },
    {
      action: {
        type: 'string',
      },
      schemaId: {
        type: 'string',
      },
    },
  );

  if (!['create', 'update', 'delete'].includes(action)) {
    throw new Error(`Unknown operation action "${action}"`);
  }

  // We can pass `OperationFields` instance or the easy fields (simple object)
  // for our convenience
  let operationFields;
  if (fields !== undefined) {
    if (fields instanceof OperationFields) {
      operationFields = fields.__internal;
    } else {
      operationFields = new OperationFields(fields).__internal;
    }
  }

  try {
    return wasm.encodeOperation(
      BigInt(OPERATION_ACTIONS[action]),
      schemaId,
      previousOperations,
      operationFields,
    );
  } catch (error) {
    throw new Error(`Could not encode operation: ${(error as Error).message}`);
  }
}

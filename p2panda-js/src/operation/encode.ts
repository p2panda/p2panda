// SPDX-License-Identifier: AGPL-3.0-or-later

import wasm from '../wasm';
import { OperationFields } from './';
import { validate } from '../validate';

import type { EasyFields } from './operationFields';

const OPERATION_ACTIONS = {
  create: 0,
  update: 1,
  delete: 2,
};

/**
 * Arguments to create an operation.
 */
type OperationArgs = {
  // Operation action
  action?: 'create' | 'update' | 'delete';

  // Schema id
  schemaId: string;

  // Document view id pointing at previous operations
  previousOperations?: string[];

  // Fields
  fields?: OperationFields | EasyFields;
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

  // We can pass `OperationFields` instance or the easy fields (simple object) for convenience
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
    throw new Error(`Could not encode operation: ${error}`);
  }
}

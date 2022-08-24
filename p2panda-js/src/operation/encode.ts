// SPDX-License-Identifier: AGPL-3.0-or-later

import * as wasm from '../wasm';
import { OPERATION_ACTIONS } from './constants';
import { OperationFields } from './operationFields';
import { validate } from '../validate';

import type { EasyValues, OperationAction } from './';

/**
 * Arguments to create an operation.
 */
export type OperationArgs = {
  /** Id of schema this operation matches */
  schemaId: string;

  /** Document view id pointing at previous operations, needs to be set
   * for UPDATE and DELETE operations */
  previous?: string[] | string;

  /** Operation action, default is CREATE */
  action?: OperationAction;

  /** Fields with user data, needs to be set for CREATE and UPDATE operations */
  fields?: OperationFields | EasyValues;
};

/**
 * Creates and encodes an p2panda operation.
 * @param {OperationArgs} operation - Arguments to create the operation
 * @returns Hexadecimal encoded operation
 * @example
 * ```
 * import { encodeOperation } from 'p2panda-js';
 *
 * const result = encodeOperation({
 *   action: 'update',
 *   schemaId: 'venues_0020c9db3376fa753b041e199ebfe1c0e6dfb50ca7924c7eedfdd35f141ac8d1207c',
 *   previous: '00205f00bd1174909d6f7060800f3b9969e433dd564f9b75772d202f6ea48e5415e0',
 *   fields: {
 *     name: 'Untergruen',
 *   },
 * });
 * ```
 */
export function encodeOperation(operation: OperationArgs): string {
  validate({ operation }, { operation: { type: 'object' } });

  const { action = 'create', schemaId, fields = undefined } = operation;

  let previous: string[] | undefined = undefined;
  if (typeof operation.previous === 'string') {
    // Automatically convert `viewId` string into array with operation ids. As
    // view ids come usually as strings from the GraphQL API, this conversion
    // can be quite convenient!
    previous = operation.previous.split('_');
  } else if (typeof operation.previous === 'object') {
    previous = operation.previous;
  }

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
      previous,
      operationFields,
    );
  } catch (error) {
    throw new Error(`Could not encode operation: ${(error as Error).message}`);
  }
}

// SPDX-License-Identifier: AGPL-3.0-or-later

import * as wasm from '../wasm';
import { OPERATION_ACTIONS_INDEX } from './constants';
import { validate } from '../validate';

import type { OperationMeta, OperationAction, OperationValue } from './';

/**
 * Plain operation with fields which have not been checked against a schema
 * yet.
 */
export type PlainOperation = OperationMeta & {
  /** Version of this operation encoding */
  version: number;

  /** Operation action */
  action: OperationAction;

  /** Plain fields which have not been checked against a schema yet */
  fields?: Map<string, OperationValue>;
};

/**
 * Decodes an p2panda operation.
 * @param {string} encodedOperation - Hexadecimal string of an encoded operation
 * @returns {PlainOperation} Plain operation which has not been checked against
 * a schema yet
 * @example
 * ```
 * import { decodeOperation } from 'p2panda-js';
 *
 * const encodedOperation =
 *   '840100781a736368656d615f6669656c645f646566696e697469' +
 *   '6f6e5f7631a2646e616d656576656e7565647479706563737472';
 *
 * const result = decodeOperation(encodedOperation);
 * console.log(result.action); // "create"
 * ```
 */
export function decodeOperation(encodedOperation: string): PlainOperation {
  validate(
    {
      encodedOperation,
    },
    {
      encodedOperation: {
        validHex: true,
      },
    },
  );

  try {
    const result = wasm.decodeOperation(encodedOperation);

    const plainOperation: PlainOperation = {
      // Convert version to 'number'
      version: Number(result.version),
      // Translate operation action to human readable string
      action: OPERATION_ACTIONS_INDEX[Number(result.action)] as OperationAction,
      schemaId: result.schemaId,
    };

    if (result.previous) {
      plainOperation.previous = result.previous;
    }

    if (result.fields) {
      plainOperation.fields = result.fields;
    }

    return plainOperation;
  } catch (error) {
    throw new Error(`Could not decode operation: ${(error as Error).message}`);
  }
}

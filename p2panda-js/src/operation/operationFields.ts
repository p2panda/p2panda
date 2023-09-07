// SPDX-License-Identifier: AGPL-3.0-or-later

import * as wasm from '../wasm';
import { validate } from '../validate';
import { isInt, isFloat } from '../utils';

import type { OperationValue, EasyValues } from './';

/**
 * Field type string as defined by the p2panda specification indicating
 * the operation value type.
 */
export type FieldType =
  | 'str'
  | 'int'
  | 'float'
  | 'bool'
  | 'bytes'
  | 'relation'
  | 'pinned_relation'
  | 'relation_list'
  | 'pinned_relation_list';

/**
 * Allow 'number' when inserting new operation values.
 */
export type OperationValueArg = OperationValue | number;

/**
 * Operation fields containing application data.
 * @example
 * ```
 * import { OperationFields, encodeOperation } from 'p2panda-js';
 *
 * const fields = new OperationFields({
 *   name: 'Panda Party!',
 * });
 *
 * fields.insert(
 *   'venue',
 *   'relation',
 *   '002078619bd4beff4bec4d4ccf75a7a5c25bf5d3c37fbd051a45debf1f17a1f75230'
 * );
 *
 * const operation = encodeOperation({
 *   schemaId: 'events_00203ebb383f195f2923d51ac9929a8dbf7fba65dcddff874dfbe5ef131362696636',
 *   fields,
 * });
 * ```
 */
export class OperationFields {
  /**
   * @internal
   */
  readonly __internal: wasm.OperationFields;

  /**
   * Creates a new instance of `OperationFields`.
   * @param {EasyValues?} fields - "Easy field values" to populate the
   * operation with basic data types. This can be used to easily create
   * operation fields, even when there is no schema at hand. Please note that
   * only unambigious field types like "str", "int", "float" and "bool" can be
   * used here
   * @returns OperationFields instance
   * @example
   * ```
   * import { OperationFields } from 'p2panda-js';
   *
   * const fields = new OperationFields({
   *   hasDate: true,
   *   latestYear: 2002,
   * });
   * ```
   */
  constructor(fields?: EasyValues) {
    const operationFields = new wasm.OperationFields();

    // We can pass in "easy fields" into the constructor to allow the fast
    // creation of unambigious field types, meaning that we can guess the field
    // type even without a schema at hand.
    //
    // Integers are converted to strings to be able to pass large numbers into
    // wasm context
    if (fields) {
      Object.keys(fields).forEach((fieldName) => {
        const value = fields[fieldName];

        if (typeof value === 'string') {
          operationFields.insert(fieldName, 'str', value);
        } else if (typeof value === 'boolean') {
          operationFields.insert(fieldName, 'bool', value);
        } else if (typeof value === 'number' && isInt(value)) {
          operationFields.insert(fieldName, 'int', value.toString());
        } else if (typeof value === 'bigint') {
          operationFields.insert(fieldName, 'int', value.toString());
        } else if (typeof value === 'number' && isFloat(value)) {
          operationFields.insert(fieldName, 'float', value);
        } else {
          throw new Error(
            `Only basic field types like "str", "bool", "int" and "float" are allowed when using constructor`,
          );
        }
      });
    }

    this.__internal = operationFields;
  }

  /**
   * Inserts a new field.
   * @param {string} fieldName - Name of the field, needs to match schema
   * @param {FieldType} fieldType - Operation field type
   * @param {OperationValue} value - Actual user data
   * @example
   * ```
   * import { OperationFields } from 'p2panda-js';
   *
   * const fields = new OperationFields();
   * fields.insert('venue', 'relation', '002078619bd4beff4bec4d4ccf75a7a5c25bf5d3c37fbd051a45debf1f17a1f75230');
   * ```
   */
  insert(fieldName: string, fieldType: FieldType, value: OperationValueArg) {
    validate(
      {
        fieldName,
        fieldType,
      },
      {
        fieldName: {
          type: 'string',
        },
        fieldType: {
          type: 'string',
        },
      },
    );

    try {
      if (fieldType === 'int') {
        // Integers are passed to WebAssembly as a string to allow
        // representation of large (i64) numbers
        this.__internal.insert(fieldName, fieldType, value.toString());
      } else {
        this.__internal.insert(fieldName, fieldType, value);
      }
    } catch (error) {
      throw new Error(
        `Could not insert new field: ${(error as Error).message}`,
      );
    }
  }

  /**
   * Gets a value from a field.
   * @param {string} fieldName - Name of the field, needs to match schema
   * @returns {OperationValue} User data
   */
  get(fieldName: string): OperationValue {
    validate(
      {
        fieldName,
      },
      {
        fieldName: {
          type: 'string',
        },
      },
    );

    const value = this.__internal.get(fieldName);
    return value;
  }

  /**
   * Returns the number of fields.
   * @returns {number}
   */
  length(): number {
    return this.__internal.length();
  }

  /**
   * Returns true when there are no fields given.
   * @returns {boolean}
   */
  isEmpty(): boolean {
    return this.__internal.isEmpty();
  }
}

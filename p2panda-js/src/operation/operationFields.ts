// SPDX-License-Identifier: AGPL-3.0-or-later

import wasm from '../wasm';
import { validate } from '../validate';
import { isInt, isFloat } from '../utils';

type FieldType =
  | 'str'
  | 'int'
  | 'float'
  | 'bool'
  | 'relation'
  | 'pinned_relation'
  | 'relation_list'
  | 'pinned_relation_list';

type OperationValue =
  | string
  | bigint
  | number
  | boolean
  | string[]
  | string[][];

/*
 * "Easy fields" to populate the operation with basic data types.
 *
 * This can be used to easily create operation fields, even when there is no
 * schema at hand. Please note that only unambigious field types like "str",
 * "int", "float" and "bool" can be used here
 */
export type EasyFields = {
  [fieldName: string]: string | number | bigint | boolean;
};

/**
 * Operation fields containing application data.
 */
export class OperationFields {
  readonly __internal: wasm.OperationFields;

  /**
   * Creates a new instance of `OperationFields`.
   * @param {EasyFields?} fields - "Easy fields" to populate the operation with
   * basic data types. This can be used to easily create operation fields, even
   * when there is no schema at hand. Please note that only unambigious field
   * types like "str", "int", "float" and "bool" can be used here
   * @returns OperationFields instance
   */
  constructor(fields?: EasyFields) {
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
   */
  insert(fieldName: string, fieldType: FieldType, value: OperationValue) {
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
      throw new Error(`Could not insert new field: ${error}`);
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

    return this.__internal.get(fieldName);
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

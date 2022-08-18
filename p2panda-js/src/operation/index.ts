// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * Operation actions, represented as strings.
 */
export type OperationAction = 'create' | 'update' | 'delete';

/**
 * Meta fields which are given next to `action` and `version`.
 */
export type OperationMeta = {
  /** Id of schema this operation matches */
  schemaId: string;

  /** Document view id pointing at previous operations, needs to be set
   * for UPDATE and DELETE operations */
  previousOperations?: string[];
};

/**
 * Possible operation values.
 */
export type OperationValue = string | bigint | boolean | string[] | string[][];

/**
 * "Easy operation values" to populate the operation with basic data types.
 *
 * This can be used to easily create operation fields, even when there is no
 * schema at hand. Please note that only unambigious field types like "str",
 * "int", "float" and "bool" can be used here
 */
export type EasyValues = {
  [fieldName: string]: string | number | bigint | boolean;
};

export { OperationFields } from './operationFields';
export { decodeOperation } from './decode';
export { encodeOperation } from './encode';

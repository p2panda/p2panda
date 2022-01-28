// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * Arguments for publishing the next entry.
 */
export type EntryArgs = {
  entryHashSkiplink: string | undefined;
  entryHashBacklink: string | undefined;
  seqNum: string;
  logId: string;
};

/**
 * Entry record received from aquadoggo.
 */
export type EncodedEntry = {
  author: string;
  entryBytes: string;
  entryHash: string;
  logId: BigInt;
  payloadBytes: string;
  payloadHash: string;
  seqNum: BigInt;
};

/**
 * Entry record from aquadoggo with decoded `Entry`.
 */
export type EntryRecord = Entry & {
  encoded: EncodedEntry;
};

/**
 * Decoded entry containing optional `Operation`.
 */
export type Entry = {
  entryHashBacklink: string | undefined;
  entryHashSkiplink: string | undefined;
  logId: BigInt;
  operation: Operation | undefined;
  seqNum: BigInt;
};

/**
 * Decoded form of an operation, which can create, update or delete documents.
 */
export type Operation = {
  action: 'create' | 'update' | 'delete';
  schema: string;
  previousOperations?: string[];
  fields?: Fields;
  id?: string;
};

/**
 * Object containing operation field values.
 */
export type Fields = {
  [fieldname: string]: boolean | number | string | BigInt;
};

/**
 * Decoded entry containing optional `Operation`.
 */
export type EntryTagged = {
  entryHashBacklink: string | undefined;
  entryHashSkiplink: string | undefined;
  logId: BigInt;
  operation: OperationTagged | undefined;
  seqNum: BigInt;
};

/**
 * Decoded form of an operation, which can create, update or delete documents.
 */
export type OperationTagged = {
  action: 'create' | 'update' | 'delete';
  previousOperations?: string[];
  schema: string;
  fields: FieldsTagged;
};

/**
 * Object containing operation fields in tagged form.
 */
export type FieldsTagged = Map<string, OperationValue>;

export type OperationValue =
  | OperationValueText
  | OperationValueBool
  | OperationValueInt
  | OperationValueFloat;

/**
 * An operation value of `boolean` type.
 */
export type OperationValueBool = {
  value: boolean;
  type: 'bool';
};

/**
 * An operation value of `integer` type.
 */
export type OperationValueInt = {
  // Internally stored as a string to give support for very large numbers
  value: string;
  type: 'int';
};

/**
 * An operation value of `float` type.
 */
export type OperationValueFloat = {
  value: number;
  type: 'float';
};

/**
 * An operation value of `string` type.
 */
export type OperationValueText = {
  value: string;
  type: 'str';
};

/**
 * A materialised instance item with meta data.
 */
export type InstanceRecord = Record<
  string,
  boolean | number | string | unknown
> & {
  _meta: {
    id: string;
    author: string;
    deleted: boolean;
    edited: boolean;
    entries: EntryRecord[];
    schema: string;
    // The tip of the operation graph which produced this instance.
    last_operation: string;
  };
};

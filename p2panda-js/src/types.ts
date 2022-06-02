// SPDX-License-Identifier: AGPL-3.0-or-later

export type SchemaId =
  | 'schema_definition_v1'
  | 'schema_field_definition_v1'
  | string;

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
  schema: SchemaId;
  previous_operations?: string[];
  fields?: Fields;
  id?: string;
};

/**
 * Object containing operation field values.
 */
export type Fields = {
  [fieldname: string]:
    | boolean
    | number
    | string
    | BigInt
    | Relation
    | Relation[]
    | PinnedRelation
    | PinnedRelation[];
};

/**
 * Relation pointing at a document id.
 */
export type Relation = string;

/**
 * Pinned relation pointing at a document view id.
 */
export type PinnedRelation = string[];

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
  previous_operations?: string[];
  schema: SchemaId;
  fields: FieldsTagged;
};

/**
 * Object containing operation fields in tagged form.
 */
export type FieldsTagged = Map<string, OperationValue>;

export type OperationValue =
  | OperationValueBool
  | OperationValueFloat
  | OperationValueInt
  | OperationValueRelation
  | OperationValueRelationList
  | OperationValueText;

/**
 * An operation value of `boolean` type.
 */
export type OperationValueBool = {
  bool: boolean;
};

/**
 * An operation value of `integer` type.
 */
export type OperationValueInt = {
  // Internally stored as a string to give support for very large numbers
  int: string;
};

/**
 * An operation value of `float` type.
 */
export type OperationValueFloat = {
  float: number;
};

/**
 * An operation value of `string` type.
 */
export type OperationValueText = {
  str: string;
};

/**
 * An operation value of `relation` type.
 */
export type OperationValueRelation = {
  relation: Relation;
};

/**
 * An operation value of `relation_list` type.
 */
export type OperationValueRelationList = {
  relation_list: Relation[];
};
/**
 * An operation value of `pinned_relation` type.
 */
export type OperationValuePinnedRelation = {
  pinned_relation: PinnedRelation;
};

/**
 * An operation value of `pinned_relation_list` type.
 */
export type OperationValuePinnedRelationList = {
  pinned_relation_list: PinnedRelation[];
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
    schema: SchemaId;
    // The tip of the operation graph which produced this instance.
    last_operation: string;
  };
};

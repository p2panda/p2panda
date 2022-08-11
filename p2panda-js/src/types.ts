// SPDX-License-Identifier: AGPL-3.0-or-later

export type SchemaId =
  | 'schema_definition_v1'
  | 'schema_field_definition_v1'
  | string;

/**
 * Arguments for publishing the next entry.
 */
export type NextArgs = {
  skiplink: string | undefined;
  backlink: string | undefined;
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
  logId: bigint;
  payloadBytes: string;
  payloadHash: string;
  seqNum: bigint;
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
  backlink: string | undefined;
  skiplink: string | undefined;
  logId: bigint;
  operation: Operation | undefined;
  seqNum: bigint;
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
    | bigint
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
  backlink: string | undefined;
  skiplink: string | undefined;
  logId: bigint;
  operation: OperationTagged | undefined;
  seqNum: bigint;
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
 * An operation value of `relation` type.
 */
export type OperationValueRelation = {
  value: Relation;
  type: 'relation';
};

/**
 * An operation value of `relation_list` type.
 */
export type OperationValueRelationList = {
  value: Relation[];
  type: 'relation_list';
};
/**
 * An operation value of `pinned_relation` type.
 */
export type OperationValuePinnedRelation = {
  value: PinnedRelation;
  type: 'pinned_relation';
};

/**
 * An operation value of `pinned_relation_list` type.
 */
export type OperationValuePinnedRelationList = {
  value: PinnedRelation[];
  type: 'pinned_relation_list';
};

// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * Arguments for publishing the next entry
 */
export type EntryArgs = {
  entryHashSkiplink: string | undefined;
  entryHashBacklink: string | undefined;
  seqNum: number;
  logId: number;
};

/**
 * Entry record received from aquadoggo
 */
export type EncodedEntry = {
  author: string;
  entryBytes: string;
  entryHash: string;
  logId: number;
  payloadBytes: string;
  payloadHash: string;
  seqNum: number;
};

/**
 * Entry record from aquadoggo with decoded `Entry`
 */
export type EntryRecord = Entry & {
  encoded: EncodedEntry;
};

/**
 * Decoded entry containing optional `Message`
 */
export type Entry = {
  entryHashBacklink: string | null;
  entryHashSkiplink: string | null;
  logId: number;
  message: Message | null;
  seqNum: number;
};

/**
 * Decoded form of a message, which can create, update or delete instances
 */
export type Message = {
  action: 'create' | 'update' | 'delete';
  schema: string;
  fields: Fields;
};

/**
 * Object containing message field values
 */
export type Fields = {
  [fieldname: string]: boolean | number | string;
};

/**
 * Decoded entry containing optional `Message`
 */
export type EntryTagged = {
  entryHashBacklink: string | null;
  entryHashSkiplink: string | null;
  logId: number;
  message: MessageTagged | null;
  seqNum: number;
};

/**
 * Decoded form of a message, which can create, update or delete instances
 */
export type MessageTagged = {
  action: 'create' | 'update' | 'delete';
  schema: string;
  fields: FieldsTagged;
};

/**
 * Object containing message fields in tagged form
 */
export type FieldsTagged = {
  // currently only a schema with a text message is supported
  [fieldname: string]: MessageValue;
};

export type MessageValue =
  | MessageValueText
  | MessageValueBool
  | MessageValueInt;

/**
 * A message value of `boolean` type
 */
export type MessageValueBool = {
  value: boolean;
  type: 'bool';
};

/**
 * A message value of `number` type, which must be an integer
 */
export type MessageValueInt = {
  value: number;
  type: 'int';
};

/**
 * A message value of `string` type
 */
export type MessageValueText = {
  value: string;
  type: 'str';
};

export type InstanceRecord = Record<
  string,
  boolean | number | string | unknown
> & {
  _meta: {
    author: string;
    deleted: boolean;
    edited: boolean;
    entries: EntryRecord[];
    hash: string;
    schema: string;
  };
};

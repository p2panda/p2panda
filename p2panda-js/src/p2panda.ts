// SPDX-License-Identifier: AGPL-3.0-or-later

export { KeyPair, verifySignature } from './identity';
export { OperationFields, encodeOperation, decodeOperation } from './operation';
export { generateHash } from './hash';
export { hexToBytes, bytesToHex } from './utils';
export { signAndEncodeEntry, decodeEntry } from './entry';

export type { Entry } from './entry/decode';
export type { EntryArgs } from './entry/encode';
export type { OperationArgs } from './operation/encode';
export type {
  EasyValues,
  OperationAction,
  OperationMeta,
  OperationValue,
} from './operation';
export type { PlainOperation } from './operation/decode';
export type { FieldType, OperationValueArg } from './operation/operationFields';

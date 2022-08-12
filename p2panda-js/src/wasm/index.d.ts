/* tslint:disable */
/* eslint-disable */
/**
* Sets a [`panic hook`] for better error messages in NodeJS or web browser.
*
* [`panic hook`]: https://crates.io/crates/console_error_panic_hook
*/
export function setWasmPanicHook(): void;
/**
* Verify the integrity of a signed operation.
* @param {string} public_key
* @param {string} byte_string
* @param {string} signature
* @returns {any}
*/
export function verifySignature(public_key: string, byte_string: string, signature: string): any;
/**
* Returns an encoded CREATE operation that creates a document of the provided schema.
* @param {any} schema_id
* @param {OperationFields} fields
* @returns {string}
*/
export function encodeCreateOperation(schema_id: any, fields: OperationFields): string;
/**
* Returns an encoded UPDATE operation that updates fields of a given document.
* @param {any} schema_id
* @param {any} previous_operations
* @param {OperationFields} fields
* @returns {string}
*/
export function encodeUpdateOperation(schema_id: any, previous_operations: any, fields: OperationFields): string;
/**
* Returns an encoded DELETE operation that deletes a given document.
* @param {any} schema_id
* @param {any} previous_operations
* @returns {string}
*/
export function encodeDeleteOperation(schema_id: any, previous_operations: any): string;
/**
* Returns a signed and encoded entry that can be published to a p2panda node.
*
* `entry_backlink_hash`, `entry_skiplink_hash`, `seq_num` and `log_id` are obtained by querying
* the `getEntryArguments` method of a p2panda node.
* @param {KeyPair} key_pair
* @param {string} encoded_operation
* @param {string | undefined} entry_skiplink_hash
* @param {string | undefined} entry_backlink_hash
* @param {bigint} seq_num
* @param {bigint} log_id
* @returns {any}
*/
export function signEncodeEntry(key_pair: KeyPair, encoded_operation: string, entry_skiplink_hash: string | undefined, entry_backlink_hash: string | undefined, seq_num: bigint, log_id: bigint): any;
/**
* Decodes an entry and optional operation given their encoded form.
* @param {string} entry_str
* @param {string | undefined} operation_str
* @returns {any}
*/
export function decodeEntry(entry_str: string, operation_str?: string): any;
/**
* Ed25519 key pair for authors to sign Bamboo entries with.
*/
export class KeyPair {
  free(): void;
/**
* Generates a new key pair using the browsers random number generator as a seed.
*/
  constructor();
/**
* Derives a key pair from a private key, encoded as hex string for better handling in browser
* contexts.
* @param {string} private_key
* @returns {KeyPair}
*/
  static fromPrivateKey(private_key: string): KeyPair;
/**
* Returns the public half of the key pair, encoded as a hex string.
* @returns {string}
*/
  publicKey(): string;
/**
* Returns the private half of the key pair, encoded as a hex string.
* @returns {string}
*/
  privateKey(): string;
/**
* Sign an operation using this key pair, returns signature encoded as a hex string.
* @param {string} operation
* @returns {string}
*/
  sign(operation: string): string;
}
/**
* Use `OperationFields` to attach application data to an [`Operation`].
*/
export class OperationFields {
  free(): void;
/**
* Returns an `OperationFields` instance.
*/
  constructor();
/**
* Adds a field with a value and a given value type.
*
* The type is defined by a simple string, similar to an enum. Possible type values are:
*
* - "bool" (Boolean)
* - "float" (Number)
* - "int" (Number)
* - "str" (String)
* - "relation" (hex-encoded document id)
* - "relation_list" (array of hex-encoded document ids)
* - "pinned_relation" (document view id, represented as an array
*     of hex-encoded operation ids)
* - "pinned_relation_list" (array of document view ids, represented as an array
*     of arrays of hex-encoded operation ids)
*
* This method will throw an error when the field was already set, an invalid type value got
* passed or when the value does not reflect the given type.
* @param {string} name
* @param {string} value_type
* @param {any} value
*/
  insert(name: string, value_type: string, value: any): void;
/**
* Returns field of this `OperationFields` instance when existing.
* @param {string} name
* @returns {any}
*/
  get(name: string): any;
/**
* Returns the number of fields in this instance.
* @returns {number}
*/
  length(): number;
/**
* Returns true when no field exists.
* @returns {boolean}
*/
  isEmpty(): boolean;
/**
* Returns this instance formatted for debugging.
* @returns {string}
*/
  toString(): string;
}

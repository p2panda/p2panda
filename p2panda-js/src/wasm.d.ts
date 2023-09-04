/* tslint:disable */
/* eslint-disable */
/**
* Returns a signed Bamboo entry.
* @param {bigint} log_id
* @param {bigint} seq_num
* @param {string | undefined} skiplink_hash
* @param {string | undefined} backlink_hash
* @param {string} payload
* @param {KeyPair} key_pair
* @returns {string}
*/
export function signAndEncodeEntry(log_id: bigint, seq_num: bigint, skiplink_hash: string | undefined, backlink_hash: string | undefined, payload: string, key_pair: KeyPair): string;
/**
* Decodes an hexadecimal string into an `Entry`.
* @param {string} encoded_entry
* @returns {any}
*/
export function decodeEntry(encoded_entry: string): any;
/**
* Sets a [`panic hook`] for better error messages in NodeJS or web browser.
*
* [`panic hook`]: https://crates.io/crates/console_error_panic_hook
*/
export function setWasmPanicHook(): void;
/**
* Creates, validates and encodes an operation as hexadecimal string.
* @param {bigint} action
* @param {string} schema_id
* @param {any} previous
* @param {OperationFields | undefined} fields
* @returns {string}
*/
export function encodeOperation(action: bigint, schema_id: string, previous: any, fields?: OperationFields): string;
/**
* Decodes an operation into its plain form.
*
* A plain operation has not been checked against a schema yet.
* @param {string} encoded_operation
* @returns {any}
*/
export function decodeOperation(encoded_operation: string): any;
/**
* Returns hash of an hexadecimal encoded value.
* @param {string} value
* @returns {string}
*/
export function generateHash(value: string): string;
/**
* Verify the integrity of a signed operation.
* @param {string} public_key
* @param {string} byte_string
* @param {string} signature
* @returns {any}
*/
export function verifySignature(public_key: string, byte_string: string, signature: string): any;
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
* Sign any data using this key pair, returns signature encoded as a hex string.
* @param {string} value
* @returns {string}
*/
  sign(value: string): string;
}
/**
* Interface to create, update and retreive values from operation fields.
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
* - "bytes" (Bytes)
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
}
/**
* Interface to create, update and retreive values from operation fields.
*/
export class PlainFields {
  free(): void;
/**
* Returns field value when existing.
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
}

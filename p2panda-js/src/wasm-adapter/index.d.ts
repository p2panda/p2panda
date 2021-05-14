/* tslint:disable */
/* eslint-disable */
/**
 * Sets a [`panic hook`] for better error messages in NodeJS or web browser.
 *
 * [`panic hook`]: https://crates.io/crates/console_error_panic_hook
 */
export function setWasmPanicHook(): void;
/**
 * Returns an encoded `create` message that creates an instance of the provided schema.
 *
 * Use `create` messages by attaching them to an entry that you publish.
 * @param {string} schema_hash
 * @param {MessageFields} fields
 * @returns {string}
 */
export function encodeCreateMessage(
  schema_hash: string,
  fields: MessageFields,
): string;
/**
 * Returns an encoded `update` message that updates fields of a given instance.
 *
 * Use `update` messages by attaching them to an entry that you publish.
 * @param {string} instance_id
 * @param {string} schema_hash
 * @param {MessageFields} fields
 * @returns {string}
 */
export function encodeUpdateMessage(
  instance_id: string,
  schema_hash: string,
  fields: MessageFields,
): string;
/**
 * Returns an encoded `delete` message that deletes a given instance.
 *
 * Use `delete` messages by attaching them to an entry that you publish.
 * @param {string} instance_id
 * @param {string} schema_hash
 * @returns {string}
 */
export function encodeDeleteMessage(
  instance_id: string,
  schema_hash: string,
): string;
/**
 * Returns a signed and encoded entry that can be published to a p2panda node.
 *
 * `entry_backlink_hash`, `entry_skiplink_hash`, `previous_seq_num` and `log_id` are obtained by
 * querying the `getEntryArguments` method of a p2panda node.
 * @param {KeyPair} key_pair
 * @param {string} encoded_message
 * @param {string | undefined} entry_skiplink_hash
 * @param {string | undefined} entry_backlink_hash
 * @param {BigInt | undefined} previous_seq_num
 * @param {BigInt} log_id
 * @returns {any}
 */
export function signEncodeEntry(
  key_pair: KeyPair,
  encoded_message: string,
  entry_skiplink_hash: string | undefined,
  entry_backlink_hash: string | undefined,
  previous_seq_num: BigInt | undefined,
  log_id: BigInt,
): any;
/**
 * Decodes an entry and optional message given their encoded form.
 * @param {string} entry_encoded
 * @param {string | undefined} message_encoded
 * @returns {any}
 */
export function decodeEntry(
  entry_encoded: string,
  message_encoded?: string,
): any;
/**
 * Ed25519 key pair for authors to sign bamboo entries with.
 */
export class KeyPair {
  free(): void;
  /**
   * Generates a new key pair using the systems random number generator (CSPRNG) as a seed.
   *
   * This uses `getrandom` for random number generation internally. See [`getrandom`] crate for
   * supported platforms.
   *
   * **WARNING:** Depending on the context this does not guarantee the random number generator
   * to be cryptographically secure (eg. broken / hijacked browser or system implementations),
   * so make sure to only run this in trusted environments.
   *
   * [`getrandom`]: https://docs.rs/getrandom/0.2.1/getrandom/
   */
  constructor();
  /**
   * Derives a key pair from a private key (encoded as hex string for better handling in browser
   * contexts).
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
   * Returns the public half of the key pair.
   * @returns {Uint8Array}
   */
  publicKeyBytes(): Uint8Array;
  /**
   * Returns the private half of the key pair.
   * @returns {Uint8Array}
   */
  privateKeyBytes(): Uint8Array;
  /**
   * Sign a message using this key pair.
   * @param {Uint8Array} message
   * @returns {Uint8Array}
   */
  sign(message: Uint8Array): Uint8Array;
  /**
   * Verify a signature for a message.
   * @param {Uint8Array} message
   * @param {Uint8Array} signature
   * @returns {any}
   */
  verify(message: Uint8Array, signature: Uint8Array): any;
}
/**
 * Use `MessageFields` to attach user data to a [`Message`].
 *
 * See [`crate::atomic::MessageFields`] for further documentation.
 */
export class MessageFields {
  free(): void;
  /**
   * Returns a `MessageFields` instance
   */
  constructor();
  /**
   * Adds a new field to this `MessageFields` instance.
   *
   * Only `text` fields are currently supported and no schema validation is being done to make
   * sure that only fields that are part of a schema can be added.
   * @param {string} name
   * @param {any} value
   */
  add(name: string, value: any): void;
  /**
   * Returns this instance formatted for debugging
   * @returns {string}
   */
  toString(): string;
}

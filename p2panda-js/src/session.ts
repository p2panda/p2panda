import { RequestManager, HTTPTransport, Client } from '@open-rpc/client-js';
import debug from 'debug';

import p2panda, { P2Panda } from '~/wasm';
import type { Resolved } from '~/index';
import Instance, { Context } from '~/instance';
import { marshallResponseFields } from '~/utils';

import type { EntryArgs, EntryRecord, EncodedEntry, Fields } from '~/types';
import { KeyPair } from 'wasm-web';

const log = debug('p2panda-js:session');

export default class Session {
  // Address of a p2panda node that we can connect to
  endpoint: string;

  // An rpc client connected to the configured endpoint
  client: Client;

  // The wasm library from p2panda-rs. To ensure that it is loaded before
  // using it await `this.loadWasm()`
  p2panda: Resolved<typeof p2panda> | null = null;

  // Cached arguments for the next entry
  nextEntryArgs: { [cacheKey: string]: EntryArgs } = {};

  constructor(endpoint: Session['endpoint']) {
    if (endpoint == null || endpoint === '') {
      throw new Error('Missing `endpoint` parameter for creating a session');
    }
    this.endpoint = endpoint;
    const transport = new HTTPTransport(endpoint);
    this.client = new Client(new RequestManager([transport]));
  }

  private _schema: string | null = null;

  /**
   * Return currently configured schema.
   *
   * Throws if no schema is configured.
   */
  get schema(): string {
    if (!this._schema) {
      throw new Error(
        'Requires a schema. Configure a schema with ' +
          '`session.schema()` or with the `options` parameter on methods.',
      );
    }
    return this._schema;
  }

  /**
   * Set a fixed schema for this session, which will be used if no other schema
   * is defined through a methods `options` parameter.
   *
   * @param val schema hash
   * @returns Session
   */
  setSchema(val: string): Session {
    this._schema = val;
    return this;
  }

  private _keyPair: KeyPair | null = null;

  get keyPair(): KeyPair {
    if (!this._keyPair) {
      throw new Error(
        'Requires a signing key pair. Configure a key pair with ' +
          '`session.keyPair()` or with the `options` parameter on methods.',
      );
    }
    return this._keyPair;
  }

  /**
   * Set a fixed key pair for this session, which will be used by methods unless
   * a different key pair is configured through their `options` parameters.
   *
   * This does not check the integrity or type of the supplied key pair!
   *
   * @param val key pair instance generated using the `KeyPair` class.
   * @returns key pair instance
   */
  setKeyPair(val: KeyPair): Session {
    this._keyPair = val;
    return this;
  }

  /**
   * Load and return the WebAssembly p2panda library.
   *
   * Always await this function before using `this.p2panda`. Unfortunately this
   * cannot be handled in the constructor as the contructor cannot be async.
   *
   * @returns object p2panda wasm library exports
   */
  async loadWasm(): Promise<P2Panda> {
    if (this.p2panda == null) {
      this.p2panda = await p2panda;
      log('initialized wasm lib');
    } else {
      log('access cached wasm lib');
    }
    return this.p2panda;
  }

  /**
   * Return arguments for constructing the next entry given author and schema.
   *
   * This uses the cache set through `Session._setNextEntryArgs`.
   *
   * @param author public key of the author
   * @param schema schema id
   * @returns an `EntryArgs` object
   */
  async _getNextEntryArgs(author: string, schema: string): Promise<EntryArgs> {
    if (!author || !schema)
      throw new Error('Author and schema must be provided');
    const cacheKey = `${author}/${schema}`;
    const cachedValue = this.nextEntryArgs[cacheKey];
    if (cachedValue) {
      // use cache
      delete this.nextEntryArgs[cacheKey];
      log('call panda_getEntryArguments [cached]', cachedValue);
      return cachedValue;
    } else {
      // do rpc call
      const nextEntryArgs = await this.client.request({
        method: 'panda_getEntryArguments',
        params: { author, schema },
      });
      log('call panda_getEntryArguments', nextEntryArgs);
      return nextEntryArgs;
    }
  }

  /**
   * Cache next entry args for a given author and schema
   *
   * @param author public key of the author
   * @param schema schema id
   * @param entryArgs an object with entry arguments
   */
  _setNextEntryArgs(
    author: string,
    schema: string,
    entryArgs: EntryArgs,
  ): void {
    const cacheKey = `${author}/${schema}`;
    this.nextEntryArgs[cacheKey] = entryArgs;
  }

  /**
   * Publish an encoded entry and message.
   *
   * @param entryEncoded
   * @param messageEncoded
   * @returns
   */
  async _publishEntry(
    entryEncoded: string,
    messageEncoded: string,
  ): Promise<EntryArgs> {
    if (!entryEncoded || !messageEncoded)
      throw new Error('Encoded entry and message must be provided');

    const params = { entryEncoded, messageEncoded };
    log('call panda_publishEntry', params);
    const result = await this.client.request({
      method: 'panda_publishEntry',
      params,
    });
    log('response panda_publishEntry', result);
    return result;
  }

  /**
   * Query node for encoded entries of a given schema.
   *
   * @param schema schema id
   * @returns an array of encoded entries
   */
  async _queryEntriesEncoded(schema: string): Promise<EncodedEntry[]> {
    if (!schema) throw new Error('Schema must be provided');
    const params = { schema };
    log('call panda_queryEntries', params);
    const result = await this.client.request({
      method: 'panda_queryEntries',
      params,
    });
    log('response panda_queryEntries', result);
    return result.entries;
  }

  /**
   * Query node for entries of a given schema and decode entries.
   *
   * Returned entries retain their encoded form on `entry.encoded`.
   *
   * @param schema schema id
   * @returns an array of decoded entries
   */
  async _queryEntries(schema: string): Promise<EntryRecord[]> {
    if (!schema) throw new Error('Schema must be provided');
    const { decodeEntry } = await this.loadWasm();
    const result = await this._queryEntriesEncoded(schema);
    log(`decoding ${result.length} entries`);
    return Promise.all(
      result.map(async (entry) => {
        const decoded = await decodeEntry(entry.entryBytes, entry.payloadBytes);
        decoded.message.fields = marshallResponseFields(decoded.message.fields);
        return {
          ...decoded,
          encoded: entry,
        };
      }),
    );
  }

  // Instance operations

  /**
   * Signs and publishes a `create` entry for the given user data and matching schema.
   *
   * Caches arguments for creating the next entry of this schema in the given session.
   *
   * @param fields user data to publish with the new entry, needs to match schema
   * @param instanceArgs optional config object:
   * @param instanceArgs.keyPair will be used to sign the new entry
   * @param instanceArgs.schema hex-encoded schema id
   * @example
   * const messageFields = {
   *   message: 'ahoy'
   * };
   * await new Session(endpoint)
   *   .setKeyPair(keyPair)
   *   .create(messageFields, { schema });
   */
  async create(fields: Fields, options: Partial<Context>): Promise<Session> {
    log('create instance', fields);
    const mergedOptions = {
      schema: options.schema || this.schema,
      keyPair: options.keyPair || this.keyPair,
      session: this,
    };
    Instance.create(fields, mergedOptions);
    return this;
  }

  async update(): Promise<Session> {
    throw new Error('not implemented');
  }

  async delete(): Promise<Session> {
    throw new Error('not implemented');
  }

  toString(): string {
    const keyPairStr = this._keyPair
      ? ` key pair ${this._keyPair.publicKey().slice(-8)}`
      : '';
    const schemaStr = this._schema ? ` schema ${this.schema.slice(-8)}` : '';
    return `<Session ${this.endpoint}${keyPairStr}${schemaStr}>`;
  }
}

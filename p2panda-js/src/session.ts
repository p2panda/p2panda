import { RequestManager, HTTPTransport, Client } from '@open-rpc/client-js';
import debug from 'debug';

import p2panda, { P2Panda } from '~/wasm';
import type { Resolved } from '~/index';
import Instance, { Context } from '~/instance';
import { marshallResponseFields } from '~/utils';

import type { EntryArgs, EntryRecord, EncodedEntry, Fields } from '~/types';
import { KeyPair } from 'wasm-web';

const log = debug('p2panda-api');

export default class Session {
  // Address of a p2panda node that we can connect to
  endpoint: string;

  // An rpc client connected to the confgiured endpoint
  client: Client;

  // The wasm library from p2panda-rs. To ensure that it is loaded before
  // using it await `this.loadWasm()`
  // @ts-expect-error p2panda must be resolved before accessing it
  p2panda: Resolved<typeof p2panda>;

  // Cached arguments for the next entry
  nextEntryArgs: { [cacheKey: string]: EntryArgs } = {};

  constructor(endpoint: Session['endpoint']) {
    this.endpoint = endpoint;
    const transport = new HTTPTransport(endpoint);
    this.client = new Client(new RequestManager([transport]));
  }

  __schema: string | null = null;

  get _schema(): string {
    if (!this.__schema) {
      throw new Error(
        'Requires a schema. Configure a schema with ' +
          '`session.schema()` or with the `options` parameter on methods.',
      );
    }
    return this._schema;
  }

  set _schema(val: string) {
    this.__schema = val;
  }

  /**
   * Preconfigure a schema
   *
   * @param val schema hash
   * @returns Session
   */
  schema(val: string): Session {
    this._schema = val;
    return this;
  }

  __keyPair: KeyPair | null = null;

  get _keyPair(): KeyPair {
    if (!this.__keyPair) {
      throw new Error(
        'Requires a signing key pair. Configure a key pair with ' +
          '`session.keyPair()` or with the `options` parameter on methods.',
      );
    }
    return this._keyPair;
  }

  set _keyPair(val: KeyPair) {
    this.__keyPair = val;
  }

  keyPair(val: KeyPair): Session {
    this._keyPair = val;
    return this;
  }

  // Load and return the WebAssembly p2panda library.
  //
  // Always await this function before using `this.p2panda`. Unfortunately this
  // cannot be handled in the constructor as the contructor cannot be async.
  async loadWasm(): Promise<P2Panda> {
    if (this.p2panda == null) {
      this.p2panda = await p2panda;

      // I am removing this again because it breaks the node build but it may
      // have to go back in.

      // if (this.p2panda.default != null) {
      //   log('fallback loader from `p2panda.default`');
      //   // @ts-expect-error only applies for node context
      //   this.p2panda = await p2panda.default;
      // } else {
      //   log('loaded wasm lib');
      // }
      log('loaded wasm lib');
    } else {
      log('access cached wasm lib');
    }
    return this.p2panda;
  }

  async getNextEntryArgs(author: string, schema: string): Promise<EntryArgs> {
    if (!author || !schema)
      throw new Error('Author and schema must be provided');
    const cacheKey = `${author}/${schema}`;
    const cachedValue = this.nextEntryArgs[cacheKey];
    if (cachedValue) {
      // use cache
      delete this.nextEntryArgs[cacheKey];
      log('panda_getEntryArguments [cached]', cachedValue);
      return cachedValue;
    } else {
      // do rpc call
      const nextEntryArgs = await this.client.request({
        method: 'panda_getEntryArguments',
        params: { author, schema },
      });
      log('panda_getEntryArguments', nextEntryArgs);
      return nextEntryArgs;
    }
  }

  /**
   * Cache next entry args for a given author and schema
   */
  setNextEntryArgs(author: string, schema: string, entryArgs: EntryArgs): void {
    const cacheKey = `${author}/${schema}`;
    this.nextEntryArgs[cacheKey] = entryArgs;
  }

  async publishEntry(
    entryEncoded: string,
    messageEncoded: string,
  ): Promise<EntryArgs> {
    if (!entryEncoded || !messageEncoded)
      throw new Error('Encoded entry and message must be provided');

    const result = await this.client.request({
      method: 'panda_publishEntry',
      params: { entryEncoded, messageEncoded },
    });
    log('panda_publishEntry');
    return result;
  }

  async queryEntriesEncoded(schema: string): Promise<EncodedEntry[]> {
    if (!schema) throw new Error('Schema must be provided');
    const result = await this.client.request({
      method: 'panda_queryEntries',
      params: { schema },
    });
    log('panda_queryEntries', result);
    return result.entries;
  }

  async queryEntries(schema: string): Promise<EntryRecord[]> {
    if (!schema) throw new Error('Schema must be provided');
    const { decodeEntry } = await this.loadWasm();
    const result = await this.queryEntriesEncoded(schema);
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
   *   .schema(schema)
   *   .keyPair(keyPair)
   *   .create(messageFields);
   */
  async create(fields: Fields, options: Partial<Context>): Promise<Session> {
    Instance.create(fields, {
      schema: this._schema,
      keyPair: this._keyPair,
      session: this,
      ...options,
    });
    return this;
  }

  async update(): Promise<Session> {
    throw new Error('not implemented');
  }

  async delete(): Promise<Session> {
    throw new Error('not implemented');
  }

  toString(): string {
    const schemaStr = this._schema
      ? ` with schema ${this._schema.slice(-8)}`
      : '';
    return `<Session ${this.endpoint}${schemaStr}>`;
  }
}

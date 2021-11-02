// SPDX-License-Identifier: AGPL-3.0-or-later

import { RequestManager, HTTPTransport, Client } from '@open-rpc/client-js';
import debug from 'debug';

import wasm from '~/wasm';
import {
  createInstance,
  deleteInstance,
  updateInstance,
  queryInstances,
} from '~/instance';
import { marshallResponseFields } from '~/utils';

import type {
  EntryArgs,
  EntryRecord,
  EncodedEntry,
  Fields,
  InstanceRecord,
} from '~/types';
import type { KeyPair } from 'wasm';

const log = debug('p2panda-js:session');

export type Context = {
  keyPair: KeyPair;
  schema: string;
  session: Session;
};

/**
 * Communicate with the p2panda network through a `Session` instance
 *
 * `Session` provides a high-level interface to create data in the p2panda
 * network by creating, updating and deleting instances of data schemas. It also
 * provides a low-level api for directly accessing and creating entries on the
 * bamboo append-only log structure.
 *
 * A session is configured with the URL of a p2panda node, which
 * may be running locally or on a remote machine. It is possible to set a fixed
 * key pair and/or data schema for a session by calling `setKeyPair()` and
 * `setSchema()` or you can also configure these through the `options` parameter
 * of methods.
 *
 * Sessions also provide access to the p2panda web assembly library, which is
 * why many functions in `p2panda-js` have a `session` parameter.
 */
export class Session {
  // Address of a p2panda node that we can connect to
  endpoint: string;

  // An rpc client connected to the configured endpoint
  client: Client;

  // Cached arguments for the next entry
  private nextEntryArgs: { [cacheKey: string]: EntryArgs } = {};

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
        'Configure a schema with `session.schema()` or with the `options` ' +
          'parameter on methods.',
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
        'Configure a key pair with `session.keyPair()` or with the `options` ' +
          'parameter on methods.',
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
   * Return arguments for constructing the next entry given author and schema.
   *
   * This uses the cache set through `Session._setNextEntryArgs`.
   *
   * @param author public key of the author
   * @param schema schema id
   * @returns an `EntryArgs` object
   */
  async getNextEntryArgs(author: string, schema: string): Promise<EntryArgs> {
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
  setNextEntryArgs(author: string, schema: string, entryArgs: EntryArgs): void {
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
  async publishEntry(
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
  private async queryEntriesEncoded(schema: string): Promise<EncodedEntry[]> {
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
  async queryEntries(schema: string): Promise<EntryRecord[]> {
    if (!schema) throw new Error('Schema must be provided');
    const { decodeEntry } = await wasm;
    const result = await this.queryEntriesEncoded(schema);
    log(`decoding ${result.length} entries`);
    return Promise.all(
      result.map(async (entry) => {
        const decoded = await decodeEntry(entry.entryBytes, entry.payloadBytes);
        if (decoded.message.action !== 'delete') {
          decoded.message.fields = marshallResponseFields(
            decoded.message.fields,
          );
        }
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
   * @param options optional config object:
   * @param options.keyPair will be used to sign the new entry
   * @param options.schema hex-encoded schema id
   * @example
   * const messageFields = {
   *   message: 'ahoy'
   * };
   * await new Session(endpoint)
   *   .setKeyPair(keyPair)
   *   .create(messageFields, { schema });
   */
  async create(fields: Fields, options?: Partial<Context>): Promise<Session> {
    // We should validate the data against the schema here too eventually
    if (!fields) throw new Error('Message fields must be provided');
    log('create instance', fields);
    const mergedOptions = {
      schema: options?.schema || this.schema,
      keyPair: options?.keyPair || this.keyPair,
      session: this,
    };
    createInstance(fields, mergedOptions);
    return this;
  }

  /**
   * Signs and publishes an `update` entry for the given user data and matching schema.
   * An `update` entry references the entry hash of the `create` entry which is the root
   * of this materialized instance.
   *
   * Caches arguments for creating the next entry of this schema in the given session.
   *
   * @param id the id of the instance we wish to update, this is the hash of the root `create` entry
   * @param fields user data to publish with the new entry, needs to match schema
   * @param options optional config object:
   * @param options.keyPair will be used to sign the new entry
   * @param options.schema hex-encoded schema id
   * @example
   * const instanceId = '0040fd224effd3aa26c2551a380ef9c48a6fae89f388949f24de314027d8ce3e2a5749077afa64a445299ca9528970092a33ef29aa30e5783d958fcee81bed0a197c';
   * const messageFields = {
   *   message: 'ahoy'
   * };
   * await new Session(endpoint)
   *   .setKeyPair(keyPair)
   *   .update(instanceId, messageFields, { schema });
   */
  async update(
    id: string,
    fields: Fields,
    options?: Partial<Context>,
  ): Promise<Session> {
    // We should validate the data against the schema here too eventually
    if (!id) throw new Error('Instance id must be provided');
    if (!fields) throw new Error('Message fields must be provided');
    log('update instance', id, fields);
    const mergedOptions = {
      schema: options?.schema || this.schema,
      keyPair: options?.keyPair || this.keyPair,
      session: this,
    };
    updateInstance(id, fields, mergedOptions);
    return this;
  }

  /**
   * Signs and publishes a `delete` entry for the given schema. References the entry hash of the `create` entry which
   * is the id of this materialized instance.
   *
   * Caches arguments for creating the next entry of this schema in the given session.
   *
   * @param id the id of the instance we wish to update, this is the hash of the root `create` entry
   * @param options optional config object:
   * @param options.keyPair will be used to sign the new entry
   * @param options.schema hex-encoded schema id
   * @example
   * const instanceId = '0040fd224effd3aa26c2551a380ef9c48a6fae89f388949f24de314027d8ce3e2a5749077afa64a445299ca9528970092a33ef29aa30e5783d958fcee81bed0a197c';
   * await new Session(endpoint)
   *   .setKeyPair(keyPair)
   *   .delete(instanceId, { schema });
   */
  async delete(id: string, options?: Partial<Context>): Promise<Session> {
    if (!id) throw new Error('Instance id must be provided');
    log('delete instance', id);
    const mergedOptions = {
      schema: options?.schema || this.schema,
      keyPair: options?.keyPair || this.keyPair,
      session: this,
    };
    deleteInstance(id, mergedOptions);
    return this;
  }

  /**
   * Query data instances of a specific schema from the node
   *
   * Calling this method will retrieve all instances of the given schema from
   * the node and then materialize them locally.
   *
   * @param options optional config object:
   * @param options.schema hex-encoded schema id
   * @returns array of instance records, which have data fields and an extra
   *  `_meta_ field, which holds instance metadata and its entry history
   */
  async query(options?: Partial<Context>): Promise<InstanceRecord[]> {
    log('query schema', options?.schema || this.schema);
    const instances = queryInstances({
      schema: options?.schema || this.schema,
      session: this,
    });
    return instances;
  }

  toString(): string {
    const keyPairStr = this._keyPair
      ? ` key pair ${this._keyPair.publicKey().slice(-8)}`
      : '';
    const schemaStr = this._schema ? ` schema ${this.schema.slice(-8)}` : '';
    return `<Session ${this.endpoint}${keyPairStr}${schemaStr}>`;
  }
}

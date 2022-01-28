// SPDX-License-Identifier: AGPL-3.0-or-later

import { RequestManager, HTTPTransport, Client } from '@open-rpc/client-js';
import debug from 'debug';

import wasm from '~/wasm';
import { createDocument, deleteDocument, updateDocument } from '~/document';
import { queryInstances } from '~/instance';
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
 * Communicate with the p2panda network through a `Session` instance.
 *
 * `Session` provides a high-level interface to create data in the p2panda
 * network by creating, updating and deleting documents following data schemas.
 * It also provides a low-level API for directly accessing and creating
 * entries on the Bamboo append-only log structure.
 *
 * A session is configured with the URL of a p2panda node, which may be running
 * locally or on a remote machine. It is possible to set a fixed key pair
 * and/or data schema for a session by calling `setKeyPair()` and `setSchema()`
 * or you can also configure these through the `options` parameter of
 * methods.
 *
 * Sessions also provide access to the p2panda WebAssembly library, which is
 * why many functions in `p2panda-js` have a `session` parameter.
 */
export class Session {
  // Address of a p2panda node that we can connect to
  endpoint: string;

  // An RPC client connected to the configured endpoint
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
   * Set a fixed key pair for this session, which will be used by methods
   * unless a different key pair is configured through their `options`
   * parameters.
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
   * @param document optional document id
   * @returns an `EntryArgs` object
   */
  async getNextEntryArgs(
    author: string,
    documentId?: string,
  ): Promise<EntryArgs> {
    if (!author) {
      throw new Error('Author must be provided');
    }

    // Use cache only when documentId is set
    if (documentId) {
      const cacheKey = `${author}/${documentId}`;
      const cachedValue = this.nextEntryArgs[cacheKey];

      if (cachedValue) {
        delete this.nextEntryArgs[cacheKey];
        log('call panda_getEntryArguments [cached]', cachedValue);
        return cachedValue;
      }
    }

    // Do RPC call
    const nextEntryArgs = await this.client.request({
      method: 'panda_getEntryArguments',
      params: { author, document: documentId },
    });

    log('call panda_getEntryArguments', nextEntryArgs);
    return nextEntryArgs;
  }

  /**
   * Cache next entry args for a given author and document id.
   *
   * @param author public key of the author
   * @param document document id
   * @param entryArgs an object with entry arguments
   */
  setNextEntryArgs(
    author: string,
    documentId: string,
    entryArgs: EntryArgs,
  ): void {
    const cacheKey = `${author}/${documentId}`;
    this.nextEntryArgs[cacheKey] = entryArgs;
  }

  /**
   * Publish an encoded entry and operation.
   *
   * @param entryEncoded
   * @param operationEncoded
   * @returns next entry arguments
   */
  async publishEntry(
    entryEncoded: string,
    operationEncoded: string,
  ): Promise<EntryArgs> {
    if (!entryEncoded || !operationEncoded) {
      throw new Error('Encoded entry and operation must be provided');
    }

    const params = { entryEncoded, operationEncoded };
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
    if (!schema) {
      throw new Error('Schema must be provided');
    }

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
    if (!schema) {
      throw new Error('Schema must be provided');
    }

    const { decodeEntry } = await wasm;
    const result = await this.queryEntriesEncoded(schema);

    log(`decoding ${result.length} entries`);

    return Promise.all(
      result.map(async (entry) => {
        const decoded = await decodeEntry(entry.entryBytes, entry.payloadBytes);

        if (decoded.operation.action !== 'delete') {
          decoded.operation.fields = marshallResponseFields(
            decoded.operation.fields,
          );
        }

        return {
          ...decoded,
          encoded: entry,
        };
      }),
    );
  }

  // Document operations

  /**
   * Signs and publishes a CREATE operation for the given application data and
   * matching schema.
   *
   * Caches arguments for creating the next entry of this document in the given
   * session.
   *
   * @param fields application data to publish with the new entry, needs to match schema
   * @param options optional config object:
   * @param options.keyPair will be used to sign the new entry
   * @param options.schema hex-encoded schema id
   * @example
   * const operationFields = {
   *   message: 'ahoy'
   * };
   * await new Session(endpoint)
   *   .setKeyPair(keyPair)
   *   .create(operationFields, { schema });
   */
  async create(fields: Fields, options?: Partial<Context>): Promise<Session> {
    // We should validate the data against the schema here too eventually
    if (!fields) {
      throw new Error('Operation fields must be provided');
    }

    log('create document', fields);
    const mergedOptions = {
      schema: options?.schema || this.schema,
      keyPair: options?.keyPair || this.keyPair,
      session: this,
    };
    createDocument(fields, mergedOptions);

    return this;
  }

  /**
   * Signs and publishes an UPDATE operation for the given application data and
   * matching schema. An UPDATE operation references the entry hash of the
   * CREATE operation which is the root of this document.
   *
   * Caches arguments for creating the next entry of this schema in the given
   * session.
   *
   * @param documentId id of the document we update, this is the hash of the root `create` entry
   * @param fields application data to publish with the new entry, needs to match schema
   * @param previousOperations array of operation hash ids identifying the tips of all currently un-merged branches in the document graph
   * @param options optional config object:
   * @param options.keyPair will be used to sign the new entry
   * @param options.schema hex-encoded schema id
   * @example
   * const documentId = '00200cf84048b0798942deba7b1b9fcd77ca72876643bd3fedfe612d4c6fb60436be';
   * const operationFields = {
   *   message: 'ahoy',
   * };
   * const previousOperations = [
   *   '00203341c9dd226525886ee77c95127cd12f74366703e02f9b48f3561a9866270f07',
   * ];
   * await new Session(endpoint)
   *   .setKeyPair(keyPair)
   *   .update(documentId, operationFields, previousOperations, { schema });
   */
  async update(
    documentId: string,
    fields: Fields,
    previousOperations: string[],
    options?: Partial<Context>,
  ): Promise<Session> {
    // We should validate the data against the schema here too eventually
    if (!documentId) {
      throw new Error('Document id must be provided');
    }

    if (!fields) {
      throw new Error('Operation fields must be provided');
    }

    log('update document', documentId, fields);
    const mergedOptions = {
      schema: options?.schema || this.schema,
      keyPair: options?.keyPair || this.keyPair,
      session: this,
    };
    updateDocument(documentId, previousOperations, fields, mergedOptions);

    return this;
  }

  /**
   * Signs and publishes a DELETE operation for the given schema. References
   * the entry hash of the CREATE operation which is the id of this document.
   *
   * Caches arguments for creating the next entry of this schema in the given session.
   *
   * @param documentId id of the document we delete, this is the hash of the root `create` entry
   * @param previousOperations array of operation hash ids identifying the tips of all currently un-merged branches in the document graph
   * @param options optional config object:
   * @param options.keyPair will be used to sign the new entry
   * @param options.schema hex-encoded schema id
   * @example
   * const documentId = '00200cf84048b0798942deba7b1b9fcd77ca72876643bd3fedfe612d4c6fb60436be';
   * const previousOperations = [
   *   '00203341c9dd226525886ee77c95127cd12f74366703e02f9b48f3561a9866270f07',
   * ];
   * await new Session(endpoint)
   *   .setKeyPair(keyPair)
   *   .delete(documentId, previousOperations, { schema });
   */
  async delete(
    documentId: string,
    previousOperations: string[],
    options?: Partial<Context>,
  ): Promise<Session> {
    if (!documentId) {
      throw new Error('Document id must be provided');
    }

    log('delete document', documentId);
    const mergedOptions = {
      schema: options?.schema || this.schema,
      keyPair: options?.keyPair || this.keyPair,
      session: this,
    };
    deleteDocument(documentId, previousOperations, mergedOptions);

    return this;
  }

  /**
   * Query documents of a specific schema from the node.
   *
   * Calling this method will retrieve all entries of the given schema from the
   * node and then materialise them locally into instances.
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

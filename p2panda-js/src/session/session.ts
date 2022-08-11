// SPDX-License-Identifier: AGPL-3.0-or-later

import debug from 'debug';
import {
  ApolloClient,
  ApolloClientOptions,
  gql,
  HttpLink,
  InMemoryCache,
  NormalizedCacheObject,
  // Import from `client/core` to not require `react` as a dependency as long
  // as that is possible.
} from '@apollo/client/core';
import fetch from 'node-fetch';

import { createDocument, deleteDocument, updateDocument } from '~/document';

import type { NextArgs, Fields, SchemaId } from '~/types';
import type { KeyPair } from 'wasm';

const log = debug('p2panda-js:session');

export type Context = {
  keyPair: KeyPair;
  schema: SchemaId;
  session: Session;
};

type NextArgsVariables = {
  publicKey: string;
  viewId?: string;
};

// GraphQL query to retrieve next entry args from node.
// @TODO: Query `nextEntryArgs` is deprecated and will be replaced by `nextArgs` soon
export const GQL_NEXT_ARGS = gql`
  query NextArgs($publicKey: String!, $viewId: String) {
    nextEntryArgs(publicKey: $publicKey, documentId: $viewId) {
      logId
      seqNum
      backlink
      skiplink
    }
  }
`;

type PublishVariables = {
  entry: string;
  operation: string;
};

// GraphQL mutation to publish an entry and retrieve arguments for encoding the
// next operation on the same document (those are currently not used to update
// the next entry arguments cache).
// @TODO: Query `publishEntry` is deprecated and will be replaced by `publish` soon
export const GQL_PUBLISH = gql`
  mutation Publish($entry: String!, $operation: String!) {
    publishEntry(entry: $entry, operation: $operation) {
      logId
      seqNum
      backlink
      skiplink
    }
  }
`;

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

  // A GraphQL client connected to the configured endpoint
  client: ApolloClient<NormalizedCacheObject>;

  // Cached arguments for the next entry
  private nextArgs: { [cacheKey: string]: NextArgs } = {};

  constructor(
    endpoint: Session['endpoint'],
    apolloOptions?: ApolloClientOptions<NormalizedCacheObject>,
  ) {
    if (endpoint == null || endpoint === '') {
      throw new Error('Missing `endpoint` parameter for creating a session');
    }
    this.endpoint = endpoint;
    this.client = new ApolloClient({
      // @ts-expect-error using a fetch implementation that ts doesn't consider
      // valid
      link: new HttpLink({ uri: endpoint, fetch }),
      cache: new InMemoryCache(),
      ...apolloOptions,
    });
  }

  private _schema: SchemaId | null = null;

  /**
   * Return currently configured schema.
   *
   * Throws if no schema is configured.
   */
  get schema(): SchemaId {
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
   * @param val schema id
   * @returns Session
   */
  setSchema(val: SchemaId): Session {
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
   * @param publicKey public key of the author
   * @param viewId optional document view id
   * @returns an `EntryArgs` object
   */
  async getNextArgs(publicKey: string, viewId?: string): Promise<NextArgs> {
    if (!publicKey) {
      throw new Error("Author's public key must be provided");
    }

    const variables: NextArgsVariables = {
      publicKey,
    };

    // Use cache only when viewId is set
    if (viewId) {
      const cacheKey = `${publicKey}/${viewId}`;
      const cachedValue = this.nextArgs[cacheKey];

      if (cachedValue) {
        delete this.nextArgs[cacheKey];
        log('request nextArgs [cached]', cachedValue);
        return cachedValue;
      }

      variables.viewId = viewId;
    }

    try {
      // @TODO: Query `nextEntryArgs` is deprecated and will be replaced by `nextArgs` soon
      const { data } = await this.client.query<
        { nextEntryArgs: NextArgs },
        NextArgsVariables
      >({
        query: GQL_NEXT_ARGS,
        variables,
      });
      const nextArgs = data.nextEntryArgs;
      log('request nextArgs', nextArgs);
      return nextArgs;
    } catch (err) {
      log('Error fetching nextArgs');
      throw err;
    }
  }

  /**
   * Cache next entry args for a given author and document id.
   *
   * @param publicKey public key of the author
   * @param viewId document id
   * @param nextArgs an object with entry arguments
   */
  setNextArgs(publicKey: string, viewId: string, nextArgs: NextArgs): void {
    const cacheKey = `${publicKey}/${viewId}`;
    this.nextArgs[cacheKey] = nextArgs;
  }

  /**
   * Publish an encoded entry and operation.
   *
   * @param entry
   * @param operation
   * @returns next entry arguments
   */
  async publish(entry: string, operation: string): Promise<NextArgs> {
    if (!entry || !operation) {
      throw new Error('Encoded entry and operation must be provided');
    }

    const variables: PublishVariables = {
      entry,
      operation,
    };

    try {
      // @TODO: Query `publishEntry` is deprecated and will be replaced by `publish` soon
      const { data } = await this.client.mutate<
        { publishEntry: NextArgs },
        PublishVariables
      >({
        mutation: GQL_PUBLISH,
        variables,
      });
      log('request publishEntry', data);
      if (data?.publishEntry == null)
        throw new Error("Response doesn't contain field `publishEntry`");
      return data.publishEntry;
    } catch (err) {
      log('Error publishing entry');
      throw err;
    }
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
   * matching schema.
   *
   * The document to be updated is referenced by its document id, which is the
   * operation id of that document's initial `CREATE` operation.
   *
   * Caches arguments for creating the next entry of this schema in the given
   * session.
   *
   * @param documentId id of the document we update, this is the id of the root `create` operation
   * @param fields application data to publish with the new entry, needs to match schema
   * @param previousOperations array of operation ids identifying the tips of all currently un-merged branches in the document graph
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
   * Signs and publishes a DELETE operation for the given schema.
   *
   * The document to be deleted is referenced by its document id, which is the
   * operation id of that document's initial `CREATE` operation.
   *
   * Caches arguments for creating the next entry of this schema in the given session.
   *
   * @param documentId id of the document we delete, this is the hash of the root `create` entry
   * @param previousOperations array of operation ids identifying the tips of all currently un-merged branches in the document graph
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

  toString(): string {
    const keyPairStr = this._keyPair
      ? ` key pair ${this._keyPair.publicKey().slice(-8)}`
      : '';
    const schemaStr = this._schema ? ` schema ${this.schema.slice(-8)}` : '';
    return `<Session ${this.endpoint}${keyPairStr}${schemaStr}>`;
  }
}

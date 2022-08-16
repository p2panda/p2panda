import { marshallResponseFields } from '../src/utils';

import type {
  EncodedEntry,
  Entry,
  NextArgs,
  FieldsTagged,
  Operation,
  OperationTagged,
  OperationValue,
  SchemaId,
} from '../src/types';

import TEST_DATA from './test-data.json';

// Right now we only have one author `panda` who only has one schema log. This
// could be expanded in the future.
const encodedEntries = TEST_DATA.panda.logs[0].encodedEntries;
const nextEntryArgs = TEST_DATA.panda.logs[0].nextEntryArgs;

// Convert JSON-imported operations to use `Map`s instead of objects for
// reporesenting operation fields.
const decodedOperations = TEST_DATA.panda.logs[0].decodedOperations.map(
  (operation) => {
    if (operation.fields) {
      const fields: FieldsTagged = new Map();

      Object.entries(operation.fields).forEach(([key, value]) => {
        // @TODO: This only works with strings currently, it needs refactoring
        // as soon as we have schemas
        fields.set(key, {
          value,
          type: 'str',
        } as OperationValue);
      });

      // assert the type of the JSON-imported `fields` as `unknown` so that
      // Typescript allows writing our new `Map`-based value to it
      (operation.fields as unknown) = fields;
    }

    // also asserting the type as unknown to be able to change it to the correct
    // return value type
    return operation as unknown as OperationTagged;
  },
);

export const schemaFixture = (): SchemaId => {
  return decodedOperations[0].schema;
};

export const documentIdFixture = (): string => {
  return encodedEntries[0].entryHash;
};

/**
 * Return an object with fields for an author's public and private key.
 */
export const authorFixture = (): { publicKey: string; privateKey: string } => {
  const author = {
    publicKey: TEST_DATA.panda.publicKey,
    privateKey: TEST_DATA.panda.privateKey,
  };
  return author;
};

/**
 * Return an Entry given a sequence number in the testing log.
 */
export const entryFixture = (seqNum: number): Entry => {
  const index = seqNum - 1;

  let fields = undefined;
  if (decodedOperations[index].action !== 'delete') {
    fields = marshallResponseFields(
      decodedOperations[index].fields as FieldsTagged,
    );
  }

  const operation: Operation = {
    action: decodedOperations[index].action as Operation['action'],
    schema: decodedOperations[index].schema,
    fields: fields,
  };

  if (decodedOperations[index].previous_operations) {
    operation.previous_operations =
      decodedOperations[index].previous_operations;
  }

  const entry: Entry = {
    backlink: nextEntryArgs[index].backlink as string | undefined,
    skiplink: nextEntryArgs[index].skiplink as string | undefined,
    seqNum: BigInt(nextEntryArgs[index].seqNum),
    logId: BigInt(nextEntryArgs[index].logId),
    operation,
  };

  return entry;
};

/**
 * Return an encoded entry given a sequence number on the mock log.
 */
export const encodedEntryFixture = (seqNum: number): EncodedEntry => {
  const index = seqNum - 1;

  const encodedEntry: EncodedEntry = {
    author: TEST_DATA.panda.publicKey,
    entryBytes: encodedEntries[index].entryBytes,
    entryHash: encodedEntries[index].entryHash,
    logId: BigInt(encodedEntries[index].logId),
    payloadBytes: encodedEntries[index].payloadBytes,
    payloadHash: encodedEntries[index].payloadHash,
    seqNum: BigInt(encodedEntries[index].seqNum),
  };

  return encodedEntry;
};

/**
 * Return arguments for creating an entry.
 *
 * Takes a `seqNum` parameter, which is the sequence number of
 * the entry preceding the one we want arguments for.
 */
export const entryArgsFixture = (seqNum: number): NextArgs => {
  const index = seqNum - 1;

  const entryArgs: NextArgs = {
    backlink: (nextEntryArgs[index].backlink || null) as string | undefined,
    skiplink: (nextEntryArgs[index].skiplink || null) as string | undefined,
    seqNum: nextEntryArgs[index].seqNum,
    logId: nextEntryArgs[index].logId,
  };

  return entryArgs;
};

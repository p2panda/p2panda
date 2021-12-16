// SPDX-License-Identifier: AGPL-3.0-or-later

import debug from 'debug';

import { EntryRecord, InstanceRecord } from '~/types';

const log = debug('p2panda-js:operation');

/**
 * Create a record of data instances by parsing a series of p2panda log entries
 *
 * @param entries entry records from node
 * @returns records of the instance's data and metadata
 */
export const materializeEntries = (
  entries: EntryRecord[],
): { [instanceId: string]: InstanceRecord } => {
  const instances: { [instanceId: string]: InstanceRecord } = {};
  entries.sort((a, b) => a.seqNum - b.seqNum);
  log(`Materialising ${entries.length} entries`);
  for (const entry of entries) {
    if (entry.operation == null) continue;

    let instanceId: string;

    // Set the instanceId
    if (entry.operation.action === 'create') {
      instanceId = entry.encoded.entryHash;
    } else {
      instanceId = entry.operation.id as string;
    }

    const author = entry.encoded.author;
    const schema = entry.operation.schema;

    if (instances[instanceId] && instances[instanceId].deleted) continue;

    let updated: InstanceRecord;

    switch (entry.operation.action) {
      case 'create':
        instances[instanceId] = {
          ...entry.operation.fields,
          _meta: {
            id: instanceId,
            author,
            deleted: false,
            edited: false,
            entries: [entry],
            schema,
          },
        };
        break;

      case 'update':
        updated = {
          ...instances[instanceId],
          ...entry.operation.fields,
        };
        // In that case this key wouldn't exist yet.
        updated._meta.edited = true;
        updated._meta.entries.push(entry);
        instances[instanceId] = updated;
        break;

      case 'delete':
        // Same as above
        updated = { _meta: instances[instanceId]._meta };
        updated._meta.deleted = true;
        updated._meta.entries.push(entry);
        instances[instanceId] = updated;
        break;
      default:
        throw new Error('Unhandled mesage action');
    }
  }
  log(`Materialisation yields ${Object.keys(instances).length} instances`);
  return instances;
};

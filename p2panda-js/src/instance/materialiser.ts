// SPDX-License-Identifier: AGPL-3.0-or-later

import debug from 'debug';

import { EntryRecord, InstanceRecord } from '~/types';

const log = debug('p2panda-js:message');

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
    // If the message is null we continue, but what does that mean?
    // The payload was deleted?
    if (entry.message == null) continue;

    // This is the hash of this entry, but not necesarilly the instance id
    const entryHash = entry.encoded.entryHash;
    const author = entry.encoded.author;
    const schema = entry.message.schema;

    // Here we check if this entryHash exists as a key in instances
    // and if it exists and was deleted then we continue
    if (instances[entryHash] && instances[entryHash].deleted) continue;

    let updated: InstanceRecord;

    // Check the message action
    switch (entry.message.action) {
      // If this is a create message, create a new instance using the
      // entryHash as the key and set message field and _meta values
      case 'create':
        instances[entryHash] = {
          ...entry.message.fields,
          _meta: {
            author,
            deleted: false,
            edited: false,
            entries: [entry],
            hash: entryHash,
            schema,
          },
        };
        break;

      case 'update':
        // If this is an update message we want to retrieve the instance
        // by it's entryHash key. If I'm understanding this correctly though
        // the value of entryHash on this iteration won't be what we need.
        // We need the id (instance id) attached to the Message.
        updated = {
          ...instances[entryHash],
          ...entry.message.fields,
        };
        // In that case this key wouldn't exist yet.
        updated._meta.edited = true;
        updated._meta.entries.push(entry);
        instances[entryHash] = updated;
        break;

      case 'delete':
        // Same as above
        updated = { _meta: instances[entryHash]._meta };
        updated._meta.deleted = true;
        updated._meta.entries.push(entry);
        instances[entryHash] = updated;
        break;
      default:
        throw new Error('Unhandled mesage action');
    }
  }
  log(`Materialisation yields ${Object.keys(instances).length} instances`);
  return instances;
};

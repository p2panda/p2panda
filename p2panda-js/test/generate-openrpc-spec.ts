// SPDX-License-Identifier: AGPL-3.0-or-later

import fs from 'fs';

import $RefParser from '@apidevtools/json-schema-ref-parser';

import openrpcSchema from './openrpc-template.json';

$RefParser.dereference(openrpcSchema, (err, schema) => {
  if (err) {
    throw err;
  } else {
    // `schema` is just a normal JavaScript object that contains your entire
    // JSON Schema, including referenced files, combined into a single object
    fs.writeFile('openrpc.json', JSON.stringify(schema, null, 4), (err) => {
      if (err) {
        throw err;
      }
    });
  }
});

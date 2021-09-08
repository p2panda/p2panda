// SPDX-License-Identifier: AGPL-3.0-or-later

import openrpcSchema from './openrpc-template.json';
import $RefParser from '@apidevtools/json-schema-ref-parser';
import fs from 'fs';

$RefParser.dereference(openrpcSchema, (err, schema) => {
  if (err) {
    console.error(err);
  } else {
    // `schema` is just a normal JavaScript object that contains your entire JSON Schema,
    // including referenced files, combined into a single object

    fs.writeFile('openrpc.json', JSON.stringify(schema), (err) => {
      if (err) {
        throw err;
      }
    });
  }
});

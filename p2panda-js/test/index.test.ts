import { expect } from 'chai';

import p2panda from '../lib';

describe('KeyPair', () => {
  it('creates a key pair', async () => {
    const { KeyPair } = await p2panda;
    const keyPair = new KeyPair();
    expect(keyPair.privateKey().length).to.eq(64);
  });

  it('restores a key pair', async () => {
    const { KeyPair } = await p2panda;
    const keyPair = new KeyPair();
    const keyPairSecond = KeyPair.fromPrivateKey(keyPair.privateKey());
    expect(keyPair.publicKey()).to.eq(keyPairSecond.publicKey());
  });

  it('signs and validates', async () => {
    const { KeyPair } = await p2panda;
    const keyPair = new KeyPair();
    const message = 'hello panda';
    const sig = keyPair.sign(message);
    expect(keyPair.verify(message, sig)).to.be.true;
    expect(keyPair.verify('hello aquadoggo', sig)).to.be.false;
  });
});

describe('MessageFields', () => {
  it('stores and returns the right fields', async () => {
    const { MessageFields } = await p2panda;

    const fields = new MessageFields();
    fields.addText('description', 'Hello, Panda');
    fields.addInteger('temperature', 23);

    // Returns the correct fields
    expect(fields.get('description')).to.eq('Hello, Panda');
    expect(fields.get('temperature')).to.eq(23);

    // Returns the correct length
    expect(fields.length()).to.eq(2);

    // Return nothing when field does not exist
    expect(fields.get('message')).to.eq(null);
  });

  it('throws an error when removing an inexistent field', async () => {
    const { MessageFields } = await p2panda;

    const fields = new MessageFields();
    expect(() => fields.remove('test')).to.throw();
  });
});

describe('Entries', () => {
  it('creates, signs and decodes an entry', async () => {
    const TEST_SCHEMA =
      '0040cf94f6d605657e90c543b0c919070cdaaf7209c5e1ea58acb8f3568fa2114268dc9ac3bafe12af277d286fce7dc59b7c0c348973c4e9dacbe79485e56ac2a702';
    const LOG_ID = 5;
    const SEQ_NUM = 1;

    const {
      KeyPair,
      MessageFields,
      decodeEntry,
      encodeCreateMessage,
      signEncodeEntry,
    } = await p2panda;

    // Generate new key pair
    const keyPair = new KeyPair();

    // Create message
    const fields = new MessageFields();
    fields.addText('description', 'Hello, Panda');
    expect(fields.get('description')).to.eq('Hello, Panda');

    const messageEncoded = encodeCreateMessage(TEST_SCHEMA, fields);

    // Sign and encode entry
    const { entryEncoded, entryHash } = signEncodeEntry(
      keyPair,
      messageEncoded,
      null,
      null,
      SEQ_NUM,
      LOG_ID,
    );

    expect(entryHash.length).to.eq(132);

    // Decode entry and return as JSON
    const decodedEntry = decodeEntry(entryEncoded, messageEncoded);

    expect(decodedEntry.entryHashBacklink).to.be.null;
    expect(decodedEntry.entryHashSkiplink).to.be.null;
    expect(decodedEntry.logId).to.eq(LOG_ID);
    expect(decodedEntry.message.action).to.eq('create');
    expect(decodedEntry.message.schema).to.eq(TEST_SCHEMA);
    expect(decodedEntry.message.fields.description.Text).to.eq('Hello, Panda');

    // Test decoding entry without message
    expect(() => decodeEntry(entryEncoded)).not.to.throw();
  });
});

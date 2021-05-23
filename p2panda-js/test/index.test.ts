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
    const TEST_SCHEMA =
      '0040cf94f6d605657e90c543b0c919070cdaaf7209c5e1ea58acb8f3568fa2114268dc9ac3bafe12af277d286fce7dc59b7c0c348973c4e9dacbe79485e56ac2a702';

    const { MessageFields } = await p2panda;
    const fields = new MessageFields();

    // Set fields of all possible types
    fields.add('description', 'text', 'Hello, Panda');
    fields.add('temperature', 'integer', 23);
    fields.add('isCute', 'boolean', true);
    fields.add('degree', 'float', 12.322);
    fields.add('username', 'relation', TEST_SCHEMA);

    // Returns the correct fields
    expect(fields.get('description')).to.eq('Hello, Panda');
    expect(fields.get('temperature')).to.eq(23);
    expect(fields.get('isCute')).to.eq(true);
    expect(fields.get('degree')).to.eq(12.322);
    expect(fields.get('username')).to.eq(TEST_SCHEMA);

    // Return nothing when field does not exist
    expect(fields.get('message')).to.eq(null);
  });

  it('returns the correct length', async () => {
    const { MessageFields } = await p2panda;
    const fields = new MessageFields();
    expect(fields.length()).to.eq(0);
    fields.add('message', 'text', 'Good morning');
    expect(fields.length()).to.eq(1);
    fields.remove('message');
    expect(fields.length()).to.eq(0);
  });

  it('throws when trying to set a field twice', async () => {
    const { MessageFields } = await p2panda;
    const fields = new MessageFields();
    fields.add('description', 'text', 'Good morning, Panda');
    expect(() =>
      fields.add('description', 'text', 'Good night, Panda'),
    ).to.throw('field already exists');
  });

  it('throws when using invalid types or values', async () => {
    const { MessageFields } = await p2panda;
    const fields = new MessageFields();

    // Throw when type is invalid
    expect(() => fields.add('test', 'lulu', true)).to.throw(
      'Unknown type value',
    );
    expect(() => fields.add('test', 'integer', true)).to.throw(
      'Invalid integer value',
    );

    // Throw when relation is an invalid hash
    expect(() => fields.add('contact', 'relation', 'test')).to.throw(
      'invalid hex encoding in hash string',
    );
  });

  it('throws when removing an inexistent field', async () => {
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
    fields.add('description', 'text', 'Hello, Panda');
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
    expect(decodedEntry.message.fields.description.value).to.eq('Hello, Panda');
    expect(decodedEntry.message.fields.description.type).to.eq('str');

    // Test decoding entry without message
    expect(() => decodeEntry(entryEncoded)).not.to.throw();
  });
});

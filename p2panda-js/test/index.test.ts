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

describe('Entries', () => {
  it('creates, signs and decodes an entry', async () => {
    const TEST_SCHEMA =
      '0040cf94f6d605657e90c543b0c919070cdaaf7209c5e1ea58acb8f3568fa2114268dc9ac3bafe12af277d286fce7dc59b7c0c348973c4e9dacbe79485e56ac2a702';
    const LOG_ID = 5;
    const SEQ_NO = 1;

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
    fields.add('description', 'Hello, Panda');

    const messageEncoded = encodeCreateMessage(TEST_SCHEMA, fields);

    // Sign and encode entry
    const { entryEncoded, entryHash } = signEncodeEntry(
      keyPair,
      messageEncoded,
      null,
      null,
      SEQ_NO,
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

import { expect } from 'chai';

import { Session } from '../lib';

// const PRIVATE_KEY =
//   '4c21b14046f284f87f1ea4be4b973664221ad483079a68ed35a6812553b41176';

const PUBLIC_KEY =
  '2deee1fac104aaac753377bbc2ff70fa5d5154abdac4b4577392db88de6a9a49';

const SCHEMA =
  '00401d76566758a5b6bfc561f1c936d8fc86b5b42ea22ab1dabf40d249d27dd906401fde147e53f44c103dd02a254916be113e51de1077a946a3a0c1272b9b348437';

describe('Session', () => {
  it('can query entries', async () => {
    const session = new Session('http://localhost:2020');
    const entries = await session.queryEntries(SCHEMA);
    expect(entries.length).to.equal(2);
  });

  it('throws when querying without a schema', async () => {
    const session = new Session('http://localhost:2020');
    let error;
    try {
      await session.queryEntries();
    } catch (e) {
      error = e;
    }
    expect(error.message).to.equal('Schema must be provided');
  });

  it('gets next entry args', async () => {
    const session = new Session('http://localhost:2020');
    const nextEntryArgs = await session.getNextEntryArgs(PUBLIC_KEY, SCHEMA);
    expect(nextEntryArgs.entryHashSkiplink).to.equal('SKIPLINK_HASH');
    expect(nextEntryArgs.entryHashBacklink).to.equal('BACKLINK_HASH');
    expect(nextEntryArgs.seqNum).to.equal(3);
    expect(nextEntryArgs.logId).to.equal(1);
  });

  it('can publish entries', async () => {
    const ENTRY_ENCODED =
      '00ba07a8da75dd2f922d62eae7e7ac7c081e06bf0c192b2d8ea1b2ab5e9c59013e0101e700400f2338e42f03bfaab4b721949ad355d964ace34aad4d461ba2b1b42706ada938fe126e189c4b6332f891ee038cf0e9e6723d4911632497683a56dd75c8f6d89f8b411f3bd3f5c8c8114693d7c9de5672f4146ec4acb9bc357015e60bc7425874dd4158bdbdf482bfd5779eb0bc2bf4b4d068a4f7b23c8a03d75c7c61a4fba609';
    const MESSAGE_ENCODED =
      'a466616374696f6e6663726561746566736368656d6178843030343063663934663664363035363537653930633534336230633931393037306364616166373230396335653165613538616362386633353638666132313134323638646339616333626166653132616632373764323836666365376463353962376330633334383937336334653964616362653739343835653536616332613730326776657273696f6e01666669656c6473a26464617465a164546578747818323032312d30352d31335431373a32303a31352e3133375a676d657373616765a164546578746648656c6c6f21';
    const session = new Session('http://localhost:2020');
    const nextEntryArgs = await session.publishEntry(
      ENTRY_ENCODED,
      MESSAGE_ENCODED,
    );
    expect(nextEntryArgs.entryHashSkiplink).to.equal('SKIPLINK_HASH');
  });

  it('caches next entry args');

  it('throws when publishing without a key pair');

  it('throws without a configured endpoint');
});

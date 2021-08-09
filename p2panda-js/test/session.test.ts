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
      '00ba07a8da75dd2f922d62eae7e7ac7c081e06bf0c192b2d8ea1b2ab5e9c59013e01040040944b4ae2ff31d0adc13cf94ba43b766871b4e56e96d0eebbc1b9e2b8226d448e8bc1f9507a21894579578491ff778a008688c2a3e8a409fc37522d9eabaa114c004054f65f3ac2ccf13f5862eb7c29ac20e830e173d062416dfd03a27e8a2315b69f402cfa4ca741d243b184b1d8ff203cf1f1ec4619f44758263f19a75a3537e780ee00408960c9d4f864aef757d440bc5aa5a5c0d726312eddadad68f25d06fedd10f755d51a87565972f8c3d77ef7ac66531227131b0d8857fef749c3a98cfffae8519d1e8bdb78a27348232671acda6c16aca26148642b0e803e6e2e4dfc01ca0d46ea19546be7b4302b826363a6caa28fced7ef9fd847b35a49eb67b885d65af14305';
    const MESSAGE_ENCODED =
      'a466616374696f6e6663726561746566736368656d6178843030343063663934663664363035363537653930633534336230633931393037306364616166373230396335653165613538616362386633353638666132313134323638646339616333626166653132616632373764323836666365376463353962376330633334383937336334653964616362653739343835653536616332613730326776657273696f6e01666669656c6473a26464617465a164546578747818323032312d30352d30325432303a30363a34352e3433305a676d657373616765a164546578746d477574656e204d6f7267656e21';
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

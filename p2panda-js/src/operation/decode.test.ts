// SPDX-License-Identifier: AGPL-3.0-or-later

import { TEST_SCHEMA_ID } from '../wasm.test';
import { decodeOperation } from './';

describe('decodeOperation', () => {
  it('decodes an encoded operation', () => {
    const hash1 =
      '0020da3590fbac19a90bda5618dbcd1799ce6e3bf6e3cd74b7cd41d5d4cb4077af55';
    const hash2 =
      '002020e572c9e4cb884754c047d3d6fec0ff9e700e446cb5f62167575475f7e31bd2';
    const hash3 =
      '00206ebac2127506d3855abc76316299534ee1f695c52e6ac3ae105004b3b968a341';
    const hash4 =
      '002049ed4a0a6cb7308ec13c029e1b559bb1cddccd5c40710cbead900d2fa2ee86c2';
    const hash5 =
      '00206ec876ee8e56acc1e2d5d0d3390f1b02cb4807ade58ca71e885c3943e5287b96';
    const hash6 =
      '0020cf369d24676ba5ae8e74f259f9682607e3e6d01047e31b2b53d3a1cf5f31722e';
    const hash7 =
      '0020fc29afb2f0620bf7417fda043dd13b8e2ef60a47b3f99f47bf8019f68c17411e';

    const encodedOperation =
      '8401007849746573745f30303230646463393961636137373664663063613964' +
      '3162353837316261333964346564616363373532613061333432366231326333' +
      '3935383937316236633834376163a961616d48656c6c6f2c2050616e64612161' +
      '62187b6163fb402899999999999a6164f5616544000102036166784430303230' +
      '6461333539306662616331396139306264613536313864626364313739396365' +
      '3665336266366533636437346237636434316435643463623430373761663535' +
      '6167827844303032303230653537326339653463623838343735346330343764' +
      '3364366665633066663965373030653434366362356636323136373537353437' +
      '3566376533316264327844303032303665626163323132373530366433383535' +
      '6162633736333136323939353334656531663639356335326536616333616531' +
      '3035303034623362393638613334316168817844303032303439656434613061' +
      '3663623733303865633133633032396531623535396262316364646363643563' +
      '3430373130636265616439303064326661326565383663326169828178443030' +
      '3230366563383736656538653536616363316532643564306433333930663162' +
      '3032636234383037616465353863613731653838356333393433653532383762' +
      '3936827844303032306366333639643234363736626135616538653734663235' +
      '3966393638323630376533653664303130343765333162326235336433613163' +
      '6635663331373232657844303032306663323961666232663036323062663734' +
      '3137666461303433646431336238653265663630613437623366393966343762' +
      '663830313966363863313734313165';

    const result = decodeOperation(encodedOperation);

    expect(result.action).toBe('create');
    expect(result.version).toBe(1);
    expect(result.schemaId).toBe(TEST_SCHEMA_ID);
    expect(result.previous).toBeUndefined;
    expect(result.fields?.get('a')).toEqual('Hello, Panda!');
    expect(result.fields?.get('b')).toEqual(BigInt(123));
    expect(result.fields?.get('c')).toEqual(12.3);
    expect(result.fields?.get('d')).toEqual(true);
    expect(result.fields?.get('e')).toEqual(new Uint8Array([0, 1, 2, 3]));
    expect(result.fields?.get('f')).toEqual(hash1);
    expect(result.fields?.get('g')).toEqual([hash2, hash3]);
    expect(result.fields?.get('h')).toEqual([hash4]);
    expect(result.fields?.get('i')).toEqual([[hash5], [hash6, hash7]]);
  });

  it('throws when decoding an invalid operation', () => {
    // Invalid schema id
    expect(() => {
      decodeOperation(
        '84010063626C61A1646E616D6570416D616E697461206361657361726561',
      );
    }).toThrow(
      "Could not decode operation: malformed schema id `bla`: doesn't contain an underscore",
    );
  });
});

import { KeyPair } from 'wasm/index.d.ts';

export interface P2Panda {
  KeyPair: KeyPair;
}

declare const p2panda: Promise<P2Panda>;

export default p2panda;
